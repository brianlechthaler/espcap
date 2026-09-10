/// Parsed BLE advertising data fields used in discovery events.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AdvFields {
    pub name: Option<String>,
    pub company_id: Option<u16>,
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
                    fields.name = Some(String::from_utf8_lossy(val).into_owned());
                }
            }
            0xff if val.len() >= 2 => {
                fields.company_id = Some(u16::from_le_bytes([val[0], val[1]]));
            }
            _ => {}
        }
        i += 1 + elen;
    }
    fields
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
    }

    #[test]
    fn skips_unknown_and_short_mfg() {
        let data = [0x02, 0x01, 0x06, 0x02, 0xff, 0x4c];
        let f = parse_adv(&data);
        assert_eq!(f, AdvFields::default());
    }
}
