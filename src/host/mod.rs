//! Types and functions shared across host implementations (client and server).

/// Integral identifier for transports.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct TransportId(pub u64);

/// Selection criteria for a device.
pub enum DeviceCriteria {
  /// Any device (default in the CLI).
  Any,

  /// Specific serial (-s or `$ANDROID_SERIAL` in the CLI).
  Serial(String),

  /// Transport id (-t in the CLI).
  TransportId(TransportId),

  /// USB device (-d in the CLI).
  Usb,

  /// TCP device (-e in the CLI).
  Tcp,
}

/// Information about a device.
#[derive(Debug)]
pub struct DeviceDescription {
  pub serial: String,
  pub id: TransportId,
  pub transport_type: TransportType,

  pub device_path: Option<String>,
  pub product: Option<String>,
  pub model: Option<String>,
  pub device: Option<String>,
}

/// A device's self-reported type.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum DeviceType {
  Bootloader,
  Device,
  Host,
  Recovery,
  Rescue,
  Sideload,
}

impl DeviceType {
  fn to_str(self) -> &'static str {
    match self {
      DeviceType::Bootloader => "bootloader",
      DeviceType::Device => "device",
      DeviceType::Host => "host",
      DeviceType::Recovery => "recovery",
      DeviceType::Rescue => "rescue",
      DeviceType::Sideload => "sideload",
    }
  }
}

impl std::fmt::Display for DeviceType {
  fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
    write!(f, "{}", self.to_str())
  }
}

/// The state of a connection.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum TransportType {
  /// Offline.
  Offline,

  /// The adb server knows that there's a USB device available, but it lacks the permissions to access it.
  NoPermissions,

  /// The adb server attempted to authorize but failed to do so, and fell back to prompting for user authentication.
  Unauthorized,

  /// The adb server is in the midst of authentcation.
  Authorizing,

  /// The adb server is in the midst of initiating a socket connection.
  Connecting,

  /// The device is online.
  Online(DeviceType),
}

impl TransportType {
  fn to_str(self) -> &'static str {
    match self {
      TransportType::Offline => "offline",
      TransportType::NoPermissions => "no permissions",
      TransportType::Unauthorized => "unauthorized",
      TransportType::Authorizing => "authorizing",
      TransportType::Connecting => "connecting",
      TransportType::Online(device_type) => device_type.to_str(),
    }
  }
}

impl std::fmt::Display for TransportType {
  fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
    write!(f, "{}", self.to_str())
  }
}
