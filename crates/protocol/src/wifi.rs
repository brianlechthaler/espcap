use crate::mac::{format_mac, visible_text};
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
    /// AP first, then up to two stations (probe-response receiver, data SA/DA).
    pub discovery: [Option<Discovery>; 3],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    let mut discovery = [None, None, None];
    match (kind, subtype) {
        (FrameKind::Mgmt, SUBTYPE_BEACON | SUBTYPE_PROBE_RESP) => {
            let bssid = addr3.unwrap_or(addr2);
            push_disc(&mut discovery, Discovery::Ap { bssid });
            if subtype == SUBTYPE_PROBE_RESP && addr1 != bssid {
                push_sta(&mut discovery, addr1);
            }
        }
        (FrameKind::Mgmt, SUBTYPE_PROBE_REQ) => {
            push_disc(&mut discovery, Discovery::Sta { mac: addr2 });
        }
        (FrameKind::Data, _) => match (mpdu[1] & 0x01 != 0, mpdu[1] & 0x02 != 0) {
            (false, true) => push_sta(&mut discovery, addr1),
            (true, false) => push_sta(&mut discovery, addr2),
            (false, false) => {
                push_sta(&mut discovery, addr1);
                if addr2 != addr1 {
                    push_sta(&mut discovery, addr2);
                }
            }
            (true, true) => {}
        },
        _ => {}
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
            let raw = &mpdu[i..i + len];
            let n = raw
                .iter()
                .rposition(|&b| b != 0)
                .map(|j| j + 1)
                .unwrap_or(0);
            return Some(visible_text(&String::from_utf8_lossy(&raw[..n])));
        }
        i += len;
    }
    None
}

fn is_unicast(mac: &[u8; 6]) -> bool {
    mac[0] & 1 == 0
}

fn push_disc(out: &mut [Option<Discovery>; 3], item: Discovery) {
    if let Some(slot) = out.iter_mut().find(|s| s.is_none()) {
        *slot = Some(item);
    }
}

fn push_sta(out: &mut [Option<Discovery>; 3], mac: [u8; 6]) {
    if is_unicast(&mac) {
        push_disc(out, Discovery::Sta { mac });
    }
}

pub fn discovery_mac(parsed: &ParsedWifi) -> Option<[u8; 6]> {
    parsed
        .discovery
        .into_iter()
        .flatten()
        .next()
        .map(|d| match d {
            Discovery::Ap { bssid } => bssid,
            Discovery::Sta { mac } => mac,
        })
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
        assert_eq!(p.discovery, [Some(Discovery::Ap { bssid }), None, None]);
        assert_eq!(discovery_mac(&p), Some(bssid));
        assert_eq!(bssid_string(&bssid), "11:22:33:44:55:66");
        let sta = parse_80211(&probe_req_fixture([0xaa; 6], Some("Flock"))).unwrap();
        assert_eq!(discovery_mac(&sta), Some([0xaa; 6]));
        let mut data_only = vec![0x08, 0, 0, 0];
        data_only.extend_from_slice(&[1u8; 20]);
        assert_eq!(discovery_mac(&parse_80211(&data_only).unwrap()), None);

        let pr = probe_req_fixture([0xaa; 6], Some("Flock"));
        let p = parse_80211(&pr).unwrap();
        assert_eq!(
            p.discovery,
            [Some(Discovery::Sta { mac: [0xaa; 6] }), None, None]
        );
        assert_eq!(p.ssid.as_deref(), Some("Flock"));

        let pv = probe_resp_fixture(bssid, "x");
        assert_eq!(parse_80211(&pv).unwrap().subtype, SUBTYPE_PROBE_RESP);

        let mut data = vec![0x08, 0, 0, 0];
        data.extend_from_slice(&[1u8; 20]);
        let p = parse_80211(&data).unwrap();
        assert_eq!(p.kind, FrameKind::Data);
        assert_eq!(p.discovery, [None, None, None]);

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

    fn data_frame(fc1: u8, addr1: [u8; 6], addr2: [u8; 6]) -> Vec<u8> {
        let mut f = vec![0x08, fc1, 0, 0];
        f.extend_from_slice(&addr1);
        f.extend_from_slice(&addr2);
        f.extend_from_slice(&[0x11; 6]);
        f.extend_from_slice(&[0, 0]);
        f
    }

    #[test]
    fn probe_resp_and_data_discover_stations() {
        let bssid = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
        let sta = [0x3c, 0x71, 0xbf, 0xd2, 0x2a, 0x70];
        let other = [0x02, 0x11, 0x22, 0x33, 0x44, 0x55];
        let mut pv = probe_resp_fixture(bssid, "x");
        pv[4..10].copy_from_slice(&sta);
        assert_eq!(
            parse_80211(&pv).unwrap().discovery,
            [
                Some(Discovery::Ap { bssid }),
                Some(Discovery::Sta { mac: sta }),
                None
            ]
        );
        let mut same = probe_resp_fixture(bssid, "x");
        same[4..10].copy_from_slice(&bssid);
        assert_eq!(
            parse_80211(&same).unwrap().discovery,
            [Some(Discovery::Ap { bssid }), None, None]
        );
        let mut beacon = beacon_fixture(bssid, "x");
        beacon[4..10].copy_from_slice(&sta);
        assert_eq!(
            parse_80211(&beacon).unwrap().discovery,
            [Some(Discovery::Ap { bssid }), None, None]
        );

        assert_eq!(
            parse_80211(&data_frame(0x02, sta, bssid))
                .unwrap()
                .discovery,
            [Some(Discovery::Sta { mac: sta }), None, None]
        );
        assert_eq!(
            parse_80211(&data_frame(0x01, bssid, sta))
                .unwrap()
                .discovery,
            [Some(Discovery::Sta { mac: sta }), None, None]
        );
        assert_eq!(
            parse_80211(&data_frame(0x00, sta, other))
                .unwrap()
                .discovery,
            [
                Some(Discovery::Sta { mac: sta }),
                Some(Discovery::Sta { mac: other }),
                None
            ]
        );
        assert_eq!(
            parse_80211(&data_frame(0x00, sta, sta)).unwrap().discovery,
            [Some(Discovery::Sta { mac: sta }), None, None]
        );
        assert_eq!(
            parse_80211(&data_frame(0x02, [0x01, 0, 0, 0, 0, 1], bssid))
                .unwrap()
                .discovery,
            [None, None, None]
        );
        assert_eq!(
            parse_80211(&data_frame(0x01, bssid, [0xff; 6]))
                .unwrap()
                .discovery,
            [None, None, None]
        );
        assert_eq!(
            parse_80211(&data_frame(0x03, sta, other))
                .unwrap()
                .discovery,
            [None, None, None]
        );
        assert_eq!(
            parse_80211(&data_frame(0x00, [0x01; 6], other))
                .unwrap()
                .discovery,
            [Some(Discovery::Sta { mac: other }), None, None]
        );
    }

    #[test]
    fn ssid_strips_trailing_nuls_and_invalid_bytes() {
        let bssid = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
        let padded = beacon_fixture(bssid, &(String::from("Flock") + "\0\0\0"));
        assert_eq!(parse_80211(&padded).unwrap().ssid.as_deref(), Some("Flock"));
        let hidden = beacon_fixture(bssid, &"\0".repeat(32));
        assert_eq!(parse_80211(&hidden).unwrap().ssid.as_deref(), Some(""));
        let mut junk = beacon_fixture(bssid, "ab");
        let ie = junk.len() - 2;
        junk[ie] = 0xff;
        junk[ie + 1] = 0xfe;
        assert_eq!(parse_80211(&junk).unwrap().ssid.as_deref(), Some(""));
    }
}
