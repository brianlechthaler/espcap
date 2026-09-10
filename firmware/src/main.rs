use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault};
use esp_idf_svc::sys::{
    esp_task_wdt_reset, esp_timer_get_time, esp_wifi_set_channel, esp_wifi_set_promiscuous,
    esp_wifi_set_promiscuous_filter, esp_wifi_set_promiscuous_rx_cb, wifi_promiscuous_filter_t,
    wifi_promiscuous_pkt_t, wifi_promiscuous_pkt_type_t_WIFI_PKT_MISC,
    wifi_second_chan_t_WIFI_SECOND_CHAN_NONE, EspError,
};
use esp_idf_svc::wifi::{ClientConfiguration, Configuration, EspWifi};
use espcap_protocol::config::DropCounters;
use espcap_protocol::pcap::{
    ble_pcap_payload, encode_frame, pcap_global_header, wifi_pcap_payload,
    DLT_BLUETOOTH_LE_LL_WITH_PHDR, DLT_IEEE802_11_RADIO, TYPE_BLE, TYPE_GLOBAL, TYPE_WIFI,
};
use espcap_protocol::types::{freq_mhz, hop_sequence, Chip, Mode, OutputFormat, Radio};
use espcap_protocol::wifi::{parse_80211, Discovery};
use espcap_protocol::{
    ack, ble_adv, ble_disc, encode_event, error_event, parse_line, wifi_ap, wifi_frame, wifi_sta,
    Command, DeviceConfig, PushOutcome, WifiHdr, WifiRing, WifiSlot, WIFI_RING_SLOTS,
    WIFI_SNAP_LEN,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;

const DEDUP_COOLDOWN_MS: u64 = 5000;

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

static WIFI_RING: WifiRing = WifiRing::new();
static WIFI_ON: AtomicBool = AtomicBool::new(false);
static DROP_RING: AtomicU32 = AtomicU32::new(0);
static DROP_CDC: AtomicU32 = AtomicU32::new(0);
static DROP_TRUNC: AtomicU32 = AtomicU32::new(0);

#[repr(C)]
struct UsjCfg {
    tx_buffer_size: u32,
    rx_buffer_size: u32,
}

unsafe extern "C" {
    fn usb_serial_jtag_driver_install(cfg: *mut UsjCfg) -> i32;
    fn usb_serial_jtag_is_driver_installed() -> bool;
    fn usb_serial_jtag_read_bytes(buf: *mut u8, length: u32, ticks_to_wait: u32) -> i32;
    fn usb_serial_jtag_write_bytes(src: *const u8, size: usize, ticks_to_wait: u32) -> i32;
    fn usb_serial_jtag_wait_tx_done(ticks_to_wait: u32) -> i32;
    fn usb_serial_jtag_vfs_use_driver();
}

fn usj_install() {
    if unsafe { usb_serial_jtag_is_driver_installed() } {
        unsafe { usb_serial_jtag_vfs_use_driver() };
        return;
    }
    let mut cfg = UsjCfg {
        tx_buffer_size: 4096,
        rx_buffer_size: 1024,
    };
    unsafe {
        let _ = usb_serial_jtag_driver_install(&mut cfg);
        usb_serial_jtag_vfs_use_driver();
    }
}

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
    let mut buf = Vec::with_capacity(line.len() + 1);
    buf.extend_from_slice(line.as_bytes());
    buf.push(b'\n');
    emit_bytes(&buf);
}

fn emit_bytes(bytes: &[u8]) {
    let mut off = 0usize;
    for _ in 0..200 {
        let n =
            unsafe { usb_serial_jtag_write_bytes(bytes[off..].as_ptr(), bytes.len() - off, 20) };
        if n > 0 {
            off += n as usize;
            if off >= bytes.len() {
                unsafe {
                    let _ = usb_serial_jtag_wait_tx_done(40);
                }
                return;
            }
        } else {
            unsafe {
                let _ = usb_serial_jtag_wait_tx_done(20);
            }
            FreeRtos::delay_ms(2);
        }
    }
    if off > 0 && bytes.get(off.saturating_sub(1)) != Some(&b'\n') {
        let nl = [b'\n'];
        unsafe {
            let _ = usb_serial_jtag_write_bytes(nl.as_ptr(), 1, 20);
            let _ = usb_serial_jtag_wait_tx_done(20);
        }
    }
    DROP_CDC.fetch_add(1, Ordering::Relaxed);
}

fn load_cfg(nvs: &Option<EspNvs<NvsDefault>>) -> DeviceConfig {
    let Some(nvs) = nvs else {
        return DeviceConfig::default();
    };
    let Ok(Some(len)) = nvs.blob_len("cfg") else {
        return DeviceConfig::default();
    };
    let mut buf = vec![0u8; len.min(4096)];
    match nvs.get_blob("cfg", &mut buf) {
        Ok(Some(bytes)) => DeviceConfig::restore(bytes).unwrap_or_default(),
        _ => DeviceConfig::default(),
    }
}

fn save_cfg(nvs: &Option<EspNvs<NvsDefault>>, cfg: &DeviceConfig) {
    let Some(nvs) = nvs else {
        return;
    };
    if let Ok(blob) = cfg.persist() {
        let _ = nvs.set_blob("cfg", &blob);
    }
}

fn reply_status(cfg: &DeviceConfig) {
    let drops = DropCounters {
        ring_overflow: DROP_RING.load(Ordering::Relaxed),
        cdc_backpressure: DROP_CDC.load(Ordering::Relaxed),
        truncated: DROP_TRUNC.load(Ordering::Relaxed),
    };
    if let Ok(s) = serde_json::to_string(&cfg.status_json(chip(), &drops)) {
        emit(&s);
    }
}

unsafe extern "C" fn wifi_rx_cb(buf: *mut core::ffi::c_void, typ: u32) {
    if !WIFI_ON.load(Ordering::Relaxed) || buf.is_null() {
        return;
    }
    if typ == wifi_promiscuous_pkt_type_t_WIFI_PKT_MISC {
        DROP_TRUNC.fetch_add(1, Ordering::Relaxed);
        return;
    }
    let pkt = buf as *const wifi_promiscuous_pkt_t;
    let ctrl = (*pkt).rx_ctrl;
    let sig_len = ctrl.sig_len() as usize;
    let n = sig_len.min(WIFI_SNAP_LEN);
    let src = if n == 0 {
        &[]
    } else {
        core::slice::from_raw_parts(core::ptr::addr_of!((*pkt).payload) as *const u8, n)
    };
    let channel = ctrl.channel() as u8;
    let is_5ghz = channel >= 32;
    match WIFI_RING.try_push(
        WifiHdr {
            rssi: ctrl.rssi() as i8,
            channel,
            freq_mhz: freq_mhz(channel, is_5ghz),
            is_5ghz,
            rate: ctrl.rate() as u8,
            ts_ms: now_ms(),
        },
        src,
        sig_len,
    ) {
        PushOutcome::Full => {
            DROP_RING.fetch_add(1, Ordering::Relaxed);
        }
        PushOutcome::Stored { truncated } => {
            if truncated {
                DROP_TRUNC.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

fn apply_command(cfg: &mut DeviceConfig, line: &str) -> bool {
    match parse_line(line) {
        Ok(Command::Get) | Ok(Command::Status) => {
            reply_status(cfg);
            false
        }
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
            true
        }
        Ok(Command::Stop) => {
            cfg.running = false;
            emit(&encode_event(&ack()).unwrap_or_else(|_| "{\"event\":\"ack\"}".into()));
            true
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
                        true
                    }
                    Err(e) => {
                        emit(
                            &encode_event(&error_event(e.to_string())).unwrap_or_else(|_| {
                                "{\"event\":\"error\",\"msg\":\"encode\"}".into()
                            }),
                        );
                        false
                    }
                },
                Err(e) => {
                    emit(
                        &encode_event(&error_event(e.to_string()))
                            .unwrap_or_else(|_| "{\"event\":\"error\",\"msg\":\"encode\"}".into()),
                    );
                    false
                }
            }
        }
        Err(espcap_protocol::Error::UnknownCommand) => false,
        Err(e) => {
            emit(
                &encode_event(&error_event(e.to_string()))
                    .unwrap_or_else(|_| "{\"event\":\"error\",\"msg\":\"encode\"}".into()),
            );
            false
        }
    }
}

fn maybe_emit_wifi(cfg: &DeviceConfig, dedup: &mut HashMap<[u8; 6], Dedup>, pkt: &WifiSlot) {
    let Ok(parsed) = parse_80211(pkt.payload()) else {
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
                pkt.hdr.rssi,
                pkt.hdr.channel,
                pkt.hdr.freq_mhz,
                pkt.hdr.ts_ms,
                pkt.payload(),
            )) {
                emit(&s);
            }
        } else {
            emit_bytes(&encode_frame(
                TYPE_WIFI,
                &wifi_pcap_payload(
                    pkt.hdr.ts_ms,
                    pkt.hdr.freq_mhz,
                    pkt.hdr.is_5ghz,
                    pkt.hdr.rssi,
                    pkt.hdr.rate,
                    pkt.payload(),
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
        first: pkt.hdr.ts_ms,
        last: pkt.hdr.ts_ms,
        hits: 0,
        rssi: pkt.hdr.rssi,
        last_emit: 0,
    });
    e.hits = e.hits.saturating_add(1);
    e.last = pkt.hdr.ts_ms;
    let rssi_jump = (e.rssi as i16 - pkt.hdr.rssi as i16).unsigned_abs() >= 6;
    e.rssi = pkt.hdr.rssi;
    if e.last_emit != 0
        && pkt.hdr.ts_ms.saturating_sub(e.last_emit) < DEDUP_COOLDOWN_MS
        && !rssi_jump
    {
        return;
    }
    e.last_emit = pkt.hdr.ts_ms;
    let ev = match disc {
        Discovery::Ap { bssid } => wifi_ap(
            bssid,
            pkt.hdr.rssi,
            pkt.hdr.channel,
            pkt.hdr.freq_mhz,
            pkt.hdr.ts_ms,
            parsed.ssid.as_deref().unwrap_or(""),
            e.hits,
            e.first,
            e.last,
        ),
        Discovery::Sta { mac } => wifi_sta(
            mac,
            pkt.hdr.rssi,
            pkt.hdr.channel,
            pkt.hdr.freq_mhz,
            pkt.hdr.ts_ms,
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
    }
}

fn apply_radios(cfg: &DeviceConfig) {
    let wifi = cfg.running && matches!(cfg.radio, Radio::Wifi | Radio::Both);
    let ble = cfg.running && matches!(cfg.radio, Radio::Ble | Radio::Both);
    WIFI_ON.store(wifi, Ordering::Relaxed);
    unsafe {
        let _ = esp_wifi_set_promiscuous(wifi);
    }
    if ble {
        log::warn!("BLE requested; NimBLE host FFI is unavailable in this build");
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
        FreeRtos::delay_ms(u32::from(dwell.max(200)));
        let _ = running.load(Ordering::Relaxed);
    }
}

fn main() -> Result<(), EspError> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    log::set_max_level(log::LevelFilter::Warn);
    usj_install();

    let peripherals = Peripherals::take()?;
    let sysloop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;
    let store = EspNvs::new(nvs.clone(), "espcap", true).ok();
    let mut wifi = EspWifi::new(peripherals.modem, sysloop, Some(nvs))?;
    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ..Default::default()
    }))?;
    wifi.start()?;

    let (ble_tx, ble_rx) = mpsc::sync_channel::<BlePkt>(32);

    unsafe {
        let _ = esp_wifi_set_promiscuous_rx_cb(Some(wifi_rx_cb));
    }

    let cfg = Arc::new(Mutex::new(load_cfg(&store)));
    let running = Arc::new(AtomicBool::new(true));
    {
        #[cfg(target_arch = "xtensa")]
        {
            use esp_idf_svc::hal::task::thread::ThreadSpawnConfiguration;
            ThreadSpawnConfiguration {
                name: Some(c"hop"),
                stack_size: 8192,
                priority: 4,
                inherit: false,
                pin_to_core: Some(esp_idf_svc::hal::cpu::Core::Core1),
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
    let mut acc = Vec::new();
    let mut tmp = [0u8; 256];
    let mut last_mask;
    {
        let g = cfg.lock().unwrap();
        last_mask = g.promiscuous_filter_mask();
        set_promisc_filter(last_mask);
        apply_radios(&g);
        reply_status(&g);
    }
    loop {
        let n = unsafe { usb_serial_jtag_read_bytes(tmp.as_mut_ptr(), tmp.len() as u32, 0) };
        if n > 0 {
            acc.extend_from_slice(&tmp[..n as usize]);
            if acc.len() > 4096 {
                acc.clear();
            }
            while let Some(pos) = acc.iter().position(|&b| b == b'\n' || b == b'\r') {
                let line = acc.drain(..=pos).collect::<Vec<_>>();
                let line = String::from_utf8_lossy(&line);
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let mut local = cfg.lock().unwrap().clone();
                let persist = apply_command(&mut local, line);
                {
                    let mut g = cfg.lock().unwrap();
                    *g = local;
                    if persist {
                        save_cfg(&store, &g);
                    }
                    let mask = g.promiscuous_filter_mask();
                    if mask != last_mask {
                        last_mask = mask;
                        set_promisc_filter(mask);
                    }
                    apply_radios(&g);
                }
            }
        }
        let snap = cfg.lock().unwrap().clone();
        if snap.running && matches!(snap.radio, Radio::Wifi | Radio::Both) {
            for _ in 0..WIFI_RING_SLOTS {
                let Some(pkt) = WIFI_RING.try_pop() else {
                    break;
                };
                maybe_emit_wifi(&snap, &mut wifi_dedup, &pkt);
            }
        } else {
            while WIFI_RING.try_pop().is_some() {}
        }
        for _ in 0..8 {
            let Ok(pkt) = ble_rx.try_recv() else {
                break;
            };
            if snap.running && matches!(snap.radio, Radio::Ble | Radio::Both) {
                maybe_emit_ble(&snap, &mut ble_dedup, &pkt);
            }
        }
        unsafe {
            let _ = esp_task_wdt_reset();
        }
        FreeRtos::delay_ms(10);
    }
}

fn spawn_ble(tx: SyncSender<BlePkt>) {
    std::mem::forget(tx);
}
