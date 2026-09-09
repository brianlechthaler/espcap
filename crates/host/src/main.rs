use clap::Parser;
use espcap::cli::set_command;
use espcap::io_util::{capture_json_from, capture_pcap_from};
use espcap::oui::merge_filters;
use espcap::session::write_command;
use espcap::{
    builtin_oui_db, chip_from_status, read_status, reject_illegal_band, wait_ack, wait_ready, Cli,
    Cmd, Error,
};
use espcap_protocol::Command;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::time::Duration;

fn open_serial(port: &str, baud: u32) -> Result<Box<dyn serialport::SerialPort>, Error> {
    let builder = serialport::new(port, baud)
        .timeout(Duration::from_millis(400))
        .dtr_on_open(false);
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        let p = builder
            .open_native()
            .map_err(|e| Error::Serial(e.to_string()))?;
        unsafe {
            let fd = p.as_raw_fd();
            let mut ios: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(fd, &mut ios) == 0 {
                ios.c_cflag &= !libc::HUPCL;
                let _ = libc::tcsetattr(fd, libc::TCSANOW, &ios);
            }
        }
        Ok(Box::new(p))
    }
    #[cfg(not(unix))]
    {
        use serialport::SerialPort;
        use std::io::Write;
        let mut p = builder.open().map_err(|e| Error::Serial(e.to_string()))?;
        let _ = p.write_data_terminal_ready(false);
        let _ = p.write_all(b"\n");
        let _ = p.flush();
        Ok(p)
    }
}

fn consume_unread<R: std::io::Read>(reader: &mut BufReader<R>) {
    let n = reader.buffer().len();
    reader.consume(n);
}

fn run(cli: Cli) -> Result<(), Error> {
    let port = open_serial(&cli.port, cli.baud)?;
    let mut reader = BufReader::new(port);
    wait_ready(&mut reader, Duration::from_secs(15))?;
    reader.get_mut().set_timeout(Duration::from_secs(5)).ok();
    match cli.cmd {
        Cmd::Status => {
            write_command(reader.get_mut(), &Command::Status)?;
            let ev = read_status(&mut reader)?;
            match ev {
                espcap_protocol::Event::Status(v) => println!("{}", serde_json::to_string(&v)?),
                _ => return Err(Error::msg("expected status")),
            }
        }
        Cmd::Stop => {
            write_command(reader.get_mut(), &Command::Stop)?;
            wait_ack(&mut reader)?;
        }
        Cmd::Set {
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
            oui,
            mac,
            name_regex,
            ssid_regex,
            manufacturer_regex,
            company_id,
            start,
        } => {
            write_command(reader.get_mut(), &Command::Stop)?;
            let _ = wait_ack(&mut reader);
            consume_unread(&mut reader);
            write_command(reader.get_mut(), &Command::Status)?;
            let st = read_status(&mut reader)?;
            consume_unread(&mut reader);
            let chip = chip_from_status(&st)?;
            let band = wifi_band.map(Into::into);
            let ch5 = channels_5ghz.clone().unwrap_or_default();
            reject_illegal_band(chip, band, &ch5)?;
            let db = builtin_oui_db()?;
            let filters = merge_filters(
                oui,
                mac,
                name_regex,
                ssid_regex,
                company_id,
                manufacturer_regex.as_deref(),
                &db,
            )?;
            let mask = match hopmask.as_deref() {
                Some(s) => Some(espcap_protocol::types::parse_hopmask(s)?),
                None => None,
            };
            let cmd = set_command(
                radio,
                mode,
                format,
                wifi_band,
                dwell_ms,
                mask,
                channels_5ghz,
                ble_interval_ms,
                ble_window_ms,
                ble_active,
                wifi_types,
                filters,
            );
            write_command(reader.get_mut(), &cmd)?;
            wait_ack(&mut reader)?;
            if start {
                write_command(reader.get_mut(), &Command::Start)?;
                wait_ack(&mut reader)?;
            }
        }
        Cmd::Start { json, pcap, detach } => {
            write_command(reader.get_mut(), &Command::Start)?;
            wait_ack(&mut reader)?;
            if detach {
                return Ok(());
            }
            if let Some(prefix) = pcap {
                capture_pcap_from(&mut reader, Path::new(&prefix))?;
            } else {
                capture_json_from(&mut reader, json.as_deref().unwrap_or("-"))?;
            }
        }
    }
    Ok(())
}

fn main() {
    env_logger::init();
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
