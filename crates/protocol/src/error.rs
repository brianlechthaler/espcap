use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Error {
    #[error("invalid json: {0}")]
    Json(String),
    #[error("unknown command")]
    UnknownCommand,
    #[error("unknown event")]
    UnknownEvent,
    #[error("{0}")]
    Config(String),
    #[error("invalid mac: {0}")]
    Mac(String),
    #[error("invalid hopmask: {0}")]
    Hopmask(String),
    #[error("invalid regex: {0}")]
    Regex(String),
    #[error("short 802.11 frame")]
    ShortFrame,
    #[error("truncated pcap frame")]
    TruncatedFrame,
    #[error("bad pcap magic")]
    BadMagic,
}

impl Error {
    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Self::Json(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_helper_and_json_from() {
        assert_eq!(Error::config("x"), Error::Config("x".into()));
        let err = serde_json::from_str::<u8>("nope").unwrap_err();
        assert!(matches!(Error::from(err), Error::Json(_)));
    }
}
