/// Error type returned by library functions.
#[derive(Debug)]
pub enum Error {
  UnexpectedData(String),
  ServiceError(String),
  SocketSpecInvalid,
  SocketSpecMissingHost,
  SocketSpecUnsupportedType,
  IoError(std::io::Error),
}

impl From<std::io::Error> for Error {
  fn from(err: std::io::Error) -> Error {
    Error::IoError(err)
  }
}

/// `Result` typedef using the library's Error type.
pub type Result<T> = std::result::Result<T, Error>;
