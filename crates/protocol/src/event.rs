use crate::mac::{format_mac, format_oui, oui_of, payload_hex};
use crate::Error;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    Ack,
    Error {
        msg: String,
    },
    Status(serde_json::Value),
    WifiAp {
        mac: String,
        oui: String,
        rssi: i8,
        channel: u8,
        freq_mhz: u16,
        ts_ms: u64,
        ssid: String,
        hit_count: u32,
        first_ts_ms: u64,
        last_ts_ms: u64,
    },
    WifiSta {
        mac: String,
        oui: String,
        rssi: i8,
        channel: u8,
        freq_mhz: u16,
        ts_ms: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ssid: Option<String>,
        hit_count: u32,
        first_ts_ms: u64,
        last_ts_ms: u64,
    },
    Ble {
        mac: String,
        oui: String,
        rssi: i8,
        channel: u8,
        freq_mhz: u16,
        ts_ms: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        company_id: Option<u16>,
        hit_count: u32,
        first_ts_ms: u64,
        last_ts_ms: u64,
        #[serde(default)]
        addr_type: String,
    },
    WifiFrame {
        mac: String,
        oui: String,
        rssi: i8,
        channel: u8,
        freq_mhz: u16,
        ts_ms: u64,
        payload_hex: String,
    },
    BleAdv {
        mac: String,
        oui: String,
        rssi: i8,
        channel: u8,
        freq_mhz: u16,
        ts_ms: u64,
        payload_hex: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
}

pub fn parse_event_line(line: &str) -> Result<Event, Error> {
    let line = line.trim().trim_end_matches('\r');
    if line.is_empty() {
        return Err(Error::UnknownEvent);
    }
    let v: serde_json::Value = serde_json::from_str(line)?;
    let ev = v
        .get("event")
        .and_then(|e| e.as_str())
        .ok_or(Error::UnknownEvent)?;
    match ev {
        "ack" | "error" | "wifi_ap" | "wifi_sta" | "ble" | "wifi_frame" | "ble_adv" => {
            serde_json::from_value(v).map_err(Into::into)
        }
        "status" => Ok(Event::Status(v)),
        _ => Err(Error::UnknownEvent),
    }
}

pub fn encode_event(ev: &Event) -> Result<String, Error> {
    match ev {
        Event::Status(v) => Ok(serde_json::to_string(v)?),
        other => Ok(serde_json::to_string(other)?),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn wifi_ap(
    mac: &[u8; 6],
    rssi: i8,
    channel: u8,
    freq_mhz: u16,
    ts_ms: u64,
    ssid: &str,
    hit_count: u32,
    first_ts_ms: u64,
    last_ts_ms: u64,
) -> Event {
    Event::WifiAp {
        mac: format_mac(mac),
        oui: format_oui(&oui_of(mac)),
        rssi,
        channel,
        freq_mhz,
        ts_ms,
        ssid: ssid.into(),
        hit_count,
        first_ts_ms,
        last_ts_ms,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn wifi_sta(
    mac: &[u8; 6],
    rssi: i8,
    channel: u8,
    freq_mhz: u16,
    ts_ms: u64,
    ssid: Option<&str>,
    hit_count: u32,
    first_ts_ms: u64,
    last_ts_ms: u64,
) -> Event {
    Event::WifiSta {
        mac: format_mac(mac),
        oui: format_oui(&oui_of(mac)),
        rssi,
        channel,
        freq_mhz,
        ts_ms,
        ssid: ssid.map(str::to_string),
        hit_count,
        first_ts_ms,
        last_ts_ms,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn ble_disc(
    mac: &[u8; 6],
    rssi: i8,
    ts_ms: u64,
    name: Option<&str>,
    company_id: Option<u16>,
    hit_count: u32,
    first_ts_ms: u64,
    last_ts_ms: u64,
    addr_random: bool,
) -> Event {
    Event::Ble {
        mac: format_mac(mac),
        oui: format_oui(&oui_of(mac)),
        rssi,
        channel: 39,
        freq_mhz: 2480,
        ts_ms,
        name: name.map(str::to_string),
        company_id,
        hit_count,
        first_ts_ms,
        last_ts_ms,
        addr_type: if addr_random {
            "random".into()
        } else {
            "public".into()
        },
    }
}

pub fn wifi_frame(
    mac: &[u8; 6],
    rssi: i8,
    channel: u8,
    freq_mhz: u16,
    ts_ms: u64,
    payload: &[u8],
) -> Event {
    Event::WifiFrame {
        mac: format_mac(mac),
        oui: format_oui(&oui_of(mac)),
        rssi,
        channel,
        freq_mhz,
        ts_ms,
        payload_hex: payload_hex(payload),
    }
}

pub fn ble_adv(mac: &[u8; 6], rssi: i8, ts_ms: u64, payload: &[u8], name: Option<&str>) -> Event {
    Event::BleAdv {
        mac: format_mac(mac),
        oui: format_oui(&oui_of(mac)),
        rssi,
        channel: 39,
        freq_mhz: 2480,
        ts_ms,
        payload_hex: payload_hex(payload),
        name: name.map(str::to_string),
    }
}

pub fn ack() -> Event {
    Event::Ack
}

pub fn error_event(msg: impl Into<String>) -> Event {
    Event::Error { msg: msg.into() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jsonl_round_trip_all_events() {
        let mac = [0xf4, 0x4e, 0xfc, 0, 1, 2];
        let events = [
            ack(),
            error_event("nope"),
            wifi_ap(&mac, -40, 6, 2437, 10, "Flock", 2, 1, 10),
            wifi_sta(&mac, -50, 1, 2412, 11, Some("x"), 1, 11, 11),
            wifi_sta(&mac, -50, 1, 2412, 11, None, 1, 11, 11),
            ble_disc(&mac, -60, 12, Some("AirPods"), Some(0x4c), 3, 1, 12, true),
            ble_disc(&mac, -60, 12, None, None, 1, 12, 12, false),
            wifi_frame(&mac, -30, 36, 5180, 13, &[0x80, 0x00]),
            ble_adv(&mac, -70, 14, &[0xaa], Some("n")),
            Event::Status(serde_json::json!({"event":"status","chip":"esp32s3"})),
        ];
        for ev in events {
            let line = encode_event(&ev).unwrap();
            assert!(!line.contains('\n'));
            let parsed = parse_event_line(&format!("{line}\r")).unwrap();
            assert_eq!(encode_event(&parsed).unwrap(), line);
        }
        let status = parse_event_line(r#"{"event":"status","chip":"esp32s3"}"#).unwrap();
        assert!(matches!(status, Event::Status(_)));
        assert_eq!(
            encode_event(&Event::Status(
                serde_json::json!({"event":"status","chip":"esp32c5"})
            ))
            .unwrap(),
            r#"{"chip":"esp32c5","event":"status"}"#
        );
        assert!(parse_event_line("").is_err());
        assert!(parse_event_line("{}").is_err());
        assert!(parse_event_line("{\"event\":\"nope\"}").is_err());
        assert!(parse_event_line("{").is_err());
        assert!(parse_event_line("{\"event\":\"ack\",\"extra\":1}").is_ok());
        assert!(parse_event_line("{\"event\":\"ack\"}").is_ok());
    }
}
