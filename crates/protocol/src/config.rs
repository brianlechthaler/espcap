use crate::filter::{FilterEngine, FilterSpec};
use crate::types::{
    format_hopmask, Chip, Mode, OutputFormat, Radio, WifiBand, WifiType, DEFAULT_DWELL_MS,
    DEFAULT_HOPMASK, MAX_DWELL_MS, MIN_DWELL_MS,
};
use crate::Error;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DropCounters {
    pub ring_overflow: u32,
    pub cdc_backpressure: u32,
    pub truncated: u32,
}

#[derive(Debug, Clone)]
pub struct DeviceConfig {
    pub radio: Radio,
    pub mode: Mode,
    pub format: OutputFormat,
    pub wifi_band: WifiBand,
    pub dwell_ms: u16,
    pub hopmask: u16,
    pub channels_5ghz: Vec<u8>,
    pub ble_interval_ms: u16,
    pub ble_window_ms: u16,
    pub ble_active: bool,
    pub wifi_types: Vec<WifiType>,
    pub filters: FilterEngine,
    pub running: bool,
}

impl Default for DeviceConfig {
    fn default() -> Self {
        Self {
            radio: Radio::Off,
            mode: Mode::Discovery,
            format: OutputFormat::Json,
            wifi_band: WifiBand::TwoG,
            dwell_ms: DEFAULT_DWELL_MS,
            hopmask: DEFAULT_HOPMASK,
            channels_5ghz: Vec::new(),
            ble_interval_ms: 100,
            ble_window_ms: 30,
            ble_active: false,
            wifi_types: vec![WifiType::Mgmt, WifiType::Data],
            filters: FilterEngine::default(),
            running: false,
        }
    }
}

impl DeviceConfig {
    #[allow(clippy::too_many_arguments)]
    pub fn apply_set(
        &mut self,
        radio: Option<Radio>,
        mode: Option<Mode>,
        format: Option<OutputFormat>,
        wifi_band: Option<WifiBand>,
        dwell_ms: Option<u16>,
        hopmask: Option<u16>,
        channels_5ghz: Option<Vec<u8>>,
        ble_interval_ms: Option<u16>,
        ble_window_ms: Option<u16>,
        ble_active: Option<bool>,
        wifi_types: Option<Vec<WifiType>>,
        filters: Option<FilterSpec>,
    ) -> Result<(), Error> {
        if let Some(d) = dwell_ms {
            if !(MIN_DWELL_MS..=MAX_DWELL_MS).contains(&d) {
                return Err(Error::config(format!(
                    "dwell_ms must be {MIN_DWELL_MS}-{MAX_DWELL_MS}"
                )));
            }
            self.dwell_ms = d;
        }
        if let Some(mask) = hopmask {
            crate::types::channels_from_hopmask(mask)?;
            self.hopmask = mask;
        }
        if let Some(chs) = channels_5ghz {
            for ch in &chs {
                if !(32..=177).contains(ch) {
                    return Err(Error::config(format!("invalid 5 GHz channel {ch}")));
                }
            }
            self.channels_5ghz = chs;
        }
        if let Some(types) = wifi_types {
            if types.is_empty() {
                return Err(Error::config("wifi_types must not be empty"));
            }
            self.wifi_types = types;
        }
        if let Some(spec) = filters {
            self.filters = FilterEngine::from_spec(&spec)?;
        }
        if let Some(v) = radio {
            self.radio = v;
        }
        if let Some(v) = mode {
            self.mode = v;
        }
        if let Some(v) = format {
            self.format = v;
        }
        if let Some(v) = wifi_band {
            self.wifi_band = v;
        }
        if let Some(v) = ble_interval_ms {
            self.ble_interval_ms = v;
        }
        if let Some(v) = ble_window_ms {
            self.ble_window_ms = v;
        }
        if let Some(v) = ble_active {
            self.ble_active = v;
        }
        Ok(())
    }

    pub fn validate_for_chip(&self, chip: Chip) -> Result<(), Error> {
        if !chip.supports_5ghz() {
            if self.wifi_band.uses_5g() {
                return Err(Error::config("ESP32-S3 supports 2.4 GHz WiFi only"));
            }
            if !self.channels_5ghz.is_empty() {
                return Err(Error::config("ESP32-S3 rejects channels_5ghz"));
            }
        }
        Ok(())
    }

    pub fn promiscuous_filter_mask(&self) -> u32 {
        let mut mask = 0u32;
        for t in &self.wifi_types {
            mask |= match t {
                WifiType::Mgmt => 0x0001,
                WifiType::Ctrl => 0x0002,
                WifiType::Data => 0x0004,
            };
        }
        mask
    }

    pub fn status_json(&self, chip: Chip, drops: &DropCounters) -> serde_json::Value {
        serde_json::json!({
            "event": "status",
            "chip": chip,
            "wifi_bands": chip.wifi_bands(),
            "radio": self.radio,
            "mode": self.mode,
            "format": self.format,
            "wifi_band": self.wifi_band,
            "dwell_ms": self.dwell_ms,
            "hopmask": format_hopmask(self.hopmask),
            "channels_5ghz": self.channels_5ghz,
            "ble_interval_ms": self.ble_interval_ms,
            "ble_window_ms": self.ble_window_ms,
            "ble_active": self.ble_active,
            "wifi_types": self.wifi_types,
            "running": self.running,
            "drops": drops,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::parse_line;
    use crate::command::Command;

    fn apply_cmd(cfg: &mut DeviceConfig, line: &str) -> Result<(), Error> {
        match parse_line(line)? {
            Command::Set {
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
            } => cfg.apply_set(
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
            ),
            Command::Get | Command::Start | Command::Stop | Command::Status => {
                Err(Error::UnknownCommand)
            }
        }
    }

    #[test]
    fn omitted_fields_keep_defaults() {
        let mut cfg = DeviceConfig::default();
        apply_cmd(&mut cfg, r#"{"cmd":"set","radio":"wifi"}"#).unwrap();
        assert_eq!(cfg.radio, Radio::Wifi);
        assert_eq!(cfg.dwell_ms, 300);
        assert_eq!(cfg.hopmask, DEFAULT_HOPMASK);
        assert_eq!(cfg.format, OutputFormat::Json);
        assert!(!cfg.ble_active);
        assert_eq!(cfg.promiscuous_filter_mask(), 0x0005);
    }

    #[test]
    fn dwell_and_types_and_5ghz_validation() {
        let mut cfg = DeviceConfig::default();
        assert!(apply_cmd(&mut cfg, r#"{"cmd":"set","dwell_ms":99}"#).is_err());
        assert!(apply_cmd(&mut cfg, r#"{"cmd":"set","dwell_ms":2001}"#).is_err());
        apply_cmd(&mut cfg, r#"{"cmd":"set","dwell_ms":250}"#).unwrap();
        assert_eq!(cfg.dwell_ms, 250);
        assert!(apply_cmd(&mut cfg, r#"{"cmd":"set","wifi_types":[]}"#).is_err());
        apply_cmd(&mut cfg, r#"{"cmd":"set","wifi_types":["ctrl"]}"#).unwrap();
        assert_eq!(cfg.promiscuous_filter_mask(), 0x0002);
        apply_cmd(
            &mut cfg,
            r#"{"cmd":"set","wifi_types":["mgmt","ctrl","data"]}"#,
        )
        .unwrap();
        assert_eq!(cfg.promiscuous_filter_mask(), 0x0007);
        assert!(apply_cmd(&mut cfg, r#"{"cmd":"set","channels_5ghz":[1]}"#).is_err());
        apply_cmd(&mut cfg, r#"{"cmd":"set","channels_5ghz":[36,165]}"#).unwrap();
        apply_cmd(&mut cfg, r#"{"cmd":"set","mode":"capture","format":"pcap","wifi_band":"both","ble_interval_ms":80,"ble_window_ms":80,"ble_active":true}"#).unwrap();
        assert_eq!(cfg.mode, Mode::Capture);
        assert!(cfg.ble_active);
        apply_cmd(&mut cfg, r#"{"cmd":"set","hopmask":"0x0002"}"#).unwrap();
        assert_eq!(cfg.hopmask, 0x0002);
        assert!(apply_cmd(&mut cfg, r#"{"cmd":"set","hopmask":"0x8000"}"#).is_err());
        apply_cmd(
            &mut cfg,
            r#"{"cmd":"set","filters":{"oui":["F4:4E:FC"],"ssid_regex":"^Flock"}}"#,
        )
        .unwrap();
        assert!(!cfg.filters.is_open());
        assert!(apply_cmd(&mut DeviceConfig::default(), r#"{"cmd":"get"}"#).is_err());
        assert!(apply_cmd(&mut DeviceConfig::default(), r#"{"cmd":"start"}"#).is_err());
        assert!(apply_cmd(&mut DeviceConfig::default(), r#"{"cmd":"stop"}"#).is_err());
        assert!(apply_cmd(&mut DeviceConfig::default(), r#"{"cmd":"status"}"#).is_err());
        assert!(apply_cmd(
            &mut DeviceConfig::default(),
            r#"{"cmd":"set","filters":{"name_regex":"("}}"#,
        )
        .is_err());
    }

    #[test]
    fn s3_rejects_5ghz_c5_accepts() {
        let mut cfg = DeviceConfig::default();
        apply_cmd(&mut cfg, r#"{"cmd":"set","wifi_band":"5"}"#).unwrap();
        assert!(cfg.validate_for_chip(Chip::Esp32s3).is_err());
        assert!(cfg.validate_for_chip(Chip::Esp32c5).is_ok());
        let mut cfg = DeviceConfig::default();
        apply_cmd(&mut cfg, r#"{"cmd":"set","wifi_band":"both"}"#).unwrap();
        assert!(cfg.validate_for_chip(Chip::Esp32s3).is_err());
        let mut cfg = DeviceConfig::default();
        apply_cmd(&mut cfg, r#"{"cmd":"set","channels_5ghz":[36]}"#).unwrap();
        assert!(cfg.validate_for_chip(Chip::Esp32s3).is_err());
        cfg.channels_5ghz.clear();
        assert!(cfg.validate_for_chip(Chip::Esp32s3).is_ok());
        let status = cfg.status_json(Chip::Esp32s3, &DropCounters::default());
        assert_eq!(status["chip"], "esp32s3");
        assert_eq!(status["wifi_bands"][0], "2.4");
        let status = cfg.status_json(Chip::Esp32c5, &DropCounters::default());
        assert_eq!(status["wifi_bands"][1], "5");
    }
}
