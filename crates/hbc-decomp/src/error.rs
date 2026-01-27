use std::fmt;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Parse(String),
    UnsupportedVersion(u32),
    MissingFormat(u32),
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::Io(err)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(err) => write!(f, "io error: {err}"),
            Error::Parse(msg) => write!(f, "parse error: {msg}"),
            Error::UnsupportedVersion(version) => {
                write!(f, "unsupported bytecode version: {version}")
            }
            Error::MissingFormat(version) => {
                write!(f, "missing opcode format for bytecode version: {version}")
            }
        }
    }
}

impl std::error::Error for Error {}
