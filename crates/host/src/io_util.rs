use crate::Error;
use espcap_protocol::pcap::{
    decode_frames, pcap_global_header, DLT_BLUETOOTH_LE_LL_WITH_PHDR, DLT_IEEE802_11_RADIO,
    TYPE_BLE, TYPE_GLOBAL, TYPE_WIFI,
};
use espcap_protocol::{parse_event_line, Chip, Event, WifiBand};
use std::fs::File;
use std::io::{self, BufRead, Read, Write};
use std::path::{Path, PathBuf};

pub fn chip_from_status(ev: &Event) -> Result<Chip, Error> {
    let Event::Status(v) = ev else {
        return Err(Error::msg("expected status event"));
    };
    match v.get("chip").and_then(|c| c.as_str()) {
        Some("esp32s3") => Ok(Chip::Esp32s3),
        Some("esp32c5") => Ok(Chip::Esp32c5),
        Some(other) => Err(Error::msg(format!("unknown chip {other}"))),
        None => Err(Error::msg("status missing chip")),
    }
}

pub fn reject_illegal_band(
    chip: Chip,
    band: Option<WifiBand>,
    channels_5ghz: &[u8],
) -> Result<(), Error> {
    if chip.supports_5ghz() {
        return Ok(());
    }
    if band.is_some_and(|b| b.uses_5g()) {
        return Err(Error::msg(
            "ESP32-S3 does not support 5 GHz; omit --wifi-band 5/both",
        ));
    }
    if !channels_5ghz.is_empty() {
        return Err(Error::msg("ESP32-S3 rejects --channels-5ghz"));
    }
    Ok(())
}

fn write_lines(out: &mut impl Write, lines: impl Iterator<Item = String>) -> Result<(), Error> {
    for line in lines {
        writeln!(out, "{line}")?;
    }
    Ok(())
}

fn write_bytes(out: &mut impl Write, buf: &[u8]) -> Result<(), Error> {
    out.write_all(buf)?;
    Ok(())
}

pub fn write_jsonl(path: &str, lines: impl Iterator<Item = String>) -> Result<(), Error> {
    if path == "-" {
        let mut out = io::stdout().lock();
        return write_lines(&mut out, lines);
    }
    let mut f = File::create(path)?;
    write_lines(&mut f, lines)
}

pub struct PcapSinks {
    wifi: File,
    ble: File,
    wifi_path: PathBuf,
    ble_path: PathBuf,
}

impl PcapSinks {
    pub fn create(prefix: &Path) -> Result<Self, Error> {
        let wifi_path = prefix_with_suffix(prefix, "-wifi.pcap");
        let ble_path = prefix_with_suffix(prefix, "-ble.pcap");
        let mut wifi = File::create(&wifi_path)?;
        let mut ble = File::create(&ble_path)?;
        write_bytes(&mut wifi, &pcap_global_header(DLT_IEEE802_11_RADIO))?;
        write_bytes(&mut ble, &pcap_global_header(DLT_BLUETOOTH_LE_LL_WITH_PHDR))?;
        Ok(Self {
            wifi,
            ble,
            wifi_path,
            ble_path,
        })
    }

    pub fn absorb(&mut self, buf: &[u8]) -> Result<usize, Error> {
        let (frames, n) = decode_frames(buf);
        for fr in frames {
            match fr.typ {
                TYPE_GLOBAL => {}
                TYPE_WIFI => write_bytes(&mut self.wifi, &fr.payload)?,
                TYPE_BLE => write_bytes(&mut self.ble, &fr.payload)?,
                _ => {}
            }
        }
        Ok(n)
    }

    pub fn paths(&self) -> (&Path, &Path) {
        (&self.wifi_path, &self.ble_path)
    }
}

pub fn capture_json_from(r: &mut impl BufRead, path: &str) -> Result<(), Error> {
    let mut out: Box<dyn Write> = if path == "-" {
        Box::new(io::stdout().lock())
    } else {
        Box::new(File::create(path)?)
    };
    let mut buf = String::new();
    loop {
        buf.clear();
        match r.read_line(&mut buf) {
            Ok(0) => break,
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::TimedOut => continue,
            Err(e) => return Err(e.into()),
        }
        let trimmed = buf.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some(json) = trimmed.find('{').map(|i| &trimmed[i..]) else {
            continue;
        };
        if parse_event_line(json).is_ok() {
            writeln!(out, "{json}")?;
            out.flush()?;
        }
    }
    Ok(())
}

pub fn capture_pcap_from(r: &mut impl Read, prefix: &Path) -> Result<(), Error> {
    let mut sinks = PcapSinks::create(prefix)?;
    let mut acc = Vec::new();
    let mut tmp = [0u8; 512];
    loop {
        let n = match r.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::TimedOut => break,
            Err(e) => return Err(e.into()),
        };
        acc.extend_from_slice(&tmp[..n]);
        let consumed = sinks.absorb(&acc)?;
        acc.drain(..consumed);
    }
    Ok(())
}

fn prefix_with_suffix(prefix: &Path, suffix: &str) -> PathBuf {
    let mut s = prefix.as_os_str().to_os_string();
    s.push(suffix);
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use espcap_protocol::pcap::{ble_pcap_payload, encode_frame, wifi_pcap_payload};
    use espcap_protocol::wifi::beacon_fixture;
    use espcap_protocol::{parse_event_line, WifiBand};
    use std::fs;
    use std::io::{self, BufReader, Cursor, Read, Write};
    use std::path::Path;

    #[test]
    fn s3_rejects_5g_c5_allows() {
        let st = parse_event_line(r#"{"event":"status","chip":"esp32s3"}"#).unwrap();
        assert_eq!(chip_from_status(&st).unwrap(), Chip::Esp32s3);
        assert!(reject_illegal_band(Chip::Esp32s3, Some(WifiBand::FiveG), &[]).is_err());
        assert!(reject_illegal_band(Chip::Esp32s3, Some(WifiBand::Both), &[]).is_err());
        assert!(reject_illegal_band(Chip::Esp32s3, Some(WifiBand::TwoG), &[36]).is_err());
        assert!(reject_illegal_band(Chip::Esp32s3, Some(WifiBand::TwoG), &[]).is_ok());
        let st = parse_event_line(r#"{"event":"status","chip":"esp32c5"}"#).unwrap();
        assert_eq!(chip_from_status(&st).unwrap(), Chip::Esp32c5);
        assert!(reject_illegal_band(Chip::Esp32c5, Some(WifiBand::Both), &[36]).is_ok());
        assert!(chip_from_status(&espcap_protocol::ack()).is_err());
        let st = parse_event_line(r#"{"event":"status","chip":"esp32"}"#).unwrap();
        assert!(chip_from_status(&st).is_err());
        let st = parse_event_line(r#"{"event":"status"}"#).unwrap();
        assert!(chip_from_status(&st).is_err());
    }

    #[test]
    fn jsonl_and_pcap_files() {
        let dir = std::env::temp_dir().join(format!("espcap-test-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let json_path = dir.join("c.jsonl");
        write_jsonl(
            json_path.to_str().unwrap(),
            ["{\"event\":\"ack\"}".into()].into_iter(),
        )
        .unwrap();
        assert!(fs::read_to_string(&json_path).unwrap().contains("ack"));
        let prefix = dir.join("capture");
        let mut sinks = PcapSinks::create(&prefix).unwrap();
        let mpdu = beacon_fixture([1; 6], "n");
        let mut framed = encode_frame(TYPE_GLOBAL, &pcap_global_header(DLT_IEEE802_11_RADIO));
        framed.extend_from_slice(&encode_frame(
            TYPE_WIFI,
            &wifi_pcap_payload(0, 2412, false, -30, 2, &mpdu),
        ));
        framed.extend_from_slice(&encode_frame(
            TYPE_BLE,
            &ble_pcap_payload(0, -50, 39, &[2; 6], true, &[0x01]),
        ));
        framed.extend_from_slice(&encode_frame(99, &[0]));
        let n = sinks.absorb(&framed).unwrap();
        assert_eq!(n, framed.len());
        let (w, b) = sinks.paths();
        assert!(w.ends_with("capture-wifi.pcap"));
        assert!(b.ends_with("capture-ble.pcap"));
        let wifi_p = w.to_path_buf();
        let ble_p = b.to_path_buf();
        drop(sinks);
        assert!(fs::metadata(&wifi_p).unwrap().len() > 24);
        assert!(fs::metadata(&ble_p).unwrap().len() > 24);

        let mut json_in = Cursor::new(b"\nI (1) boot size{\"event\":\"ack\"}\n{\"incomplete\"");
        capture_json_from(&mut json_in, json_path.to_str().unwrap()).unwrap();
        let mut json_in = Cursor::new(b"\nnojson\n{\"event\":\"ack\"}\n");
        capture_json_from(&mut json_in, json_path.to_str().unwrap()).unwrap();
        let mut json_in = Cursor::new(b"\n{\"event\":\"ack\"}\n");
        capture_json_from(&mut json_in, json_path.to_str().unwrap()).unwrap();
        let streamed = dir.join("stream.jsonl");
        struct MidWrite {
            stage: u8,
            path: std::path::PathBuf,
        }
        impl Read for MidWrite {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                match self.stage {
                    0 => {
                        self.stage = 1;
                        let line = b"{\"event\":\"ack\"}\n";
                        buf[..line.len()].copy_from_slice(line);
                        Ok(line.len())
                    }
                    1 => {
                        let got = fs::read_to_string(&self.path).unwrap_or_default();
                        assert!(got.contains("ack"), "json capture must flush before EOF");
                        self.stage = 2;
                        Err(io::Error::new(io::ErrorKind::TimedOut, "t"))
                    }
                    _ => Ok(0),
                }
            }
        }
        let mut mid = BufReader::new(MidWrite {
            stage: 0,
            path: streamed.clone(),
        });
        capture_json_from(&mut mid, streamed.to_str().unwrap()).unwrap();
        assert!(fs::read_to_string(&streamed).unwrap().contains("ack"));
        capture_json_from(&mut Cursor::new(b"{\"event\":\"ack\"}\n"), "-").unwrap();
        assert!(capture_json_from(
            &mut Cursor::new(b"{\"event\":\"ack\"}\n"),
            "/no/such/espcap-dir/x.jsonl",
        )
        .is_err());
        struct BoomR;
        impl Read for BoomR {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::other("boom"))
            }
        }
        assert!(capture_json_from(&mut BufReader::new(BoomR), streamed.to_str().unwrap()).is_err());
        write_jsonl("-", ["{\"event\":\"ack\"}".into()].into_iter()).unwrap();

        let mut pcap_in = Cursor::new(framed);
        capture_pcap_from(&mut pcap_in, &dir.join("from")).unwrap();
        let timed: io::Result<&[u8]> = Err(io::Error::new(io::ErrorKind::TimedOut, "t"));
        struct Timed;
        impl Read for Timed {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::TimedOut, "t"))
            }
        }
        capture_pcap_from(&mut Timed, &dir.join("t")).unwrap();
        struct Boom;
        impl Read for Boom {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::other("boom"))
            }
        }
        assert!(capture_pcap_from(&mut Boom, &dir.join("b")).is_err());
        assert!(write_jsonl("/no/such/espcap-dir/x.jsonl", ["x".into()].into_iter()).is_err());
        assert!(PcapSinks::create(Path::new("/no/such/espcap-dir/x")).is_err());
        struct FailWrite;
        impl Write for FailWrite {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::other("fail"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut fw = FailWrite;
        assert!(fw.flush().is_ok());
        assert!(write_lines(&mut fw, ["a".into()].into_iter()).is_err());
        assert!(write_bytes(&mut fw, b"x").is_err());
        let ble_dir_prefix = dir.join("cap");
        fs::create_dir_all(dir.join("cap-ble.pcap")).unwrap();
        assert!(PcapSinks::create(&ble_dir_prefix).is_err());
        assert!(
            capture_pcap_from(&mut Cursor::new([]), Path::new("/no/such/espcap-dir/from"),)
                .is_err()
        );
        let mut bad = Cursor::new(b"{\"not\":\"an event\"}\n");
        capture_json_from(&mut bad, json_path.to_str().unwrap()).unwrap();
        let _ = timed;
        let _ = fs::remove_dir_all(&dir);
    }
}
