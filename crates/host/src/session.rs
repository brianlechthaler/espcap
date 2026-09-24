use crate::Error;
use espcap_protocol::{encode_line, parse_event_line, Command, Event};
use std::io::{self, BufRead, BufReader, Write};
use std::time::{Duration, Instant};

fn is_timeout(e: &Error) -> bool {
    matches!(e, Error::Io(err) if err.kind() == io::ErrorKind::TimedOut)
}

fn skip_line(e: &Error) -> bool {
    is_timeout(e) || matches!(e, Error::Msg(m) if m.starts_with("parse ") || m == "line too long")
}

pub fn write_command(w: &mut impl Write, cmd: &Command) -> Result<(), Error> {
    let line = encode_line(cmd)?;
    log::debug!("tx {line}");
    writeln!(w, "{line}")?;
    w.flush()?;
    Ok(())
}

pub const MAX_LINE: usize = 4096;

pub fn sanitize_text(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).take(200).collect()
}

pub fn read_limited_line(r: &mut impl BufRead) -> Result<Option<String>, Error> {
    let mut out = Vec::new();
    loop {
        let available = r.fill_buf()?;
        if available.is_empty() {
            if out.is_empty() {
                return Ok(None);
            }
            break;
        }
        if let Some(i) = available.iter().position(|&b| b == b'\n') {
            let take = i + 1;
            if out.len() + take > MAX_LINE {
                r.consume(take);
                return Err(Error::msg("line too long"));
            }
            out.extend_from_slice(&available[..take]);
            r.consume(take);
            break;
        }
        if out.len() + available.len() > MAX_LINE {
            let n = available.len();
            r.consume(n);
            discard_until_newline(r)?;
            return Err(Error::msg("line too long"));
        }
        let n = available.len();
        out.extend_from_slice(available);
        r.consume(n);
    }
    Ok(Some(String::from_utf8_lossy(&out).into_owned()))
}

fn discard_until_newline(r: &mut impl BufRead) -> Result<(), Error> {
    loop {
        let available = r.fill_buf()?;
        if available.is_empty() {
            return Ok(());
        }
        if let Some(i) = available.iter().position(|&b| b == b'\n') {
            r.consume(i + 1);
            return Ok(());
        }
        let n = available.len();
        r.consume(n);
    }
}

pub fn read_event(r: &mut impl BufRead) -> Result<Event, Error> {
    loop {
        let Some(line) = read_limited_line(r)? else {
            return Err(Error::msg("eof"));
        };
        let trimmed = line.trim();
        if trimmed.is_empty() || !trimmed.starts_with('{') {
            continue;
        }
        match parse_event_line(trimmed) {
            Ok(ev) => return Ok(ev),
            Err(e) => {
                let snippet: String = sanitize_text(trimmed).chars().take(24).collect();
                return Err(Error::msg(format!("parse {e} | {snippet}")));
            }
        }
    }
}

pub fn read_status(r: &mut impl BufRead) -> Result<Event, Error> {
    read_status_for(r, Duration::from_secs(5))
}

pub fn read_status_for(r: &mut impl BufRead, timeout: Duration) -> Result<Event, Error> {
    let deadline = Instant::now() + timeout;
    loop {
        if Instant::now() > deadline {
            return Err(Error::msg("expected status"));
        }
        match read_event(r) {
            Ok(Event::Status(v)) => return Ok(Event::Status(v)),
            Ok(Event::Error { msg }) => return Err(Error::msg(sanitize_text(&msg))),
            Ok(_) => {}
            Err(e) if skip_line(&e) => {}
            Err(e) => return Err(e),
        }
    }
}

pub fn wait_ack(r: &mut impl BufRead) -> Result<(), Error> {
    wait_ack_for(r, Duration::from_secs(5))
}

pub fn wait_ack_for(r: &mut impl BufRead, timeout: Duration) -> Result<(), Error> {
    let deadline = Instant::now() + timeout;
    loop {
        if Instant::now() > deadline {
            return Err(Error::msg("expected ack"));
        }
        match read_event(r) {
            Ok(Event::Ack) => return Ok(()),
            Ok(Event::Error { msg }) => return Err(Error::msg(sanitize_text(&msg))),
            Ok(_) => {}
            Err(e) if skip_line(&e) => {}
            Err(e) => return Err(e),
        }
    }
}

pub fn request_status(rw: &mut (impl BufRead + Write)) -> Result<Event, Error> {
    write_command(rw, &Command::Status)?;
    read_status(rw)
}

pub fn wait_ready<P: std::io::Read + Write>(
    reader: &mut BufReader<P>,
    timeout: Duration,
) -> Result<Event, Error> {
    let deadline = Instant::now() + timeout;
    let mut last = Error::msg("device not ready");
    while Instant::now() < deadline {
        if let Err(e) = write_command(reader.get_mut(), &Command::Status) {
            last = e;
            continue;
        }
        let slice = deadline.saturating_duration_since(Instant::now());
        match read_status_for(reader, slice.min(Duration::from_millis(800))) {
            Ok(ev) => return Ok(ev),
            Err(e) => last = e,
        }
    }
    Err(last)
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
        let mut rx = Cursor::new(b"\n{\"event\":\"ack\"}\n".to_vec());
        wait_ack(&mut rx).unwrap();
        let mut rx = Cursor::new(
            b"I (349) esp_image: segment 4: size{\"event\":\"ack\"}\n{\"event\":\"ack\"}\n"
                .to_vec(),
        );
        wait_ack(&mut rx).unwrap();
        let mut rx = Cursor::new(b"{\"event\":\"error\",\"msg\":\"no\"}\n".to_vec());
        assert!(wait_ack(&mut rx).is_err());
        let wifi = b"{\"event\":\"wifi_ap\",\"mac\":\"00:00:00:00:00:00\",\"oui\":\"00:00:00\",\"rssi\":0,\"channel\":1,\"freq_mhz\":2412,\"ts_ms\":0,\"ssid\":\"\",\"hit_count\":1,\"first_ts_ms\":0,\"last_ts_ms\":0}\n";
        let mut rx = Cursor::new(wifi.to_vec());
        assert!(wait_ack(&mut rx).is_err());
        let mut skip = wifi.to_vec();
        skip.extend_from_slice(b"{\"event\":\"ack\"}\n");
        wait_ack(&mut Cursor::new(skip)).unwrap();
        let mut flood = Vec::new();
        for _ in 0..80 {
            flood.extend_from_slice(wifi);
        }
        flood.extend_from_slice(b"{\"event\":\"ack\"}\n");
        wait_ack(&mut Cursor::new(flood)).unwrap();
        let mut st = wifi.to_vec();
        st.extend_from_slice(b"{\"event\":\"status\",\"chip\":\"esp32s3\"}\n");
        assert!(matches!(
            read_status(&mut Cursor::new(st)).unwrap(),
            Event::Status(_)
        ));
        let mut st_flood = Vec::new();
        for _ in 0..80 {
            st_flood.extend_from_slice(wifi);
        }
        st_flood.extend_from_slice(b"{\"event\":\"status\",\"chip\":\"esp32s3\"}\n");
        assert!(matches!(
            read_status(&mut Cursor::new(st_flood)).unwrap(),
            Event::Status(_)
        ));
        struct Repeat {
            chunk: Vec<u8>,
            pos: usize,
        }
        impl Read for Repeat {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                if self.pos >= self.chunk.len() {
                    self.pos = 0;
                }
                let n = (self.chunk.len() - self.pos).min(buf.len());
                buf[..n].copy_from_slice(&self.chunk[self.pos..self.pos + n]);
                self.pos += n;
                Ok(n)
            }
        }
        let mut repeating = BufReader::new(Repeat {
            chunk: wifi.to_vec(),
            pos: 0,
        });
        assert!(wait_ack_for(&mut repeating, Duration::from_millis(20)).is_err());
        let mut rx = Cursor::new(Vec::<u8>::new());
        assert!(read_event(&mut rx).is_err());
        let mut rx = Cursor::new(b"{not-json\n".to_vec());
        assert!(read_event(&mut rx)
            .unwrap_err()
            .to_string()
            .contains("parse "));
        wait_ack(&mut Cursor::new(
            b"{not-json\n{\"event\":\"ack\"}\n".to_vec(),
        ))
        .unwrap();
        let mut glued = b"}{\"event\":\"wifi_ap\"\n".to_vec();
        glued.extend_from_slice(b"{\"event\":\"ack\"}\n");
        wait_ack(&mut Cursor::new(glued)).unwrap();
        let mut st_bad = b"{truncated\n".to_vec();
        st_bad.extend_from_slice(b"{\"event\":\"status\",\"chip\":\"esp32s3\"}\n");
        assert!(matches!(
            read_status(&mut Cursor::new(st_bad)).unwrap(),
            Event::Status(_)
        ));
        assert!(read_status(&mut Cursor::new(wifi.to_vec())).is_err());
        assert!(read_status(&mut Cursor::new(
            b"{\"event\":\"error\",\"msg\":\"no\"}\n".to_vec()
        ))
        .is_err());

        struct Duplex {
            out: Vec<u8>,
            inp: Cursor<Vec<u8>>,
            fail_write: bool,
        }
        impl Write for Duplex {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                if self.fail_write {
                    return Err(std::io::Error::other("fail"));
                }
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
            fail_write: false,
        };
        let ev = request_status(&mut rw).unwrap();
        assert!(matches!(ev, Event::Status(_)));
        assert!(String::from_utf8(rw.out).unwrap().contains("status"));
        let mut leftover = Duplex {
            out: Vec::new(),
            inp: Cursor::new(b"xyz".to_vec()),
            fail_write: false,
        };
        let mut tmp = [0u8; 2];
        assert_eq!(leftover.read(&mut tmp).unwrap(), 2);
        let mut ready = BufReader::new(Duplex {
            out: Vec::new(),
            inp: Cursor::new(b"{\"event\":\"status\",\"chip\":\"esp32s3\"}\n".to_vec()),
            fail_write: false,
        });
        assert!(matches!(
            wait_ready(&mut ready, Duration::from_secs(1)).unwrap(),
            Event::Status(_)
        ));
        let mut dead = BufReader::new(Duplex {
            out: Vec::new(),
            inp: Cursor::new(Vec::new()),
            fail_write: false,
        });
        assert!(wait_ready(&mut dead, Duration::from_millis(30)).is_err());
        let mut fail = BufReader::new(Duplex {
            out: Vec::new(),
            inp: Cursor::new(Vec::new()),
            fail_write: true,
        });
        assert!(wait_ready(&mut fail, Duration::from_millis(30)).is_err());
    }

    #[test]
    fn wait_ack_retries_serial_timeout() {
        struct TimeoutThen {
            left: u32,
            data: Cursor<Vec<u8>>,
        }
        impl Read for TimeoutThen {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                if self.left > 0 {
                    self.left -= 1;
                    return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "t"));
                }
                self.data.read(buf)
            }
        }
        impl BufRead for TimeoutThen {
            fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
                if self.left > 0 {
                    self.left -= 1;
                    return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "t"));
                }
                self.data.fill_buf()
            }
            fn consume(&mut self, amt: usize) {
                self.data.consume(amt);
            }
        }
        let mut rx = TimeoutThen {
            left: 2,
            data: Cursor::new(b"{\"event\":\"ack\"}\n".to_vec()),
        };
        wait_ack_for(&mut rx, Duration::from_millis(200)).unwrap();
        let mut st = TimeoutThen {
            left: 2,
            data: Cursor::new(b"{\"event\":\"status\",\"chip\":\"esp32s3\"}\n".to_vec()),
        };
        assert!(matches!(
            read_status_for(&mut st, Duration::from_millis(200)).unwrap(),
            Event::Status(_)
        ));
        let mut r = TimeoutThen {
            left: 1,
            data: Cursor::new(b"x".to_vec()),
        };
        let mut b = [0u8; 1];
        assert!(r.read(&mut b).is_err());
        assert_eq!(r.read(&mut b).unwrap(), 1);
        let mut huge = vec![b'{'];
        huge.extend(std::iter::repeat_n(b'a', 5000));
        huge.extend_from_slice(b"\n{\"event\":\"ack\"}\n");
        wait_ack(&mut Cursor::new(huge)).unwrap();
        let mut esc = Cursor::new(b"{\"event\":\"error\",\"msg\":\"no\\u001b[2J\"}\n".to_vec());
        let err = wait_ack(&mut esc).unwrap_err().to_string();
        assert!(!err.contains('\u{1b}'));
        assert!(err.contains("no"));
        assert_eq!(sanitize_text("a\u{1b}\nb"), "ab");
        assert!(read_limited_line(&mut Cursor::new(Vec::<u8>::new()))
            .unwrap()
            .is_none());
        struct Pieces {
            parts: Vec<Vec<u8>>,
            i: usize,
        }
        impl BufRead for Pieces {
            fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
                if self.i >= self.parts.len() {
                    Ok(&[])
                } else {
                    Ok(&self.parts[self.i])
                }
            }
            fn consume(&mut self, n: usize) {
                assert_eq!(n, self.parts[self.i].len());
                self.i += 1;
            }
        }
        impl Read for Pieces {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                let src = self.fill_buf()?;
                if src.is_empty() {
                    return Ok(0);
                }
                let n = src.len().min(buf.len());
                buf[..n].copy_from_slice(&src[..n]);
                self.consume(n);
                Ok(n)
            }
        }
        let mut once = Pieces {
            parts: vec![b"hi".to_vec()],
            i: 0,
        };
        let mut tmp = [0u8; 8];
        assert_eq!(once.read(&mut tmp).unwrap(), 2);
        assert_eq!(once.read(&mut tmp).unwrap(), 0);
        let mut split = Pieces {
            parts: vec![
                vec![b'x'; 3000],
                vec![b'y'; 2000],
                vec![b'z'; 50],
                b"\n".to_vec(),
            ],
            i: 0,
        };
        assert!(read_limited_line(&mut split).is_err());
        let mut eof_mid = Pieces {
            parts: vec![vec![b'z'; 5000]],
            i: 0,
        };
        assert!(read_limited_line(&mut eof_mid).is_err());
        let mut partial = Pieces {
            parts: vec![b"{\"event\":\"ack\"}".to_vec()],
            i: 0,
        };
        assert!(read_limited_line(&mut partial)
            .unwrap()
            .unwrap()
            .contains("ack"));
    }
}
