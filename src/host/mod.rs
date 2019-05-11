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
