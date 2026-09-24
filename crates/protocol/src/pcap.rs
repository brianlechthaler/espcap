use crate::types::SNAPLEN;
use crate::Error;

pub const FRAME_MAGIC: [u8; 2] = [0xa5, 0x5a];
pub const TYPE_GLOBAL: u8 = 0;
pub const TYPE_WIFI: u8 = 1;
pub const TYPE_BLE: u8 = 2;
pub const DLT_IEEE802_11_RADIO: u32 = 127;
pub const DLT_BLUETOOTH_LE_LL_WITH_PHDR: u32 = 256;
pub const RADIOTAP_LEN: usize = 23;
pub const BTLE_RF_LEN: usize = 10;
pub const PCAP_RECORD_LEN: usize = 16;
pub const ADV_AA: u32 = 0x8e89_bed6;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerialFrame {
    pub typ: u8,
    pub payload: Vec<u8>,
}

pub fn encode_frame(typ: u8, payload: &[u8]) -> Vec<u8> {
    let len = payload.len() as u16;
    let mut out = Vec::with_capacity(5 + payload.len());
    out.extend_from_slice(&FRAME_MAGIC);
    out.push(typ);
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(payload);
    out
}

pub fn decode_frames(buf: &[u8]) -> (Vec<SerialFrame>, usize) {
    let mut frames = Vec::new();
    let mut i = 0;
    while i + 5 <= buf.len() {
        if buf[i] != FRAME_MAGIC[0] {
            i += 1;
            continue;
        }
        if buf[i + 1] != FRAME_MAGIC[1] {
            i += 1;
            continue;
        }
        let typ = buf[i + 2];
        let len = u16::from_be_bytes([buf[i + 3], buf[i + 4]]) as usize;
        if i + 5 + len > buf.len() {
            break;
        }
        frames.push(SerialFrame {
            typ,
            payload: buf[i + 5..i + 5 + len].to_vec(),
        });
        i += 5 + len;
    }
    (frames, i)
}

pub fn pcap_global_header(dlt: u32) -> [u8; 24] {
    let mut h = [0u8; 24];
    h[0..4].copy_from_slice(&0xa1b2c3d4u32.to_le_bytes());
    h[4..6].copy_from_slice(&2u16.to_le_bytes());
    h[6..8].copy_from_slice(&4u16.to_le_bytes());
    h[16..20].copy_from_slice(&SNAPLEN.to_le_bytes());
    h[20..24].copy_from_slice(&dlt.to_le_bytes());
    h
}

pub fn pcap_record(ts_ms: u64, packet: &[u8]) -> Vec<u8> {
    let mut rec = Vec::with_capacity(PCAP_RECORD_LEN + packet.len());
    let sec = (ts_ms / 1000) as u32;
    let usec = ((ts_ms % 1000) * 1000) as u32;
    rec.extend_from_slice(&sec.to_le_bytes());
    rec.extend_from_slice(&usec.to_le_bytes());
    rec.extend_from_slice(&(packet.len() as u32).to_le_bytes());
    rec.extend_from_slice(&(packet.len() as u32).to_le_bytes());
    rec.extend_from_slice(packet);
    rec
}

pub fn radiotap(
    ts_ms: u64,
    freq_mhz: u16,
    is_5ghz: bool,
    rssi: i8,
    rate: u8,
) -> [u8; RADIOTAP_LEN] {
    let mut r = [0u8; RADIOTAP_LEN];
    r[2] = RADIOTAP_LEN as u8;
    // present: TSFT | Flags | Rate | Channel | dBm Ant Signal
    r[4..8].copy_from_slice(&0x002fu32.to_le_bytes());
    r[8..16].copy_from_slice(&(ts_ms * 1000).to_le_bytes());
    r[16] = 0x10; // FCS present
    r[17] = rate;
    r[18..20].copy_from_slice(&freq_mhz.to_le_bytes());
    let chan_flags: u16 = if is_5ghz { 0x0140 } else { 0x00c0 };
    r[20..22].copy_from_slice(&chan_flags.to_le_bytes());
    r[22] = rssi as u8;
    r
}

pub fn wifi_pcap_payload(
    ts_ms: u64,
    freq_mhz: u16,
    is_5ghz: bool,
    rssi: i8,
    rate: u8,
    mpdu: &[u8],
) -> Vec<u8> {
    let rt = radiotap(ts_ms, freq_mhz, is_5ghz, rssi, rate);
    let mut pkt = Vec::with_capacity(RADIOTAP_LEN + mpdu.len());
    pkt.extend_from_slice(&rt);
    pkt.extend_from_slice(mpdu);
    pcap_record(ts_ms, &pkt)
}

pub fn btle_rf_phdr(rssi: i8, channel: u8) -> [u8; BTLE_RF_LEN] {
    let mut p = [0u8; BTLE_RF_LEN];
    p[0] = channel;
    p[1] = rssi as u8;
    p[2] = 0;
    p[3] = 0;
    p[4..8].copy_from_slice(&ADV_AA.to_le_bytes());
    // flags: signal power valid | channel valid | access address valid
    p[8..10].copy_from_slice(&0x0003u16.to_le_bytes());
    p
}

pub fn reconstruct_adv_pdu(addr: &[u8; 6], addr_random: bool, adv_data: &[u8]) -> Vec<u8> {
    let mut pdu = Vec::with_capacity(8 + adv_data.len());
    let mut header = 0u16; // ADV_IND
    if addr_random {
        header |= 1 << 6; // TxAdd
    }
    let len = 6 + adv_data.len();
    header |= ((len as u16) & 0xff) << 8;
    pdu.extend_from_slice(&header.to_le_bytes());
    pdu.extend_from_slice(addr);
    pdu.extend_from_slice(adv_data);
    pdu
}

pub fn ble_pcap_payload(
    ts_ms: u64,
    rssi: i8,
    rf_channel: u8,
    addr: &[u8; 6],
    addr_random: bool,
    adv_data: &[u8],
) -> Vec<u8> {
    let phdr = btle_rf_phdr(rssi, rf_channel);
    let pdu = reconstruct_adv_pdu(addr, addr_random, adv_data);
    let mut pkt = Vec::with_capacity(BTLE_RF_LEN + pdu.len());
    pkt.extend_from_slice(&phdr);
    pkt.extend_from_slice(&pdu);
    pcap_record(ts_ms, &pkt)
}

pub fn parse_global_header(bytes: &[u8]) -> Result<u32, Error> {
    if bytes.len() != 24 {
        return Err(Error::TruncatedFrame);
    }
    let magic = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
    if magic != 0xa1b2c3d4 {
        return Err(Error::BadMagic);
    }
    Ok(u32::from_le_bytes(bytes[20..24].try_into().unwrap()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::freq_mhz;
    use crate::wifi::beacon_fixture;

    #[test]
    fn frame_round_trip_and_resync() {
        let f = encode_frame(TYPE_WIFI, &[1, 2, 3]);
        assert_eq!(&f[..2], &FRAME_MAGIC);
        let (frames, n) = decode_frames(&f);
        assert_eq!(n, f.len());
        assert_eq!(frames[0].typ, TYPE_WIFI);
        assert_eq!(frames[0].payload, vec![1, 2, 3]);
        let mut noisy = vec![0x00, 0xa5, 0x00];
        noisy.extend_from_slice(&f);
        noisy.extend_from_slice(&[0xa5, 0x5a, TYPE_BLE, 0x00, 0x10]); // incomplete
        let (frames, consumed) = decode_frames(&noisy);
        assert_eq!(frames.len(), 1);
        assert_eq!(consumed, 3 + f.len());
        let empty = decode_frames(&[0xa5]);
        assert!(empty.0.is_empty());
    }

    #[test]
    fn radiotap_2g_5g_and_headers() {
        let rt = radiotap(1500, freq_mhz(6, false), false, -40, 2);
        assert_eq!(rt.len(), 23);
        assert_eq!(u16::from_le_bytes([rt[18], rt[19]]), 2437);
        assert_eq!(u16::from_le_bytes([rt[20], rt[21]]), 0x00c0);
        let rt5 = radiotap(0, freq_mhz(36, true), true, -70, 12);
        assert_eq!(u16::from_le_bytes([rt5[18], rt5[19]]), 5180);
        assert_eq!(u16::from_le_bytes([rt5[20], rt5[21]]), 0x0140);
        let gh = pcap_global_header(DLT_IEEE802_11_RADIO);
        assert_eq!(parse_global_header(&gh).unwrap(), 127);
        let ble_h = pcap_global_header(DLT_BLUETOOTH_LE_LL_WITH_PHDR);
        assert_eq!(parse_global_header(&ble_h).unwrap(), 256);
        assert!(parse_global_header(&[0; 10]).is_err());
        let mut bad = gh;
        bad[0] = 0;
        assert!(parse_global_header(&bad).is_err());
        let mpdu = beacon_fixture([1; 6], "n");
        let payload = wifi_pcap_payload(2500, 2412, false, -30, 2, &mpdu);
        assert_eq!(payload.len(), 16 + 23 + mpdu.len());
        assert_eq!(u32::from_le_bytes(payload[0..4].try_into().unwrap()), 2);
        assert_eq!(
            u32::from_le_bytes(payload[4..8].try_into().unwrap()),
            500_000
        );
    }

    #[test]
    fn ble_reconstruct_dlt256() {
        let addr = [0xf4, 0x4e, 0xfc, 1, 2, 3];
        let payload = ble_pcap_payload(0, -60, 39, &addr, true, &[0x08, 0x09, b'A']);
        assert!(payload.len() > 16 + 10);
        let pdu = reconstruct_adv_pdu(&addr, false, &[]);
        assert_eq!(pdu[0] & 0x40, 0);
        let pdu_r = reconstruct_adv_pdu(&addr, true, &[1]);
        assert_ne!(pdu_r[0] & 0x40, 0);
        let phdr = btle_rf_phdr(-50, 39);
        assert_eq!(phdr[0], 39);
        assert_eq!(u32::from_le_bytes(phdr[4..8].try_into().unwrap()), ADV_AA);
    }
}
