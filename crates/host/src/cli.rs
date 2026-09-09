use clap::{Parser, Subcommand, ValueEnum};
use espcap_protocol::{Command, FilterSpec, Mode, OutputFormat, Radio, WifiBand, WifiType};

#[derive(Debug, Parser, PartialEq, Eq)]
#[command(name = "espcap", about = "Configure and record from espcap firmware")]
pub struct Cli {
    #[arg(long)]
    pub port: String,
    #[arg(long, default_value_t = 115200)]
    pub baud: u32,
    #[command(subcommand)]
    pub cmd: Cmd,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum Cmd {
    Status,
    Stop,
    Set {
        #[arg(long)]
        radio: Option<RadioArg>,
        #[arg(long)]
        mode: Option<ModeArg>,
        #[arg(long)]
        format: Option<FormatArg>,
        #[arg(long)]
        wifi_band: Option<WifiBandArg>,
        #[arg(long)]
        dwell_ms: Option<u16>,
        #[arg(long)]
        hopmask: Option<String>,
        #[arg(long, value_delimiter = ',')]
        channels_5ghz: Option<Vec<u8>>,
        #[arg(long)]
        ble_interval_ms: Option<u16>,
        #[arg(long)]
        ble_window_ms: Option<u16>,
        #[arg(long)]
        ble_active: Option<bool>,
        #[arg(long, value_delimiter = ',')]
        wifi_types: Option<Vec<WifiTypeArg>>,
        #[arg(long, value_delimiter = ',')]
        oui: Option<Vec<String>>,
        #[arg(long, value_delimiter = ',')]
        mac: Option<Vec<String>>,
        #[arg(long)]
        name_regex: Option<String>,
        #[arg(long)]
        ssid_regex: Option<String>,
        #[arg(long)]
        manufacturer_regex: Option<String>,
        #[arg(long, value_delimiter = ',')]
        company_id: Option<Vec<u16>>,
    },
    Start {
        #[arg(long, conflicts_with = "pcap")]
        json: Option<String>,
        #[arg(long, conflicts_with = "json")]
        pcap: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum RadioArg {
    Wifi,
    Ble,
    Both,
    Off,
}

impl From<RadioArg> for Radio {
    fn from(v: RadioArg) -> Self {
        match v {
            RadioArg::Wifi => Radio::Wifi,
            RadioArg::Ble => Radio::Ble,
            RadioArg::Both => Radio::Both,
            RadioArg::Off => Radio::Off,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ModeArg {
    Discovery,
    Capture,
}

impl From<ModeArg> for Mode {
    fn from(v: ModeArg) -> Self {
        match v {
            ModeArg::Discovery => Mode::Discovery,
            ModeArg::Capture => Mode::Capture,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum FormatArg {
    Json,
    Pcap,
}

impl From<FormatArg> for OutputFormat {
    fn from(v: FormatArg) -> Self {
        match v {
            FormatArg::Json => OutputFormat::Json,
            FormatArg::Pcap => OutputFormat::Pcap,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum WifiBandArg {
    #[value(name = "2.4")]
    TwoG,
    #[value(name = "5")]
    FiveG,
    Both,
}

impl From<WifiBandArg> for WifiBand {
    fn from(v: WifiBandArg) -> Self {
        match v {
            WifiBandArg::TwoG => WifiBand::TwoG,
            WifiBandArg::FiveG => WifiBand::FiveG,
            WifiBandArg::Both => WifiBand::Both,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum WifiTypeArg {
    Mgmt,
    Ctrl,
    Data,
}

impl From<WifiTypeArg> for WifiType {
    fn from(v: WifiTypeArg) -> Self {
        match v {
            WifiTypeArg::Mgmt => WifiType::Mgmt,
            WifiTypeArg::Ctrl => WifiType::Ctrl,
            WifiTypeArg::Data => WifiType::Data,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn set_command(
    radio: Option<RadioArg>,
    mode: Option<ModeArg>,
    format: Option<FormatArg>,
    wifi_band: Option<WifiBandArg>,
    dwell_ms: Option<u16>,
    hopmask: Option<u16>,
    channels_5ghz: Option<Vec<u8>>,
    ble_interval_ms: Option<u16>,
    ble_window_ms: Option<u16>,
    ble_active: Option<bool>,
    wifi_types: Option<Vec<WifiTypeArg>>,
    filters: Option<FilterSpec>,
) -> Command {
    Command::Set {
        radio: radio.map(Into::into),
        mode: mode.map(Into::into),
        format: format.map(Into::into),
        wifi_band: wifi_band.map(Into::into),
        dwell_ms,
        hopmask,
        channels_5ghz,
        ble_interval_ms,
        ble_window_ms,
        ble_active,
        wifi_types: wifi_types.map(|v| v.into_iter().map(Into::into).collect()),
        filters,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parse_status_and_set() {
        let cli = Cli::parse_from(["espcap", "--port", "/dev/ttyACM0", "status"]);
        assert_eq!(cli.baud, 115200);
        assert!(matches!(cli.cmd, Cmd::Status));
        let cli = Cli::parse_from([
            "espcap",
            "--port",
            "/dev/ttyACM0",
            "set",
            "--radio",
            "both",
            "--mode",
            "discovery",
            "--format",
            "json",
            "--wifi-band",
            "2.4",
            "--wifi-types",
            "mgmt,data",
            "--manufacturer-regex",
            "Apple",
        ]);
        assert!(matches!(cli.cmd, Cmd::Set { .. }));
        let cli = Cli::parse_from(["espcap", "--port", "x", "start", "--json", "-"]);
        assert!(matches!(cli.cmd, Cmd::Start { json: Some(_), .. }));
        let cli = Cli::parse_from(["espcap", "--port", "x", "start", "--pcap", "cap"]);
        assert!(matches!(cli.cmd, Cmd::Start { pcap: Some(_), .. }));
        let cli = Cli::parse_from(["espcap", "--port", "x", "stop"]);
        assert!(matches!(cli.cmd, Cmd::Stop));
        let cmd = set_command(
            Some(RadioArg::Wifi),
            Some(ModeArg::Capture),
            Some(FormatArg::Pcap),
            Some(WifiBandArg::FiveG),
            Some(300),
            Some(1),
            Some(vec![36]),
            Some(100),
            Some(30),
            Some(false),
            Some(vec![WifiTypeArg::Ctrl]),
            None,
        );
        assert!(matches!(
            cmd,
            Command::Set {
                radio: Some(Radio::Wifi),
                ..
            }
        ));
        let _ = Radio::from(RadioArg::Ble);
        let _ = Radio::from(RadioArg::Off);
        let _ = Mode::from(ModeArg::Discovery);
        let _ = OutputFormat::from(FormatArg::Json);
        let _ = WifiBand::from(WifiBandArg::Both);
        let _ = WifiBand::from(WifiBandArg::TwoG);
        let _ = WifiType::from(WifiTypeArg::Mgmt);
        let _ = WifiType::from(WifiTypeArg::Data);
        let cmd = set_command(
            Some(RadioArg::Both),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(FilterSpec::default()),
        );
        assert!(matches!(
            cmd,
            Command::Set {
                filters: Some(_),
                ..
            }
        ));
    }
}
