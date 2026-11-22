use crate::mac::visible_text;

/// Parsed BLE advertising data fields used in discovery events.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AdvFields {
    pub name: Option<String>,
    pub company_id: Option<u16>,
    /// UAS ID from an ASTM F3411 Remote ID Basic ID message (service data 0xFFFA).
    pub remote_id: Option<String>,
}

/// Walk BLE AD structure (length, type, value) and pick name / company id.
pub fn parse_adv(data: &[u8]) -> AdvFields {
    let mut fields = AdvFields::default();
    let mut i = 0;
    while i < data.len() {
        let elen = data[i] as usize;
        if elen == 0 {
            break;
        }
        if i + 1 + elen > data.len() {
            break;
        }
        let typ = data[i + 1];
        let val = &data[i + 2..i + 1 + elen];
        match typ {
            0x08 | 0x09 => {
                if fields.name.is_none() || typ == 0x09 {
                    fields.name = Some(visible_text(&String::from_utf8_lossy(val)));
                }
            }
            0xff if val.len() >= 2 => {
                fields.company_id = Some(u16::from_le_bytes([val[0], val[1]]));
            }
            _ => {}
        }
        i += 1 + elen;
    }
    fields.remote_id = remoteid::ble::find_in_advertising_data(data)
        .and_then(|rid| rid.frame().ok())
        .and_then(uas_id);
    fields
}

/// First Basic ID UAS ID in a decoded Remote ID message or message pack.
pub fn uas_id(frame: remoteid::Frame<'_>) -> Option<String> {
    let id = |m: remoteid::Message| match m {
        remoteid::Message::BasicId(b) => b.uas_id_str().map(str::to_string),
        _ => None,
    };
    match frame {
        remoteid::Frame::Message(m) => id(m),
        remoteid::Frame::Pack(p) => p.messages().filter_map(Result::ok).find_map(id),
    }
}

/// NimBLE scan interval and window are in 0.625 ms units, spec range 4..=0x4000.
pub fn scan_units(ms: u16) -> u16 {
    let units = u32::from(ms) * 8 / 5;
    units.clamp(4, 0x4000) as u16
}

/// Scan window in NimBLE units, never longer than the interval.
pub fn scan_window(interval_ms: u16, window_ms: u16) -> u16 {
    scan_units(window_ms).min(scan_units(interval_ms))
}

/// Controller addresses are little-endian. Display order is the reverse.
pub fn display_addr(controller: [u8; 6]) -> [u8; 6] {
    let mut addr = controller;
    addr.reverse();
    addr
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_truncated() {
        assert_eq!(parse_adv(&[]), AdvFields::default());
        assert_eq!(parse_adv(&[0]), AdvFields::default());
        assert_eq!(parse_adv(&[3, 0x09, b'a']), AdvFields::default());
    }

    #[test]
    fn complete_name_wins_over_short() {
        let data = [0x03, 0x08, b'x', b'y', 0x04, 0x09, b'A', b'p', b'p'];
        let f = parse_adv(&data);
        assert_eq!(f.name.as_deref(), Some("App"));
    }

    #[test]
    fn short_name_and_company_id() {
        let data = [0x03, 0x08, b'n', b'1', 0x03, 0xff, 0x4c, 0x00];
        let f = parse_adv(&data);
        assert_eq!(f.name.as_deref(), Some("n1"));
        assert_eq!(f.company_id, Some(0x004c));
        let noisy = [0x05, 0x09, b'A', 0x1b, b'\n', b'B'];
        assert_eq!(parse_adv(&noisy).name.as_deref(), Some("AB"));
    }

    #[test]
    fn skips_unknown_and_short_mfg() {
        let data = [0x02, 0x01, 0x06, 0x02, 0xff, 0x4c];
        let f = parse_adv(&data);
        assert_eq!(f, AdvFields::default());
    }

    #[test]
    fn drone_remote_id_service_data() {
        let mut data = vec![0x1e, 0x16, 0xfa, 0xff, 0x0d, 0x07, 0x02, 0x12];
        data.extend_from_slice(b"1596F3507452");
        data.resize(31, 0);
        let f = parse_adv(&data);
        assert_eq!(f.remote_id.as_deref(), Some("1596F3507452"));
        assert_eq!(f.name, None);
    }

    #[test]
    fn scan_units_scale_and_clamp() {
        assert_eq!(scan_units(100), 160);
        assert_eq!(scan_units(30), 48);
        assert_eq!(scan_units(0), 4);
        assert_eq!(scan_units(2), 4);
        assert_eq!(scan_units(10_240), 0x4000);
        assert_eq!(scan_units(u16::MAX), 0x4000);
    }

    #[test]
    fn scan_window_does_not_exceed_interval() {
        assert_eq!(scan_window(100, 30), 48);
        assert_eq!(scan_window(30, 100), scan_units(30));
    }

    #[test]
    fn controller_addr_reverses_for_display() {
        assert_eq!(
            display_addr([0x06, 0x05, 0x04, 0x03, 0x02, 0x01]),
            [0x01, 0x02, 0x03, 0x04, 0x05, 0x06]
        );
        let shown = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06];
        assert_eq!(display_addr(display_addr(shown)), shown);
    }
}
