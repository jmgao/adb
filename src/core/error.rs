/// Error type returned by library functions.
#[derive(Debug)]
pub enum Error {
  /// Received unexpected data of some sort.
  UnexpectedData(String),

  /// Failed to connect to a service with a reason.
  ServiceError(String),

  /// Attempted an operation that should be supported, but isn't implemented yet.
  UnimplementedOperation(String),

  /// SocketSpec failed to parse.
  SocketSpecInvalid,

  /// Attempted to connect to a tcp or vsock SocketSpec that didn't have a host.
  SocketSpecMissingHost,

  /// Attempted to use a SocketSpec that is unavailable on the current platform.
  SocketSpecUnsupportedType,

  /// An I/O error occurred.
  IoError(std::io::Error),
}

impl From<std::io::Error> for Error {
  fn from(err: std::io::Error) -> Error {
    Error::IoError(err)
  }
}

/// `Result` typedef using the library's Error type.
pub type Result<T> = std::result::Result<T, Error>;
