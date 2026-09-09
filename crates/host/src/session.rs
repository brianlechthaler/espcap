use crate::Error;
use espcap_protocol::{encode_line, parse_event_line, Command, Event};
use std::io::{BufRead, Write};

pub fn write_command(w: &mut impl Write, cmd: &Command) -> Result<(), Error> {
    let line = encode_line(cmd)?;
    log::debug!("tx {line}");
    writeln!(w, "{line}")?;
    w.flush()?;
    Ok(())
}

pub fn read_event(r: &mut impl BufRead) -> Result<Event, Error> {
    loop {
        let mut line = String::new();
        let n = r.read_line(&mut line)?;
        if n == 0 {
            return Err(Error::msg("eof"));
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some(json) = trimmed.find('{').map(|i| &trimmed[i..]) else {
            continue;
        };
        return Ok(parse_event_line(json)?);
    }
}

pub fn read_status(r: &mut impl BufRead) -> Result<Event, Error> {
    for _ in 0..64 {
        match read_event(r)? {
            Event::Status(v) => return Ok(Event::Status(v)),
            Event::Error { msg } => return Err(Error::msg(msg)),
            _ => {}
        }
    }
    Err(Error::msg("expected status"))
}

pub fn wait_ack(r: &mut impl BufRead) -> Result<(), Error> {
    for _ in 0..64 {
        match read_event(r)? {
            Event::Ack => return Ok(()),
            Event::Error { msg } => return Err(Error::msg(msg)),
            _ => {}
        }
    }
    Err(Error::msg("expected ack"))
}

pub fn request_status(rw: &mut (impl BufRead + Write)) -> Result<Event, Error> {
    write_command(rw, &Command::Status)?;
    read_status(rw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Read, Write};

    #[test]
    fn command_ack_and_status() {
        let mut buf = Cursor::new(Vec::<u8>::new());
        write_command(&mut buf, &Command::Get).unwrap();
        let sent = String::from_utf8(buf.into_inner()).unwrap();
        assert!(sent.contains("\"cmd\":\"get\""));

        let mut rx = Cursor::new(b"I (1) boot: hi\n{\"event\":\"ack\"}\n".to_vec());
        wait_ack(&mut rx).unwrap();
        let mut rx =
            Cursor::new(b"I (349) esp_image: segment 4: size{\"event\":\"ack\"}\n".to_vec());
        wait_ack(&mut rx).unwrap();
        let mut rx = Cursor::new(b"{\"event\":\"error\",\"msg\":\"no\"}\n".to_vec());
        assert!(wait_ack(&mut rx).is_err());
        let wifi = b"{\"event\":\"wifi_ap\",\"mac\":\"00:00:00:00:00:00\",\"oui\":\"00:00:00\",\"rssi\":0,\"channel\":1,\"freq_mhz\":2412,\"ts_ms\":0,\"ssid\":\"\",\"hit_count\":1,\"first_ts_ms\":0,\"last_ts_ms\":0}\n";
        let mut rx = Cursor::new(wifi.to_vec());
        assert!(wait_ack(&mut rx).is_err());
        let mut skip = wifi.to_vec();
        skip.extend_from_slice(b"{\"event\":\"ack\"}\n");
        wait_ack(&mut Cursor::new(skip)).unwrap();
        let mut st = wifi.to_vec();
        st.extend_from_slice(b"{\"event\":\"status\",\"chip\":\"esp32s3\"}\n");
        assert!(matches!(
            read_status(&mut Cursor::new(st)).unwrap(),
            Event::Status(_)
        ));
        let mut rx = Cursor::new(Vec::<u8>::new());
        assert!(read_event(&mut rx).is_err());
        assert!(read_status(&mut Cursor::new(wifi.to_vec())).is_err());

        struct Duplex {
            out: Vec<u8>,
            inp: Cursor<Vec<u8>>,
        }
        impl Write for Duplex {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.out.extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        impl std::io::Read for Duplex {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                self.inp.read(buf)
            }
        }
        impl std::io::BufRead for Duplex {
            fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
                self.inp.fill_buf()
            }
            fn consume(&mut self, amt: usize) {
                self.inp.consume(amt);
            }
        }
        let mut rw = Duplex {
            out: Vec::new(),
            inp: Cursor::new(b"{\"event\":\"status\",\"chip\":\"esp32s3\"}\n".to_vec()),
        };
        let ev = request_status(&mut rw).unwrap();
        assert!(matches!(ev, Event::Status(_)));
        assert!(String::from_utf8(rw.out).unwrap().contains("status"));
        let mut leftover = Duplex {
            out: Vec::new(),
            inp: Cursor::new(b"xyz".to_vec()),
        };
        let mut tmp = [0u8; 2];
        assert_eq!(leftover.read(&mut tmp).unwrap(), 2);
    }
}
