use futures::io::{AsyncRead, AsyncWrite};
use romio::TcpStream;

use std::convert::TryFrom;
use std::net::ToSocketAddrs;
use std::path::Path;

use crate as adb;
use crate::util::ConsumePrefix;

/// An implementation of adb's socket address specifiers.
///
/// `SocketSpec`s of all types can be constructed on every platform, because
/// they have meaning when talking to a remote device, but actually connecting
/// to or listening on an address can fail on unsupported platforms.
#[derive(Clone, Debug, PartialEq)]
pub enum SocketSpec {
  /// A TCP address and port.
  Tcp { host: Option<String>, port: u16 },

  /// A Unix domain socket in the Linux-only abstract namespace.
  UnixAbstract { path: String },

  /// A Unix domain socket on the filesystem.
  UnixFilesystem { path: String },

  /// A socket in the Linux vsock(7) address family.
  Vsock { host: Option<String>, port: u32 },
}

#[cfg(not(windows))]
async fn connect_unix_stream(path: impl AsRef<Path>) -> adb::Result<Box<Socket>> {
  use romio::uds::UnixStream;
  let stream = UnixStream::connect(path).await?;
  let stream: Box<Socket> = Box::new(stream);
  Ok(stream)
}

#[cfg(windows)]
async fn connect_unix_stream(_path: impl AsRef<Path>) -> adb::Result<Box<Socket>> {
  Err(adb::Error::SocketSpecUnsupportedType)
}

impl SocketSpec {
  /// Constructs a TCP [SocketSpec].
  pub fn tcp(host: Option<String>, port: u16) -> SocketSpec {
    SocketSpec::Tcp { host, port }
  }

  /// Constructs an abstract Unix domain socket [SocketSpec].
  pub fn unix_abstract(path: impl Into<String>) -> SocketSpec {
    SocketSpec::UnixAbstract { path: path.into() }
  }

  /// Constructs a Unix domain socket [SocketSpec].
  pub fn unix_filesystem(path: impl Into<String>) -> SocketSpec {
    SocketSpec::UnixAbstract { path: path.into() }
  }

  /// Constructs a vsock [SocketSpec].
  pub fn vsock(host: Option<String>, port: u32) -> SocketSpec {
    SocketSpec::Vsock { host, port }
  }

  /// Connects a socket to the address described by the [SocketSpec].
  ///
  /// This function can fail for multiple reasons:
  ///   - network failure
  ///   - attempt to connect to a `Tcp` or `Vsock` [SocketSpec] with no host
  ///   - lack of support (e.g. attempting to use Unix domain sockets on Windows)
  pub async fn connect(&self) -> adb::Result<Box<Socket>> {
    match self {
      SocketSpec::Tcp { host, port } => {
        let host = host.as_ref().ok_or(adb::Error::SocketSpecMissingHost)?;
        let addr = (host.as_str(), *port)
          .to_socket_addrs()?
          .next()
          .expect("to_socket_addrs empty");
        let stream = TcpStream::connect(&addr).await?;
        let stream: Box<Socket> = Box::new(stream);
        Ok(stream)
      }

      SocketSpec::UnixAbstract { path } => connect_unix_stream(format!("\0{}", path)).await,
      SocketSpec::UnixFilesystem { path } => connect_unix_stream(path).await,

      SocketSpec::Vsock { .. } => {
        unimplemented!("SocketSpec::connect unimplemented for Vsock");
      }
    }
  }
}

impl std::fmt::Display for SocketSpec {
  fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
    match self {
      SocketSpec::Tcp { host, port } => {
        if let Some(h) = host {
          write!(fmt, "tcp:{}:{}", h, port)
        } else {
          write!(fmt, "tcp:{}", port)
        }
      }

      SocketSpec::UnixAbstract { path } => write!(fmt, "localabstract:{}", path),
      SocketSpec::UnixFilesystem { path } => write!(fmt, "localabstract:{}", path),

      SocketSpec::Vsock { host, port } => {
        if let Some(h) = host {
          write!(fmt, "vsock:{}:{}", h, port)
        } else {
          write!(fmt, "vsock:{}", port)
        }
      }
    }
  }
}

impl TryFrom<&str> for SocketSpec {
  type Error = adb::Error;
  fn try_from(value: &str) -> adb::Result<SocketSpec> {
    if let Some(tail) = value.consume_prefix("tcp:") {
      if let Ok(port) = tail.parse::<u16>() {
        Ok(SocketSpec::tcp(None, port))
      } else {
        let (addr, tail) = if tail.starts_with('[') {
          // IPv6 bracket-enclosed address.
          let close = tail.find(']').ok_or_else(|| adb::Error::SocketSpecInvalid)?;
          tail.split_at(close + 1)
        } else {
          let colon = tail.find(':').ok_or_else(|| adb::Error::SocketSpecInvalid)?;
          tail.split_at(colon)
        };

        if !tail.starts_with(':') {
          return Err(adb::Error::SocketSpecInvalid);
        }
        let port = tail[1..].parse().map_err(|_err| adb::Error::SocketSpecInvalid)?;

        Ok(SocketSpec::tcp(Some(addr.into()), port))
      }
    } else if let Some(tail) = value.consume_prefix("localabstract:") {
      Ok(SocketSpec::unix_abstract(tail))
    } else if let Some(tail) = value.consume_prefix("localfilesystem:") {
      Ok(SocketSpec::unix_filesystem(tail))
    } else if let Some(tail) = value.consume_prefix("local:") {
      Ok(SocketSpec::unix_filesystem(tail))
    } else {
      Err(adb::Error::SocketSpecInvalid)
    }
  }
}

impl std::str::FromStr for SocketSpec {
  type Err = adb::Error;
  fn from_str(s: &str) -> Result<Self, Self::Err> {
    SocketSpec::try_from(s)
  }
}

/// Abstraction for asynchronous sockets.
pub trait Socket: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin> Socket for T {}

#[cfg(test)]
mod test {
  use super::SocketSpec;
  use std::str::FromStr;

  #[test]
  fn parse_tcp_hostless() {
    assert_eq!(
      Some(SocketSpec::Tcp { host: None, port: 5037 }),
      SocketSpec::from_str("tcp:5037").ok()
    );
    assert_eq!(None, SocketSpec::from_str("tcp:").ok());
    assert_eq!(None, SocketSpec::from_str("tcp:-1").ok());
    assert_eq!(None, SocketSpec::from_str("tcp:65536").ok());
  }

  #[test]
  fn parse_tcp_with_host() {
    assert_eq!(
      Some(SocketSpec::Tcp {
        host: Some("localhost".into()),
        port: 1234
      }),
      SocketSpec::from_str("tcp:localhost:1234").ok()
    );
    assert_eq!(None, SocketSpec::from_str("tcp:localhost").ok());
    assert_eq!(None, SocketSpec::from_str("tcp:localhost:").ok());
    assert_eq!(None, SocketSpec::from_str("tcp:localhost:-1").ok());
    assert_eq!(None, SocketSpec::from_str("tcp:localhost:65536").ok());
  }

  #[test]
  fn parse_tcp_ipv6() {
    assert_eq!(
      Some(SocketSpec::Tcp {
        host: Some("[::1]".into()),
        port: 1234
      }),
      SocketSpec::from_str("tcp:[::1]:1234").ok()
    );
    assert_eq!(None, SocketSpec::from_str("tcp:[::1]").ok());
    assert_eq!(None, SocketSpec::from_str("tcp:[::1]:").ok());
    assert_eq!(None, SocketSpec::from_str("tcp:[::1]:-1").ok());
    assert_eq!(None, SocketSpec::from_str("tcp:::1:-1").ok());
    assert_eq!(None, SocketSpec::from_str("tcp:::1:1234").ok());
  }
}
