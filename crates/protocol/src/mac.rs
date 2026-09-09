use crate::Error;

pub fn parse_mac(s: &str) -> Result<[u8; 6], Error> {
    parse_octets::<6>(s)
}

pub fn parse_oui(s: &str) -> Result<[u8; 3], Error> {
    parse_octets::<3>(s)
}

pub fn format_mac(mac: &[u8; 6]) -> String {
    format_octets(mac)
}

pub fn format_oui(oui: &[u8; 3]) -> String {
    format_octets(oui)
}

pub fn oui_of(mac: &[u8; 6]) -> [u8; 3] {
    [mac[0], mac[1], mac[2]]
}

fn parse_octets<const N: usize>(s: &str) -> Result<[u8; N], Error> {
    let hex: String = s.chars().filter(|c| *c != ':' && *c != '-').collect();
    if hex.len() != N * 2 {
        return Err(Error::Mac(s.into()));
    }
    let mut out = [0u8; N];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        out[i] = u8::from_str_radix(std::str::from_utf8(chunk).unwrap(), 16)
            .map_err(|_| Error::Mac(s.into()))?;
    }
    Ok(out)
}

fn format_octets(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}

pub fn payload_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mac_round_trip_colon_and_bare() {
        let mac = parse_mac("F4:4E:FC:00:11:22").unwrap();
        assert_eq!(mac, [0xf4, 0x4e, 0xfc, 0x00, 0x11, 0x22]);
        assert_eq!(format_mac(&mac), "f4:4e:fc:00:11:22");
        assert_eq!(parse_mac("f44efc001122").unwrap(), mac);
        assert_eq!(parse_mac("F4-4E-FC-00-11-22").unwrap(), mac);
        assert_eq!(oui_of(&mac), [0xf4, 0x4e, 0xfc]);
        assert_eq!(format_oui(&oui_of(&mac)), "f4:4e:fc");
        assert_eq!(parse_oui("F4:4E:FC").unwrap(), [0xf4, 0x4e, 0xfc]);
    }

    #[test]
    fn mac_rejects_bad_input() {
        assert!(parse_mac("aa:bb").is_err());
        assert!(parse_mac("zz:zz:zz:zz:zz:zz").is_err());
        assert!(parse_oui("gggggg").is_err());
        assert_eq!(payload_hex(&[0xde, 0xad]), "dead");
    }
}
