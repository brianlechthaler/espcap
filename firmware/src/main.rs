use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::sys::{
    ble_gap_disc, ble_gap_disc_params, ble_gap_event, ble_hs_cfg, esp_timer_get_time,
    esp_wifi_set_channel, esp_wifi_set_promiscuous, esp_wifi_set_promiscuous_filter,
    esp_wifi_set_promiscuous_rx_cb, nimble_port_freertos_deinit, nimble_port_freertos_init,
    nimble_port_init, nimble_port_run, wifi_promiscuous_filter_t, wifi_promiscuous_pkt_t,
    wifi_second_chan_t_WIFI_SECOND_CHAN_NONE, EspError, BLE_ADDR_PUBLIC, BLE_GAP_EVENT_DISC,
    BLE_GAP_EVENT_DISC_COMPLETE, BLE_GAP_EVENT_EXT_DISC,
};
use esp_idf_svc::wifi::{ClientConfiguration, Configuration, EspWifi};
use espcap_protocol::ble::parse_adv;
use espcap_protocol::config::DropCounters;
use espcap_protocol::pcap::{
    ble_pcap_payload, encode_frame, pcap_global_header, wifi_pcap_payload,
    DLT_BLUETOOTH_LE_LL_WITH_PHDR, DLT_IEEE802_11_RADIO, TYPE_BLE, TYPE_GLOBAL, TYPE_WIFI,
};
use espcap_protocol::types::{freq_mhz, hop_sequence, Chip, Mode, OutputFormat, Radio};
use espcap_protocol::wifi::{clamp_copy_len, parse_80211, Discovery};
use espcap_protocol::{
    ack, ble_adv, ble_disc, encode_event, error_event, parse_line, wifi_ap, wifi_frame, wifi_sta,
    Command, DeviceConfig,
};
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const SNAP_COPY: usize = 768;
const DEDUP_COOLDOWN_MS: u64 = 5000;

#[derive(Clone)]
struct WifiPkt {
    rssi: i8,
    channel: u8,
    freq_mhz: u16,
    is_5ghz: bool,
    rate: u8,
    ts_ms: u64,
    payload: Vec<u8>,
}

#[derive(Clone)]
struct BlePkt {
    addr: [u8; 6],
    addr_random: bool,
    rssi: i8,
    ts_ms: u64,
    name: Option<String>,
    company_id: Option<u16>,
    adv: Vec<u8>,
}

struct Dedup {
    first: u64,
    last: u64,
    hits: u32,
    rssi: i8,
    last_emit: u64,
}

static WIFI_TX: Mutex<Option<SyncSender<WifiPkt>>> = Mutex::new(None);
static BLE_TX: Mutex<Option<SyncSender<BlePkt>>> = Mutex::new(None);
static APP_CFG: Mutex<Option<Arc<Mutex<DeviceConfig>>>> = Mutex::new(None);
static DROPS: Mutex<DropCounters> = Mutex::new(DropCounters {
    ring_overflow: 0,
    cdc_backpressure: 0,
    truncated: 0,
});

fn chip() -> Chip {
    if cfg!(target_arch = "xtensa") {
        Chip::Esp32s3
    } else {
        Chip::Esp32c5
    }
}

fn now_ms() -> u64 {
    unsafe { esp_timer_get_time() as u64 / 1000 }
}

fn emit(line: &str) {
    let mut out = io::stdout();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

fn emit_bytes(bytes: &[u8]) {
    let mut out = io::stdout();
    if out.write_all(bytes).is_err() {
        if let Ok(mut d) = DROPS.lock() {
            d.cdc_backpressure = d.cdc_backpressure.saturating_add(1);
        }
    }
    let _ = out.flush();
}

fn reply_status(cfg: &DeviceConfig) {
    let drops = DROPS.lock().ok().map(|d| d.clone()).unwrap_or_default();
    if let Ok(s) = serde_json::to_string(&cfg.status_json(chip(), &drops)) {
        emit(&s);
    }
}

unsafe extern "C" fn wifi_rx_cb(buf: *mut core::ffi::c_void, _typ: u32) {
    if buf.is_null() {
        return;
    }
    let pkt = &*(buf as *const wifi_promiscuous_pkt_t);
    let ctrl = pkt.rx_ctrl;
    let sig_len = ctrl.sig_len() as usize;
    let payload = pkt.payload.as_slice(sig_len.max(1));
    let avail = payload.len().min(sig_len);
    let copy = clamp_copy_len(sig_len, avail, SNAP_COPY);
    if copy < sig_len {
        if let Ok(mut d) = DROPS.lock() {
            d.truncated = d.truncated.saturating_add(1);
        }
    }
    let channel = ctrl.channel() as u8;
    let is_5ghz = channel >= 32;
    let captured = WifiPkt {
        rssi: ctrl.rssi() as i8,
        channel,
        freq_mhz: freq_mhz(channel, is_5ghz),
        is_5ghz,
        rate: ctrl.rate() as u8,
        ts_ms: now_ms(),
        payload: payload[..copy].to_vec(),
    };
    if let Ok(guard) = WIFI_TX.lock() {
        if let Some(tx) = guard.as_ref() {
            if let Err(TrySendError::Full(_)) = tx.try_send(captured) {
                if let Ok(mut d) = DROPS.lock() {
                    d.ring_overflow = d.ring_overflow.saturating_add(1);
                }
            }
        }
    }
}

fn apply_command(cfg: &mut DeviceConfig, line: &str) {
    match parse_line(line) {
        Ok(Command::Get) | Ok(Command::Status) => reply_status(cfg),
        Ok(Command::Start) => {
            cfg.running = true;
            emit(&encode_event(&ack()).unwrap_or_else(|_| "{\"event\":\"ack\"}".into()));
            if cfg.format == OutputFormat::Pcap {
                if matches!(cfg.radio, Radio::Wifi | Radio::Both) {
                    emit_bytes(&encode_frame(
                        TYPE_GLOBAL,
                        &pcap_global_header(DLT_IEEE802_11_RADIO),
                    ));
                }
                if matches!(cfg.radio, Radio::Ble | Radio::Both) {
                    emit_bytes(&encode_frame(
                        TYPE_GLOBAL,
                        &pcap_global_header(DLT_BLUETOOTH_LE_LL_WITH_PHDR),
                    ));
                }
            }
        }
        Ok(Command::Stop) => {
            cfg.running = false;
            emit(&encode_event(&ack()).unwrap_or_else(|_| "{\"event\":\"ack\"}".into()));
        }
        Ok(Command::Set {
            radio,
            mode,
            format,
            wifi_band,
            dwell_ms,
            hopmask,
            channels_5ghz,
            ble_interval_ms,
            ble_window_ms,
            ble_active,
            wifi_types,
            filters,
        }) => {
            match cfg.apply_set(
                radio,
                mode,
                format,
                wifi_band,
                dwell_ms,
                hopmask,
                channels_5ghz,
                ble_interval_ms,
                ble_window_ms,
                ble_active,
                wifi_types,
                filters,
            ) {
                Ok(()) => match cfg.validate_for_chip(chip()) {
                    Ok(()) => {
                        emit(
                            &encode_event(&ack()).unwrap_or_else(|_| "{\"event\":\"ack\"}".into()),
                        );
                    }
                    Err(e) => emit(&encode_event(&error_event(e.to_string())).unwrap()),
                },
                Err(e) => emit(&encode_event(&error_event(e.to_string())).unwrap()),
            }
        }
        Err(e) => emit(&encode_event(&error_event(e.to_string())).unwrap()),
    }
}

fn maybe_emit_wifi(cfg: &DeviceConfig, dedup: &mut HashMap<[u8; 6], Dedup>, pkt: &WifiPkt) {
    let Ok(parsed) = parse_80211(&pkt.payload) else {
        return;
    };
    let mac = parsed.addr2;
    if !cfg.filters.matches_wifi(&mac, parsed.ssid.as_deref()) {
        return;
    }
    if cfg.mode == Mode::Capture {
        if cfg.format == OutputFormat::Json {
            if let Ok(s) = encode_event(&wifi_frame(
                &mac,
                pkt.rssi,
                pkt.channel,
                pkt.freq_mhz,
                pkt.ts_ms,
                &pkt.payload,
            )) {
                emit(&s);
            }
        } else {
            emit_bytes(&encode_frame(
                TYPE_WIFI,
                &wifi_pcap_payload(
                    pkt.ts_ms,
                    pkt.freq_mhz,
                    pkt.is_5ghz,
                    pkt.rssi,
                    pkt.rate,
                    &pkt.payload,
                ),
            ));
        }
        return;
    }
    let Some(disc) = &parsed.discovery else {
        return;
    };
    let key = match disc {
        Discovery::Ap { bssid } => *bssid,
        Discovery::Sta { mac } => *mac,
    };
    let e = dedup.entry(key).or_insert(Dedup {
        first: pkt.ts_ms,
        last: pkt.ts_ms,
        hits: 0,
        rssi: pkt.rssi,
        last_emit: 0,
    });
    e.hits = e.hits.saturating_add(1);
    e.last = pkt.ts_ms;
    let rssi_jump = (e.rssi as i16 - pkt.rssi as i16).unsigned_abs() >= 6;
    e.rssi = pkt.rssi;
    if e.last_emit != 0 && pkt.ts_ms.saturating_sub(e.last_emit) < DEDUP_COOLDOWN_MS && !rssi_jump {
        return;
    }
    e.last_emit = pkt.ts_ms;
    let ev = match disc {
        Discovery::Ap { bssid } => wifi_ap(
            bssid,
            pkt.rssi,
            pkt.channel,
            pkt.freq_mhz,
            pkt.ts_ms,
            parsed.ssid.as_deref().unwrap_or(""),
            e.hits,
            e.first,
            e.last,
        ),
        Discovery::Sta { mac } => wifi_sta(
            mac,
            pkt.rssi,
            pkt.channel,
            pkt.freq_mhz,
            pkt.ts_ms,
            parsed.ssid.as_deref(),
            e.hits,
            e.first,
            e.last,
        ),
    };
    if cfg.format == OutputFormat::Json {
        if let Ok(s) = encode_event(&ev) {
            emit(&s);
        }
    }
}

fn maybe_emit_ble(cfg: &DeviceConfig, dedup: &mut HashMap<[u8; 6], Dedup>, pkt: &BlePkt) {
    if !cfg
        .filters
        .matches_ble(&pkt.addr, pkt.name.as_deref(), pkt.company_id)
    {
        return;
    }
    if cfg.mode == Mode::Capture {
        if cfg.format == OutputFormat::Json {
            if let Ok(s) = encode_event(&ble_adv(
                &pkt.addr,
                pkt.rssi,
                pkt.ts_ms,
                &pkt.adv,
                pkt.name.as_deref(),
            )) {
                emit(&s);
            }
        } else {
            emit_bytes(&encode_frame(
                TYPE_BLE,
                &ble_pcap_payload(
                    pkt.ts_ms,
                    pkt.rssi,
                    39,
                    &pkt.addr,
                    pkt.addr_random,
                    &pkt.adv,
                ),
            ));
        }
        return;
    }
    let e = dedup.entry(pkt.addr).or_insert(Dedup {
        first: pkt.ts_ms,
        last: pkt.ts_ms,
        hits: 0,
        rssi: pkt.rssi,
        last_emit: 0,
    });
    e.hits = e.hits.saturating_add(1);
    e.last = pkt.ts_ms;
    if e.last_emit != 0 && pkt.ts_ms.saturating_sub(e.last_emit) < DEDUP_COOLDOWN_MS {
        return;
    }
    e.last_emit = pkt.ts_ms;
    if let Ok(s) = encode_event(&ble_disc(
        &pkt.addr,
        pkt.rssi,
        pkt.ts_ms,
        pkt.name.as_deref(),
        pkt.company_id,
        e.hits,
        e.first,
        e.last,
        pkt.addr_random,
    )) {
        emit(&s);
    }
}

fn set_promisc_filter(mask: u32) {
    let filter = wifi_promiscuous_filter_t { filter_mask: mask };
    unsafe {
        let _ = esp_wifi_set_promiscuous_filter(&filter);
        let _ = esp_wifi_set_promiscuous(true);
    }
}

fn hop_loop(running: Arc<AtomicBool>, cfg: Arc<Mutex<DeviceConfig>>) {
    let mut idx = 0usize;
    loop {
        let (seq, dwell, radio_on) = {
            let g = cfg.lock().unwrap();
            let seq = hop_sequence(g.wifi_band, g.hopmask, &g.channels_5ghz).unwrap_or_default();
            (
                seq,
                g.dwell_ms,
                g.running && matches!(g.radio, Radio::Wifi | Radio::Both),
            )
        };
        if radio_on && !seq.is_empty() {
            idx %= seq.len();
            let ch = seq[idx];
            unsafe {
                let _ = esp_wifi_set_channel(ch.channel, wifi_second_chan_t_WIFI_SECOND_CHAN_NONE);
            }
            idx = idx.wrapping_add(1);
        }
        thread::sleep(Duration::from_millis(u64::from(dwell.max(200))));
        let _ = running.load(Ordering::Relaxed);
    }
}

fn main() -> Result<(), EspError> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    log::set_max_level(log::LevelFilter::Off);

    let peripherals = Peripherals::take()?;
    let sysloop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;
    let mut wifi = EspWifi::new(peripherals.modem, sysloop, Some(nvs))?;
    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ..Default::default()
    }))?;
    wifi.start()?;

    let (wifi_tx, wifi_rx) = mpsc::sync_channel::<WifiPkt>(32);
    *WIFI_TX.lock().unwrap() = Some(wifi_tx);
    let (ble_tx, ble_rx) = mpsc::sync_channel::<BlePkt>(32);

    unsafe {
        let _ = esp_wifi_set_promiscuous_rx_cb(Some(wifi_rx_cb));
        let _ = esp_wifi_set_promiscuous(true);
    }

    let cfg = Arc::new(Mutex::new(DeviceConfig::default()));
    *APP_CFG.lock().unwrap() = Some(Arc::clone(&cfg));
    let running = Arc::new(AtomicBool::new(true));
    {
        #[cfg(target_arch = "xtensa")]
        {
            use esp_idf_svc::hal::task::thread::ThreadSpawnConfiguration;
            ThreadSpawnConfiguration {
                name: Some(c"hop"),
                stack_size: 4096,
                priority: 4,
                inherit: false,
                pin_to_core: None,
                ..Default::default()
            }
            .set()
            .ok();
        }
        let cfg_h = Arc::clone(&cfg);
        let run_h = Arc::clone(&running);
        thread::spawn(move || hop_loop(run_h, cfg_h));
    }

    spawn_ble(ble_tx);

    let mut wifi_dedup: HashMap<[u8; 6], Dedup> = HashMap::new();
    let mut ble_dedup: HashMap<[u8; 6], Dedup> = HashMap::new();
    let (cmd_tx, cmd_rx) = mpsc::channel::<String>();
    #[cfg(target_arch = "xtensa")]
    {
        use esp_idf_svc::hal::cpu::Core;
        use esp_idf_svc::hal::task::thread::ThreadSpawnConfiguration;
        ThreadSpawnConfiguration {
            name: Some(c"cmd"),
            stack_size: 8192,
            priority: 5,
            inherit: false,
            pin_to_core: Some(Core::Core1),
            ..Default::default()
        }
        .set()
        .ok();
    }
    thread::Builder::new()
        .name("cmd".into())
        .stack_size(8192)
        .spawn(move || {
            let stdin = io::stdin();
            for line in stdin.lock().lines() {
                if let Ok(line) = line {
                    if line.trim().is_empty() {
                        continue;
                    }
                    if cmd_tx.send(line).is_err() {
                        break;
                    }
                }
            }
        })
        .ok();
    #[cfg(target_arch = "xtensa")]
    {
        use esp_idf_svc::hal::task::thread::ThreadSpawnConfiguration;
        ThreadSpawnConfiguration::default().set().ok();
    }

    let mut last_mask = 0u32;
    loop {
        if let Ok(pkt) = wifi_rx.try_recv() {
            let g = cfg.lock().unwrap();
            if g.running && matches!(g.radio, Radio::Wifi | Radio::Both) {
                maybe_emit_wifi(&g, &mut wifi_dedup, &pkt);
            }
        }
        if let Ok(pkt) = ble_rx.try_recv() {
            let g = cfg.lock().unwrap();
            if g.running && matches!(g.radio, Radio::Ble | Radio::Both) {
                maybe_emit_ble(&g, &mut ble_dedup, &pkt);
            }
        }
        if let Ok(line) = cmd_rx.try_recv() {
            let mut g = cfg.lock().unwrap();
            apply_command(&mut g, &line);
            let mask = g.promiscuous_filter_mask();
            if mask != last_mask {
                last_mask = mask;
                set_promisc_filter(mask);
            }
        }
        thread::sleep(Duration::from_millis(5));
    }
}

fn ms_to_625us(ms: u16) -> u16 {
    ((u32::from(ms) * 8) / 5).clamp(16, 16_384) as u16
}

fn start_ble_disc(cfg: &DeviceConfig) {
    let mut params = ble_gap_disc_params::default();
    params.itvl = ms_to_625us(cfg.ble_interval_ms.max(10));
    params.window = ms_to_625us(cfg.ble_window_ms.min(cfg.ble_interval_ms).max(10));
    params.set_passive(u8::from(!cfg.ble_active));
    params.set_filter_duplicates(0);
    unsafe {
        let _ = ble_gap_disc(
            BLE_ADDR_PUBLIC as u8,
            i32::MAX,
            &params,
            Some(ble_gap_cb),
            core::ptr::null_mut(),
        );
    }
}

fn push_ble_report(addr: [u8; 6], addr_random: bool, rssi: i8, data: *const u8, length: u8) {
    let adv = if data.is_null() || length == 0 {
        Vec::new()
    } else {
        unsafe { core::slice::from_raw_parts(data, length as usize) }.to_vec()
    };
    let parsed = parse_adv(&adv);
    let pkt = BlePkt {
        addr,
        addr_random,
        rssi,
        ts_ms: now_ms(),
        name: parsed.name,
        company_id: parsed.company_id,
        adv,
    };
    if let Ok(guard) = BLE_TX.lock() {
        if let Some(tx) = guard.as_ref() {
            if let Err(TrySendError::Full(_)) = tx.try_send(pkt) {
                if let Ok(mut d) = DROPS.lock() {
                    d.ring_overflow = d.ring_overflow.saturating_add(1);
                }
            }
        }
    }
}

unsafe extern "C" fn ble_gap_cb(event: *mut ble_gap_event, _arg: *mut core::ffi::c_void) -> i32 {
    if event.is_null() {
        return 0;
    }
    let ev = &*event;
    match u32::from(ev.type_) {
        BLE_GAP_EVENT_DISC => {
            let d = ev.__bindgen_anon_1.disc;
            push_ble_report(
                d.addr.val,
                d.addr.type_ != BLE_ADDR_PUBLIC as u8,
                d.rssi,
                d.data,
                d.length_data,
            );
        }
        BLE_GAP_EVENT_EXT_DISC => {
            let d = ev.__bindgen_anon_1.ext_disc;
            push_ble_report(
                d.addr.val,
                d.addr.type_ != BLE_ADDR_PUBLIC as u8,
                d.rssi,
                d.data,
                d.length_data,
            );
        }
        BLE_GAP_EVENT_DISC_COMPLETE => {
            if let Ok(guard) = APP_CFG.lock() {
                if let Some(cfg) = guard.as_ref() {
                    if let Ok(g) = cfg.lock() {
                        start_ble_disc(&g);
                    }
                }
            }
        }
        _ => {}
    }
    0
}

unsafe extern "C" fn ble_on_sync() {
    if let Ok(guard) = APP_CFG.lock() {
        if let Some(cfg) = guard.as_ref() {
            if let Ok(g) = cfg.lock() {
                start_ble_disc(&g);
            }
        }
    }
}

unsafe extern "C" fn ble_on_reset(_reason: i32) {}

unsafe extern "C" fn ble_host_task(_arg: *mut core::ffi::c_void) {
    nimble_port_run();
    nimble_port_freertos_deinit();
}

fn spawn_ble(tx: SyncSender<BlePkt>) {
    *BLE_TX.lock().unwrap() = Some(tx);
    unsafe {
        if nimble_port_init() != 0 {
            return;
        }
        ble_hs_cfg.sync_cb = Some(ble_on_sync);
        ble_hs_cfg.reset_cb = Some(ble_on_reset);
        nimble_port_freertos_init(Some(ble_host_task));
    }
}
