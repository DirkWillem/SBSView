use std::fmt::{Display, Formatter};

#[derive(Clone, Debug)]
pub enum Error {
    SerialError(String),
    SerialTimeout,
    Timeout,
    DecodeError(String),
    WrongFrame(String),
    InvalidCommand(String),
    Internal(String),
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::SerialError(e) => write!(f, "Serial error: {e}"),
            Error::SerialTimeout => write!(f, "Serial timeout"),
            Error::Timeout => write!(f, "Timeout"),
            Error::DecodeError(e) => write!(f, "Decode error: {e}"),
            Error::WrongFrame(e) => write!(f, "Wrong frame: {e}"),
            Error::InvalidCommand(e) => write!(f, "Invalid command: {e}"),
            Error::Internal(e) => write!(f, "Internal error: {e}")
        }
    }
}

impl From<Error> for String {
    fn from(value: Error) -> Self {
        value.to_string()
    }
}
