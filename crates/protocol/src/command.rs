use crate::filter::FilterSpec;
use crate::types::{
    deserialize_opt_hopmask, serialize_opt_hopmask, Mode, OutputFormat, Radio, WifiBand, WifiType,
};
use crate::Error;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    Get,
    Start,
    Stop,
    Status,
    Set {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        radio: Option<Radio>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mode: Option<Mode>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        format: Option<OutputFormat>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        wifi_band: Option<WifiBand>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dwell_ms: Option<u16>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "deserialize_opt_hopmask",
            serialize_with = "serialize_opt_hopmask"
        )]
        hopmask: Option<u16>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        channels_5ghz: Option<Vec<u8>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ble_interval_ms: Option<u16>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ble_window_ms: Option<u16>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ble_active: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        wifi_types: Option<Vec<WifiType>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        filters: Option<FilterSpec>,
    },
}

pub fn parse_line(line: &str) -> Result<Command, Error> {
    let line = line.trim().trim_end_matches('\r');
    if line.is_empty() {
        return Err(Error::UnknownCommand);
    }
    let stream = serde_json::Deserializer::from_str(line).into_iter::<serde_json::Value>();
    let mut last = Error::UnknownCommand;
    for item in stream {
        let v = item?;
        let Some(cmd) = v.get("cmd").and_then(|c| c.as_str()) else {
            last = Error::UnknownCommand;
            continue;
        };
        return match cmd {
            "get" | "start" | "stop" | "status" | "set" => {
                serde_json::from_value(v).map_err(Into::into)
            }
            _ => Err(Error::UnknownCommand),
        };
    }
    Err(last)
}

pub fn encode_line(cmd: &Command) -> Result<String, Error> {
    Ok(serde_json::to_string(cmd)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_cmds_and_crlf() {
        for s in ["get", "start", "stop", "status"] {
            let cmd = parse_line(&format!("{{\"cmd\":\"{s}\"}}\r\n")).unwrap();
            let out = encode_line(&cmd).unwrap();
            assert!(out.contains(s));
        }
        assert!(parse_line("").is_err());
        assert!(parse_line("{\"cmd\":\"nope\"}").is_err());
        assert!(parse_line("{").is_err());
        assert!(parse_line("{}").is_err());
        assert!(matches!(
            parse_line("{\"event\":\"ack\"}{\"cmd\":\"status\"}").unwrap(),
            Command::Status
        ));
    }

    #[test]
    fn parse_set_partial_and_hopmask() {
        let cmd = parse_line(
            r#"{"cmd":"set","radio":"both","mode":"discovery","format":"json","wifi_band":"2.4","dwell_ms":300,"hopmask":"0x0421","channels_5ghz":[36,40],"ble_interval_ms":100,"ble_window_ms":30,"ble_active":false,"wifi_types":["mgmt","data"]}"#,
        )
        .unwrap();
        assert!(matches!(
            cmd,
            Command::Set {
                radio: Some(Radio::Both),
                hopmask: Some(0x0421),
                wifi_band: Some(WifiBand::TwoG),
                wifi_types: Some(_),
                ..
            }
        ));
        let encoded = encode_line(&Command::Set {
            radio: Some(Radio::Off),
            mode: None,
            format: None,
            wifi_band: None,
            dwell_ms: None,
            hopmask: Some(0x1),
            channels_5ghz: None,
            ble_interval_ms: None,
            ble_window_ms: None,
            ble_active: None,
            wifi_types: None,
            filters: None,
        })
        .unwrap();
        assert!(encoded.contains("0x0001"));
        assert!(!encoded.contains("dwell_ms"));
    }
}
