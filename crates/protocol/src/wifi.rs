use crate::mac::format_mac;
use crate::Error;

pub const FC_TYPE_MGMT: u8 = 0;
pub const FC_TYPE_CTRL: u8 = 1;
pub const FC_TYPE_DATA: u8 = 2;
pub const SUBTYPE_PROBE_REQ: u8 = 4;
pub const SUBTYPE_PROBE_RESP: u8 = 5;
pub const SUBTYPE_BEACON: u8 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameKind {
    Mgmt,
    Ctrl,
    Data,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedWifi {
    pub kind: FrameKind,
    pub subtype: u8,
    pub addr1: [u8; 6],
    pub addr2: [u8; 6],
    pub addr3: Option<[u8; 6]>,
    pub ssid: Option<String>,
    pub discovery: Option<Discovery>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Discovery {
    Ap { bssid: [u8; 6] },
    Sta { mac: [u8; 6] },
}

pub fn frame_type(fc0: u8) -> u8 {
    (fc0 >> 2) & 0x3
}

pub fn frame_subtype(fc0: u8) -> u8 {
    (fc0 >> 4) & 0xf
}

pub fn clamp_copy_len(sig_len: usize, avail: usize, cap: usize) -> usize {
    sig_len.min(avail).min(cap)
}

fn read_addr(buf: &[u8], off: usize) -> Result<[u8; 6], Error> {
    buf.get(off..off + 6)
        .and_then(|s| s.try_into().ok())
        .ok_or(Error::ShortFrame)
}

pub fn parse_80211(mpdu: &[u8]) -> Result<ParsedWifi, Error> {
    if mpdu.len() < 16 {
        return Err(Error::ShortFrame);
    }
    let fc0 = mpdu[0];
    let kind = match frame_type(fc0) {
        FC_TYPE_MGMT => FrameKind::Mgmt,
        FC_TYPE_CTRL => FrameKind::Ctrl,
        FC_TYPE_DATA => FrameKind::Data,
        _ => FrameKind::Other,
    };
    let subtype = frame_subtype(fc0);
    let addr1 = read_addr(mpdu, 4)?;
    let addr2 = read_addr(mpdu, 10)?;
    let addr3 = if kind == FrameKind::Ctrl {
        None
    } else if mpdu.len() >= 24 {
        Some(read_addr(mpdu, 16)?)
    } else {
        return Err(Error::ShortFrame);
    };
    let ssid = if kind == FrameKind::Mgmt {
        match subtype {
            SUBTYPE_BEACON | SUBTYPE_PROBE_RESP => extract_ssid(mpdu, 36),
            SUBTYPE_PROBE_REQ => extract_ssid(mpdu, 24),
            _ => None,
        }
    } else {
        None
    };
    let discovery = match (kind, subtype) {
        (FrameKind::Mgmt, SUBTYPE_BEACON | SUBTYPE_PROBE_RESP) => Some(Discovery::Ap {
            bssid: addr3.unwrap_or(addr2),
        }),
        (FrameKind::Mgmt, SUBTYPE_PROBE_REQ) => Some(Discovery::Sta { mac: addr2 }),
        _ => None,
    };
    Ok(ParsedWifi {
        kind,
        subtype,
        addr1,
        addr2,
        addr3,
        ssid,
        discovery,
    })
}

fn extract_ssid(mpdu: &[u8], ie_off: usize) -> Option<String> {
    let mut i = ie_off;
    while i + 2 <= mpdu.len() {
        let tag = mpdu[i];
        let len = mpdu[i + 1] as usize;
        i += 2;
        if i + len > mpdu.len() {
            return None;
        }
        if tag == 0 {
            return Some(String::from_utf8_lossy(&mpdu[i..i + len]).into_owned());
        }
        i += len;
    }
    None
}

pub fn discovery_mac(parsed: &ParsedWifi) -> Option<[u8; 6]> {
    match parsed.discovery {
        Some(Discovery::Ap { bssid }) => Some(bssid),
        Some(Discovery::Sta { mac }) => Some(mac),
        None => None,
    }
}

pub fn bssid_string(mac: &[u8; 6]) -> String {
    format_mac(mac)
}

pub fn beacon_fixture(bssid: [u8; 6], ssid: &str) -> Vec<u8> {
    mgmt_fixture(0x80, bssid, [0xff; 6], bssid, Some(ssid), true)
}

pub fn probe_resp_fixture(bssid: [u8; 6], ssid: &str) -> Vec<u8> {
    mgmt_fixture(0x50, bssid, [0xff; 6], bssid, Some(ssid), true)
}

pub fn probe_req_fixture(sta: [u8; 6], ssid: Option<&str>) -> Vec<u8> {
    mgmt_fixture(0x40, sta, [0xff; 6], [0xff; 6], ssid, false)
}

fn mgmt_fixture(
    fc0: u8,
    addr2: [u8; 6],
    addr1: [u8; 6],
    addr3: [u8; 6],
    ssid: Option<&str>,
    fixed12: bool,
) -> Vec<u8> {
    let mut f = vec![fc0, 0, 0, 0];
    f.extend_from_slice(&addr1);
    f.extend_from_slice(&addr2);
    f.extend_from_slice(&addr3);
    f.extend_from_slice(&[0, 0]);
    if fixed12 {
        f.extend_from_slice(&[0u8; 12]);
    }
    if let Some(s) = ssid {
        f.push(0);
        f.push(s.len() as u8);
        f.extend_from_slice(s.as_bytes());
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_beacon_probe_data_ctrl() {
        let bssid = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
        let beacon = beacon_fixture(bssid, "Flock");
        let p = parse_80211(&beacon).unwrap();
        assert_eq!(p.kind, FrameKind::Mgmt);
        assert_eq!(p.subtype, SUBTYPE_BEACON);
        assert_eq!(p.ssid.as_deref(), Some("Flock"));
        assert_eq!(p.discovery, Some(Discovery::Ap { bssid }));
        assert_eq!(discovery_mac(&p), Some(bssid));
        assert_eq!(bssid_string(&bssid), "11:22:33:44:55:66");
        let sta = parse_80211(&probe_req_fixture([0xaa; 6], Some("Flock"))).unwrap();
        assert_eq!(discovery_mac(&sta), Some([0xaa; 6]));
        let mut data_only = vec![0x08, 0, 0, 0];
        data_only.extend_from_slice(&[1u8; 20]);
        assert_eq!(discovery_mac(&parse_80211(&data_only).unwrap()), None);

        let pr = probe_req_fixture([0xaa; 6], Some("Flock"));
        let p = parse_80211(&pr).unwrap();
        assert_eq!(p.discovery, Some(Discovery::Sta { mac: [0xaa; 6] }));
        assert_eq!(p.ssid.as_deref(), Some("Flock"));

        let pv = probe_resp_fixture(bssid, "x");
        assert_eq!(parse_80211(&pv).unwrap().subtype, SUBTYPE_PROBE_RESP);

        let mut data = vec![0x08, 0, 0, 0];
        data.extend_from_slice(&[1u8; 20]);
        let p = parse_80211(&data).unwrap();
        assert_eq!(p.kind, FrameKind::Data);
        assert!(p.discovery.is_none());

        let mut ctrl = vec![0xb4, 0, 0, 0];
        ctrl.extend_from_slice(&[2u8; 12]);
        let p = parse_80211(&ctrl).unwrap();
        assert_eq!(p.kind, FrameKind::Ctrl);
        assert!(p.addr3.is_none());

        let mut other = vec![0x0c, 0, 0, 0];
        other.extend_from_slice(&[3u8; 20]);
        assert_eq!(parse_80211(&other).unwrap().kind, FrameKind::Other);

        assert!(parse_80211(&[0u8; 10]).is_err());
        let mut short_mgmt = vec![0x80, 0, 0, 0];
        short_mgmt.extend_from_slice(&[4u8; 12]);
        assert!(parse_80211(&short_mgmt).is_err());
        assert_eq!(clamp_copy_len(3000, 100, 2500), 100);
        assert_eq!(frame_type(0x80), 0);
        assert_eq!(frame_subtype(0x80), 8);
        let mut assoc = vec![0x00, 0, 0, 0];
        assoc.extend_from_slice(&[9u8; 20]);
        assert!(parse_80211(&assoc).unwrap().ssid.is_none());
        let mut trunc_ie = beacon_fixture(bssid, "ab");
        trunc_ie.pop();
        assert!(parse_80211(&trunc_ie).unwrap().ssid.is_none());
        let mut no_ssid = beacon_fixture(bssid, "ab");
        no_ssid.truncate(36);
        no_ssid.extend_from_slice(&[1, 1, 0]);
        assert!(parse_80211(&no_ssid).unwrap().ssid.is_none());
        assert!(parse_80211(&probe_req_fixture([0xbb; 6], None))
            .unwrap()
            .ssid
            .is_none());
    }
}
