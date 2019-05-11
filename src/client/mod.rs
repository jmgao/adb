//! Types and functions for client implementations.

use byteorder::{ByteOrder, LittleEndian};
use futures::io::{AsyncReadExt, AsyncWriteExt};

use crate as adb;
use crate::core::{Socket, SocketSpec};
use crate::host::{DeviceCriteria, TransportId};

/// A pointer to the location of an adb server.
pub struct Remote {
  socket_spec: SocketSpec,
}

async fn write_hex_length_prefixed(socket: &mut Socket, bytes: impl Into<Vec<u8>>) -> adb::Result<()> {
  let bytes = bytes.into();
  let s = format!("{:04x}", bytes.len());
  socket.write_all(s.as_bytes()).await?;
  socket.write_all(&bytes).await?;
  Ok(())
}

async fn read_hex_length_prefixed(socket: &mut Socket) -> adb::Result<Vec<u8>> {
  let mut length = [0u8; 4];
  socket.read_exact(&mut length).await?;

  let length_str =
    std::str::from_utf8(&length).map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;

  let length =
    usize::from_str_radix(length_str, 16).map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;

  let mut vec = vec![0; length];
  socket.read_exact(&mut vec).await?;
  Ok(vec)
}

async fn read_okay(socket: &mut Socket) -> adb::Result<()> {
  let mut okay = [0u8; 4];
  socket.read_exact(&mut okay).await?;

  if &okay == b"OKAY" {
    Ok(())
  } else if &okay == b"FAIL" {
    // Try to read the error.
    let error = read_hex_length_prefixed(socket).await?;
    let error_str = String::from_utf8_lossy(&error);
    Err(adb::Error::ServiceError(error_str.into_owned()))
  } else {
    let error_str = format!("expected OKAY or FAIL, got {}", String::from_utf8_lossy(&okay));
    Err(adb::Error::UnexpectedData(error_str))
  }
}

impl Remote {
  /// Constructs a new `Remote`.
  pub fn new(socket_spec: SocketSpec) -> Remote {
    Remote { socket_spec }
  }

  /// Opens a channel to a raw adb service.
  ///
  /// No device-selection prefix is prepended, use [Remote::open_device_channel] if you wish to connect to a device
  /// service.
  pub async fn open_channel(&self, service: impl AsRef<str>) -> adb::Result<Box<Socket>> {
    let mut channel = self.socket_spec.connect().await?;

    write_hex_length_prefixed(&mut channel, service.as_ref().as_bytes()).await?;
    read_okay(&mut channel).await?;
    Ok(channel)
  }

  async fn open_device_channel_id(&self, id: TransportId, service: impl AsRef<str>) -> adb::Result<Box<Socket>> {
    let s = format!("host:transport-id:{}", id.0);
    let mut channel = self.open_channel(s).await?;

    write_hex_length_prefixed(&mut channel, service.as_ref().as_bytes()).await?;
    read_okay(&mut channel).await?;
    Ok(channel)
  }

  async fn open_device_channel_tport(
    &self,
    tport_str: impl AsRef<str>,
    service: impl AsRef<str>,
  ) -> adb::Result<(TransportId, Box<Socket>)> {
    let mut channel = self.open_channel(tport_str).await?;
    let mut tport = [0u8; 8];
    channel.read_exact(&mut tport).await?;

    let id = TransportId(LittleEndian::read_u64(&tport));
    write_hex_length_prefixed(&mut channel, service.as_ref().as_bytes()).await?;
    read_okay(&mut channel).await?;
    Ok((id, channel))
  }

  /// Opens a channel to a service on a device specified by the provided [DeviceCriteria].
  pub async fn open_device_channel(
    &self,
    criteria: DeviceCriteria,
    service: impl AsRef<str>,
  ) -> adb::Result<(TransportId, Box<Socket>)> {
    let service = service.as_ref();
    // Use the host:tport service to select a device and get its transport id back.
    let (transport_id, channel) = match criteria {
      DeviceCriteria::Any => self.open_device_channel_tport("host:tport:any", service).await?,
      DeviceCriteria::Usb => self.open_device_channel_tport("host:tport:usb", service).await?,
      DeviceCriteria::Tcp => self.open_device_channel_tport("host:tport:tcp", service).await?,
      DeviceCriteria::Serial(serial) => {
        let s = format!("host:tport:serial:{}", serial);
        self.open_device_channel_tport(&s, service).await?
      }
      DeviceCriteria::TransportId(id) => (id, self.open_device_channel_id(id, service).await?),
    };

    Ok((transport_id, channel))
  }

  /// Get the server's protocol version.
  pub async fn version(&self) -> adb::Result<u32> {
    let mut channel = self.open_channel("host:version").await?;
    let version = read_hex_length_prefixed(&mut channel).await?;
    let version_str =
      std::str::from_utf8(&version).map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
    let version =
      u32::from_str_radix(version_str, 16).map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
    Ok(version)
  }
}

impl Default for Remote {
  /// Construct a `Remote` pointing to the default adb server location (127.0.0.1:5037).
  fn default() -> Remote {
    // TODO: Support IPv6 localhost?
    Remote::new(SocketSpec::tcp(Some("127.0.0.1".into()), 5037))
  }
}
