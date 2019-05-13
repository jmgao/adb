#![feature(async_await)]

macro_rules! fatal {
  ($($tt:tt)*) => {{
    use std::io::Write;
    write!(&mut ::std::io::stderr(), "fatal: ").unwrap();
    writeln!(&mut ::std::io::stderr(), $($tt)*).unwrap();
    ::std::process::exit(1)
  }}
}

#[cfg(not(feature = "client-binary"))]
fn main() {
  eprintln!("adb client-binary feature not enabled");
  std::process::exit(1)
}

#[cfg(feature = "client-binary")]
fn main() -> adb::Result<()> {
  client::main()
}

#[cfg(feature = "client-binary")]
mod client {
  use adb::core::*;
  use adb::host::*;
  use clap::{clap_app, crate_version};

  use futures::executor::{self, ThreadPool};
  use futures::future;
  use futures::io::{AsyncReadExt, AsyncWriteExt};
  use futures::task::SpawnExt;

  pub(crate) fn main() -> adb::Result<()> {
    let app = clap_app!(("adb-rs") =>
      (version: crate_version!())
      (@setting UnifiedHelpMessage)
      (@setting SubcommandRequiredElseHelp)
      (@setting VersionlessSubcommands)

      (global_setting: clap::AppSettings::ColoredHelp)

      // The defaults for these have the wrong tense and capitalization.
      // TODO: Write an upstream patch to allow overriding these in the subcommends globally.
      (help_message: "print help message")
      (version_message: "print version information")

      (@arg LISTEN_ALL: -a display_order(0) "listen on all network interfaces, not just localhost")
      (@arg DEVICE_SELECT_USB: -d display_order(1) "use USB device (error if multiple available)")
      (@arg DEVICE_SELECT_TCP: -e display_order(2) "use TCP/IP device (error if multiple available)")
      (@arg DEVICE_SELECT_SERIAL: -s +takes_value value_names(&["SERIAL"]) display_order(3)
                                  "use device with given serial (overrides $ANDROID_SERIAL)")
      (@arg DEVICE_SELECT_TRANSPORT_ID: -t
        +takes_value value_names(&["ID"]) display_order(4)
        overrides_with_all(&["DEVICE_SELECT_USB", "DEVICE_SELECT_TCP", "DEVICE_SELECT_SERIAL"])
        "use device with given transport id"
      )

      (@arg HOST: -H +takes_value display_order(5) conflicts_with("SPEC") "hostname of adb server")
      (@arg PORT: -P +takes_value display_order(6) conflicts_with("SPEC") "port of adb server")
      (@arg SPEC: -L +takes_value display_order(7) "socket specification of adb server")

      (@subcommand version =>
        (about: "display version information")
      )

      (@subcommand devices =>
        (about: "display connected devices")
        (@arg LONG: -l "long output")
      )

      (@subcommand raw =>
        (about: "directly connect to a service")
        (@arg SERVICE: +required "service to connect to")
        (@setting Hidden)
      )
    );

    let matches = app.get_matches();
    let criteria = if matches.is_present("DEVICE_SELECT_USB") {
      DeviceCriteria::Usb
    } else if matches.is_present("DEVICE_SELECT_TCP") {
      DeviceCriteria::Tcp
    } else if let Some(serial) = matches.value_of("DEVICE_SELECT_SERIAL") {
      DeviceCriteria::Serial(serial.to_string())
    } else if let Some(id_str) = matches.value_of("DEVICE_SELECT_TRANSPORT_ID") {
      let id = id_str
        .parse()
        .unwrap_or_else(|_| fatal!("failed to parse transport id '{}'", id_str));
      DeviceCriteria::TransportId(TransportId(id))
    } else if let Ok(serial) = std::env::var("ADB_SERIAL") {
      DeviceCriteria::Serial(serial)
    } else {
      DeviceCriteria::Any
    };

    let server_address = if let Some(spec) = matches.value_of("SPEC") {
      spec
        .parse()
        .unwrap_or_else(|_| fatal!("failed to parse socket spec '{}'", spec))
    } else {
      let host = matches.value_of("HOST").unwrap_or("127.0.0.1");
      let port = matches
        .value_of("PORT")
        .map(|s| s.parse().unwrap_or_else(|_| fatal!("failed to parse port '{}'", s)))
        .unwrap_or(5037);
      SocketSpec::tcp(Some(host.into()), port)
    };

    let result = || -> Result<i32> {
      executor::block_on(async {
        match matches.subcommand() {
          ("version", Some(_)) => cmd_version(server_address).await,
          ("devices", Some(submatches)) => cmd_devices(server_address, submatches.is_present("LONG")).await,

          ("raw", Some(submatches)) => {
            let service = submatches.value_of("SERVICE").unwrap();
            cmd_raw(server_address, criteria, service).await
          }

          (cmd, None) => fatal!("mismatched command {}", cmd),
          (cmd, Some(_)) => fatal!("unhandled command {}", cmd),
        }
      })
    }();

    match result {
      Ok(rc) => std::process::exit(rc),
      Err(err) => fatal!("{:?}", err),
    }
  }

  async fn cmd_version(server: SocketSpec) -> Result<i32> {
    println!("adb-rs {}", crate_version!());
    let remote = adb::client::Remote::new(server.clone());
    if let Ok(version) = remote.version().await {
      println!("Server version ({}): {}", server, version);
    }
    Ok(0)
  }

  async fn cmd_devices(server: SocketSpec, long_output: bool) -> Result<i32> {
    let remote = adb::client::Remote::new(server);
    let devices = remote.devices().await?;

    println!("List of devices attached");
    for device in devices {
      if long_output {
        let mut options = Vec::with_capacity(5);
        if let Some(s) = device.device_path {
          options.push(s);
        }

        if let Some(s) = device.product {
          options.push(format!("product:{}", s));
        }

        if let Some(s) = device.model {
          options.push(format!("model:{}", s));
        }

        if let Some(s) = device.device {
          options.push(format!("device:{}", s));
        }

        options.push(format!("transport_id:{}", device.id.0));
        println!("{: <22} {} {}", device.serial, device.transport_type, options.join(" "));
      } else {
        println!("{}\t{}", device.serial, device.transport_type);
      }
    }
    println!();

    Ok(0)
  }

  async fn cmd_raw(server: SocketSpec, device_criteria: DeviceCriteria, service: &str) -> Result<i32> {
    let mut pool = ThreadPool::new()?;
    let remote = adb::client::Remote::new(server);

    let channel = if service.starts_with("host:") {
      remote.open_channel(service).await?
    } else {
      let (_, socket) = remote.open_device_channel(device_criteria, service).await?;
      socket
    };

    let (mut channel_read, mut channel_write) = channel.split();
    let read = pool
      .spawn_with_handle(async move {
        let mut stdout = futures::io::AllowStdIo::new(std::io::stdout());
        let mut buf = [0u8; 2048];
        loop {
          match channel_read.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(len) => {
              let _ = stdout.write_all(&buf[..len]).await;
              let _ = stdout.flush().await;
            }
          }
        }
      })
      .unwrap();

    let write = pool
      .spawn_with_handle(async move {
        let mut stdin = futures::io::AllowStdIo::new(std::io::stdin());
        let _ = stdin.copy_into(&mut channel_write).await;
      })
      .unwrap();

    future::select(read, write).await;
    Ok(0)
  }
}
