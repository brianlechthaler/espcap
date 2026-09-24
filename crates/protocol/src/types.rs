use crate::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub const DEFAULT_HOPMASK: u16 = 0x0421;
pub const DEFAULT_DWELL_MS: u16 = 300;
pub const MIN_DWELL_MS: u16 = 100;
pub const MAX_DWELL_MS: u16 = 2000;
pub const SNAPLEN: u32 = 2500;
pub const DEFAULT_CHANNELS_5GHZ: [u8; 4] = [36, 40, 44, 48];
pub const FLOCKYOU_2G_CHANNELS: [u8; 3] = [11, 6, 1];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Chip {
    Esp32s3,
    Esp32c5,
}

impl Chip {
    pub fn wifi_bands(self) -> &'static [&'static str] {
        match self {
            Chip::Esp32s3 => &["2.4"],
            Chip::Esp32c5 => &["2.4", "5"],
        }
    }

    pub fn supports_5ghz(self) -> bool {
        matches!(self, Chip::Esp32c5)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Radio {
    Wifi,
    Ble,
    Both,
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Discovery,
    Capture,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    Json,
    Pcap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WifiBand {
    #[serde(rename = "2.4")]
    TwoG,
    #[serde(rename = "5")]
    FiveG,
    #[serde(rename = "both")]
    Both,
}

impl WifiBand {
    pub fn uses_2g(self) -> bool {
        !matches!(self, WifiBand::FiveG)
    }

    pub fn uses_5g(self) -> bool {
        !matches!(self, WifiBand::TwoG)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WifiType {
    Mgmt,
    Ctrl,
    Data,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HopChannel {
    pub channel: u8,
    pub freq_mhz: u16,
    pub is_5ghz: bool,
}

pub fn freq_mhz(channel: u8, is_5ghz: bool) -> u16 {
    if is_5ghz {
        5000 + 5 * u16::from(channel)
    } else if channel == 14 {
        2484
    } else {
        2407 + 5 * u16::from(channel)
    }
}

pub fn parse_hopmask(s: &str) -> Result<u16, Error> {
    let t = s.trim();
    let hex = t
        .strip_prefix("0x")
        .or_else(|| t.strip_prefix("0X"))
        .unwrap_or(t);
    u16::from_str_radix(hex, 16).map_err(|_| Error::Hopmask(s.into()))
}

pub fn format_hopmask(mask: u16) -> String {
    format!("0x{mask:04x}")
}

pub fn channels_from_hopmask(mask: u16) -> Result<Vec<u8>, Error> {
    if mask & !0x3fff != 0 {
        return Err(Error::Hopmask(format_hopmask(mask)));
    }
    Ok((1u8..=14)
        .filter(|ch| mask & (1 << (ch - 1)) != 0)
        .collect())
}

pub fn hop_sequence(
    band: WifiBand,
    hopmask: u16,
    channels_5ghz: &[u8],
) -> Result<Vec<HopChannel>, Error> {
    let ch2 = if band.uses_2g() {
        channels_from_hopmask(hopmask)?
    } else {
        Vec::new()
    };
    let ch5: Vec<u8> = if band.uses_5g() {
        if channels_5ghz.is_empty() {
            DEFAULT_CHANNELS_5GHZ.to_vec()
        } else {
            channels_5ghz.to_vec()
        }
    } else {
        Vec::new()
    };
    if ch2.is_empty() && ch5.is_empty() {
        return Err(Error::config("hop sequence is empty"));
    }
    if !band.uses_2g() || !band.uses_5g() || ch2.is_empty() || ch5.is_empty() {
        let mut seq = Vec::new();
        seq.extend(ch2.into_iter().map(|channel| HopChannel {
            channel,
            freq_mhz: freq_mhz(channel, false),
            is_5ghz: false,
        }));
        seq.extend(ch5.into_iter().map(|channel| HopChannel {
            channel,
            freq_mhz: freq_mhz(channel, true),
            is_5ghz: true,
        }));
        return Ok(seq);
    }
    let mut seq = Vec::new();
    let n = ch2.len().max(ch5.len());
    for i in 0..n {
        if i < ch2.len() {
            seq.push(HopChannel {
                channel: ch2[i],
                freq_mhz: freq_mhz(ch2[i], false),
                is_5ghz: false,
            });
        }
        if i < ch5.len() {
            seq.push(HopChannel {
                channel: ch5[i],
                freq_mhz: freq_mhz(ch5[i], true),
                is_5ghz: true,
            });
        }
    }
    Ok(seq)
}

pub fn deserialize_opt_hopmask<'de, D>(deserializer: D) -> Result<Option<u16>, D::Error>
where
    D: Deserializer<'de>,
{
    let v = Option::<serde_json::Value>::deserialize(deserializer)?;
    match v {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(s)) => parse_hopmask(&s)
            .map(Some)
            .map_err(serde::de::Error::custom),
        Some(serde_json::Value::Number(n)) => {
            let n = n
                .as_u64()
                .ok_or_else(|| serde::de::Error::custom("hopmask too large"))?;
            if n > u64::from(u16::MAX) {
                return Err(serde::de::Error::custom("hopmask too large"));
            }
            Ok(Some(n as u16))
        }
        Some(_) => Err(serde::de::Error::custom(
            "hopmask must be hex string or integer",
        )),
    }
}

pub fn serialize_hopmask<S>(mask: &u16, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&format_hopmask(*mask))
}

pub fn serialize_opt_hopmask<S>(mask: &Option<u16>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match mask {
        Some(m) => serializer.serialize_str(&format_hopmask(*m)),
        None => serializer.serialize_none(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_mask_is_1_6_11() {
        assert_eq!(
            channels_from_hopmask(DEFAULT_HOPMASK).unwrap(),
            vec![1, 6, 11]
        );
    }

    #[test]
    fn hopmask_parse_and_high_bits() {
        assert_eq!(parse_hopmask("0x0421").unwrap(), 0x0421);
        assert_eq!(parse_hopmask("0X0421").unwrap(), 0x0421);
        assert_eq!(parse_hopmask("421").unwrap(), 0x0421);
        assert!(parse_hopmask("zz").is_err());
        assert!(channels_from_hopmask(0x8000).is_err());
        assert_eq!(format_hopmask(0x0421), "0x0421");
    }

    #[test]
    fn freq_2g_and_5g() {
        assert_eq!(freq_mhz(1, false), 2412);
        assert_eq!(freq_mhz(6, false), 2437);
        assert_eq!(freq_mhz(11, false), 2462);
        assert_eq!(freq_mhz(14, false), 2484);
        assert_eq!(freq_mhz(36, true), 5180);
        assert_eq!(freq_mhz(149, true), 5745);
    }

    #[test]
    fn hop_2g_only_5g_only_and_interleave() {
        let two = hop_sequence(WifiBand::TwoG, DEFAULT_HOPMASK, &[]).unwrap();
        assert_eq!(
            two.iter().map(|h| h.channel).collect::<Vec<_>>(),
            vec![1, 6, 11]
        );
        assert!(two.iter().all(|h| !h.is_5ghz));
        let five = hop_sequence(WifiBand::FiveG, DEFAULT_HOPMASK, &[]).unwrap();
        assert_eq!(
            five.iter().map(|h| h.channel).collect::<Vec<_>>(),
            vec![36, 40, 44, 48]
        );
        assert!(five.iter().all(|h| h.is_5ghz));
        let both = hop_sequence(WifiBand::Both, DEFAULT_HOPMASK, &[36, 40]).unwrap();
        assert_eq!(
            both.iter()
                .map(|h| (h.channel, h.is_5ghz))
                .collect::<Vec<_>>(),
            vec![(1, false), (36, true), (6, false), (40, true), (11, false)]
        );
        assert!(hop_sequence(WifiBand::TwoG, 0, &[]).is_err());
        let flock = hop_sequence(WifiBand::TwoG, 0x0421, &[]).unwrap();
        assert_eq!(FLOCKYOU_2G_CHANNELS.len(), 3);
        assert_eq!(flock[0].freq_mhz, 2412);
    }

    #[test]
    fn chip_bands() {
        assert_eq!(Chip::Esp32s3.wifi_bands(), &["2.4"]);
        assert_eq!(Chip::Esp32c5.wifi_bands(), &["2.4", "5"]);
        assert!(!Chip::Esp32s3.supports_5ghz());
        assert!(Chip::Esp32c5.supports_5ghz());
        assert!(WifiBand::Both.uses_2g() && WifiBand::Both.uses_5g());
        assert!(!WifiBand::FiveG.uses_2g());
        assert!(!WifiBand::TwoG.uses_5g());
    }

    #[test]
    fn hopmask_serde_helpers() {
        #[derive(Serialize, Deserialize)]
        struct Wrap {
            #[serde(
                deserialize_with = "deserialize_opt_hopmask",
                serialize_with = "serialize_opt_hopmask"
            )]
            hopmask: Option<u16>,
        }
        assert_eq!(
            serde_json::from_str::<Wrap>(r#"{"hopmask":"0x0421"}"#)
                .unwrap()
                .hopmask,
            Some(0x0421)
        );
        assert_eq!(
            serde_json::from_str::<Wrap>(r#"{"hopmask":1057}"#)
                .unwrap()
                .hopmask,
            Some(0x0421)
        );
        assert_eq!(
            serde_json::from_str::<Wrap>(r#"{"hopmask":null}"#)
                .unwrap()
                .hopmask,
            None
        );
        assert!(serde_json::from_str::<Wrap>(r#"{"hopmask":true}"#).is_err());
        assert!(serde_json::from_str::<Wrap>(r#"{"hopmask":999999}"#).is_err());
        assert!(serde_json::from_str::<Wrap>(r#"{"hopmask":-1}"#).is_err());
        assert!(serde_json::from_str::<Wrap>(r#"{"hopmask":"zz"}"#).is_err());
        let json = serde_json::to_string(&Wrap {
            hopmask: Some(0x0421),
        })
        .unwrap();
        assert!(json.contains("0x0421"));
        assert_eq!(
            serde_json::to_string(&Wrap { hopmask: None }).unwrap(),
            r#"{"hopmask":null}"#
        );
        #[derive(Serialize)]
        struct Req {
            #[serde(serialize_with = "serialize_hopmask")]
            hopmask: u16,
        }
        assert_eq!(
            serde_json::to_string(&Req { hopmask: 1 }).unwrap(),
            r#"{"hopmask":"0x0001"}"#
        );
    }
}
