//! Types and functions for client implementations.

use byteorder::{ByteOrder, LittleEndian};
use futures::io::{AsyncReadExt, AsyncWriteExt};
use regex::Regex;

use crate as adb;
use crate::core::{Socket, SocketSpec};
use crate::host::{DeviceCriteria, DeviceDescription, DeviceType, TransportId, TransportType};
use crate::util::{ConsumePrefix, SplitOnce};

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
    let version_str = std::str::from_utf8(&version)
      .map_err(|_| adb::Error::UnexpectedData("version service returned invalid UTF-8".into()))?;
    let version = u32::from_str_radix(version_str, 16)
      .map_err(|_| adb::Error::UnexpectedData(format!("invalid version received '{}'", version_str)))?;
    Ok(version)
  }

  pub async fn devices(&self) -> adb::Result<Vec<DeviceDescription>> {
    let mut channel = self.open_channel("host:devices-l").await?;
    let devices = read_hex_length_prefixed(&mut channel).await?;
    let devices_str = String::from_utf8_lossy(&devices);

    let mut result = Vec::new();
    // TODO: Use an actual protocol instead of parsing user-readable string output.
    for line in devices_str.split('\n') {
      if line.is_empty() {
        continue;
      }

      let (serial, middle) = line
        .split_once(" ")
        .ok_or_else(|| adb::Error::UnexpectedData(format!("invalid device line: '{}'", line)))?;

      let (transport_id_str, middle) = middle
        .rsplit_once(" transport_id:")
        .ok_or_else(|| adb::Error::UnexpectedData(format!("transport_id missing in device line: '{}'", line)))?;

      let transport_id = TransportId(
        transport_id_str
          .parse()
          .map_err(|_| adb::Error::UnexpectedData(format!("invalid transport id in device line: '{}'", line)))?,
      );

      // The easy part is done. Now for some especially horrible string parsing:
      // First, trim the alignment spaces.
      let middle = middle.trim_start();

      // Next, parse the transport type.
      // This is especially horrible, because it can be the following text:
      //   "no permissions; see [http://developer.android.com/tools/device.html]"
      // Thankfully, we can just check for "no permissions" and stop there, because there won't be any additional info.
      let (transport_type, middle) = if middle.starts_with("offline") {
        (TransportType::Offline, "")
      } else if middle.starts_with("no permissions") {
        (TransportType::NoPermissions, "")
      } else if middle.starts_with("unauthorized") {
        (TransportType::Unauthorized, "")
      } else if middle.starts_with("authorizing") {
        (TransportType::Authorizing, "")
      } else if middle.starts_with("connecting") {
        (TransportType::Connecting, "")
      } else {
        // We are presumably connected. Figure out what our DeviceType is.
        let (device_type, middle) = if let Some(s) = middle.consume_prefix("bootloader ") {
          (DeviceType::Bootloader, s)
        } else if let Some(s) = middle.consume_prefix("device ") {
          (DeviceType::Device, s)
        } else if let Some(s) = middle.consume_prefix("host ") {
          (DeviceType::Host, s)
        } else if let Some(s) = middle.consume_prefix("recovery ") {
          (DeviceType::Recovery, s)
        } else if let Some(s) = middle.consume_prefix("rescue ") {
          (DeviceType::Rescue, s)
        } else if let Some(s) = middle.consume_prefix("sideload ") {
          (DeviceType::Sideload, s)
        } else {
          return Err(adb::Error::UnexpectedData(format!(
            "failed to parse device type from device line '{}'",
            line
          )));
        };

        (TransportType::Online(device_type), middle)
      };

      // The rest is relatively easy.
      // The first element might be a device path, after which we might have product, model, and device.
      let captures = if middle.is_empty() {
        None
      } else {
        let re = Regex::new(
          r"(?P<device_path>\S+)(?: product:(?P<product>\S+))?(?: model:(?P<model>\S+))?(?: device:(?P<device>\S+))?",
        )
        .unwrap();
        re.captures(middle)
      };

      result.push(DeviceDescription {
        serial: serial.into(),
        id: transport_id,
        transport_type,
        device_path: captures.as_ref().and_then(|c| c.name("device_path").map(|s| s.as_str().into())),
        product: captures.as_ref().and_then(|c| c.name("product").map(|s| s.as_str().into())),
        model: captures.as_ref().and_then(|c| c.name("model").map(|s| s.as_str().into())),
        device: captures.as_ref().and_then(|c| c.name("device").map(|s| s.as_str().into())),
      })
    }
    Ok(result)
  }
}

impl Default for Remote {
  /// Construct a `Remote` pointing to the default adb server location (127.0.0.1:5037).
  fn default() -> Remote {
    // TODO: Support IPv6 localhost?
    Remote::new(SocketSpec::tcp(Some("127.0.0.1".into()), 5037))
  }
}
