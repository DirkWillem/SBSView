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
