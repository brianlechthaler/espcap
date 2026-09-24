pub mod cli;
pub mod io_util;
pub mod oui;
pub mod session;

pub use cli::{Cli, Cmd};
pub use io_util::{
    capture_json_from, capture_pcap_from, chip_from_status, reject_illegal_band, write_jsonl,
    PcapSinks,
};
pub use oui::{builtin_oui_db, merge_filters, ouis_for_manufacturer, OuiEntry};
pub use session::{
    read_event, read_status, request_status, wait_ack, wait_ack_for, wait_ready, write_command,
};

use espcap_protocol::Error as ProtoError;
use std::io;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Proto(#[from] ProtoError),
    #[error("{0}")]
    Msg(String),
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("serial: {0}")]
    Serial(String),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid regex: {0}")]
    Regex(#[from] regex::Error),
}

impl Error {
    pub fn msg(m: impl Into<String>) -> Self {
        Self::Msg(m.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_msg() {
        assert!(matches!(Error::msg("x"), Error::Msg(_)));
        let _ = builtin_oui_db().unwrap();
        let _ = Error::Serial("p".into());
    }
}
