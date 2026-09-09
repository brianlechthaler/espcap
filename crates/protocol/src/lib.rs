pub mod ble;
pub mod command;
pub mod config;
pub mod error;
pub mod event;
pub mod filter;
pub mod mac;
pub mod pcap;
pub mod types;
pub mod wifi;

pub use command::{encode_line, parse_line, Command};
pub use config::{DeviceConfig, DropCounters};
pub use error::Error;
pub use event::{
    ack, ble_adv, ble_disc, encode_event, error_event, parse_event_line, wifi_ap, wifi_frame,
    wifi_sta, Event,
};
pub use filter::{FilterEngine, FilterSpec};
pub use types::{
    Chip, HopChannel, Mode, OutputFormat, Radio, WifiBand, WifiType, DEFAULT_CHANNELS_5GHZ,
    DEFAULT_HOPMASK,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reexports_compile() {
        let _ = Chip::Esp32s3;
        let _ = Radio::Both;
        let _ = parse_line(r#"{"cmd":"get"}"#).unwrap();
        let _ = DeviceConfig::default();
        let _ = DeviceConfig::default().persist().unwrap();
        let _ = FilterSpec::default();
        let _ = DEFAULT_HOPMASK;
        let _ = DEFAULT_CHANNELS_5GHZ;
        let _ = HopChannel {
            channel: 1,
            freq_mhz: 2412,
            is_5ghz: false,
        };
        let _ = Mode::Discovery;
        let _ = OutputFormat::Json;
        let _ = WifiBand::TwoG;
        let _ = WifiType::Mgmt;
        let _ = ack();
        let _ = error_event("e");
        let mac = [0u8; 6];
        let _ = wifi_ap(&mac, 0, 1, 2412, 0, "", 1, 0, 0);
        let _ = wifi_sta(&mac, 0, 1, 2412, 0, None, 1, 0, 0);
        let _ = wifi_frame(&mac, 0, 1, 2412, 0, &[]);
        let _ = ble_disc(&mac, 0, 0, None, None, 1, 0, 0, false);
        let _ = ble_adv(&mac, 0, 0, &[], None);
        let _ = encode_event(&ack()).unwrap();
        let _ = parse_event_line(r#"{"event":"ack"}"#).unwrap();
        let _ = encode_line(&Command::Get).unwrap();
        let _ = FilterEngine::default();
        let _ = DropCounters::default();
        let _ = Event::Ack;
        let _ = Error::UnknownCommand;
        let _ = ble::parse_adv(&[]);
    }
}
