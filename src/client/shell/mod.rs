use futures::future::Future;
use std::pin::Pin;

use crate as adb;
use crate::client::Remote;
use crate::host::DeviceCriteria;

mod raw;
use raw::RawShell;

mod protocol;
use protocol::ProtocolShell;

/// Events transmitted from the shell client to the shell service.
#[derive(Debug)]
pub enum ShellInput {
  Stdin(Vec<u8>),
  CloseStdin,
  WindowSizeChange {
    rows: u16,
    cols: u16,
    xpixels: u16,
    ypixels: u16,
  },
}

/// Events transmitted from the shell service to the shell client.
#[derive(Debug)]
pub enum ShellOutput {
  Stdout(Vec<u8>),
  Stderr(Vec<u8>),
  Exit(u8),
}

pub trait ShellRead: Send {
  fn read(&mut self) -> Pin<Box<dyn Future<Output = adb::Result<ShellOutput>> + Send + '_>>;
}

pub trait ShellWrite: Send {
  fn write(&mut self, event: ShellInput) -> Pin<Box<dyn Future<Output = adb::Result<()>> + Send + '_>>;
}

pub trait Shell: ShellRead + ShellWrite + Send {
  fn split(self: Box<Self>) -> (Box<ShellRead>, Box<ShellWrite>);
}

impl Shell {
  pub fn builder() -> ShellBuilder {
    ShellBuilder::new()
  }
}

/// Builder for a [Shell].
pub struct ShellBuilder {
  command: Option<Vec<String>>,
  shell_protocol: Option<bool>,
  term: Option<String>,
  tty: Option<bool>,
}

impl ShellBuilder {
  pub fn new() -> ShellBuilder {
    ShellBuilder {
      command: None,
      shell_protocol: None,
      term: None,
      tty: None,
    }
  }

  pub fn command(&mut self, cmd: Option<Vec<String>>) -> &mut ShellBuilder {
    self.command = cmd;
    self
  }

  pub fn shell_protocol(&mut self, enabled: bool) -> &mut ShellBuilder {
    self.shell_protocol = Some(enabled);
    self
  }

  pub fn term(&mut self, term: Option<String>) -> &mut ShellBuilder {
    self.term = term;
    self
  }

  pub fn tty(&mut self, tty: bool) -> &mut ShellBuilder {
    self.tty = Some(tty);
    self
  }

  pub async fn connect(&self, remote: Remote, device_criteria: DeviceCriteria) -> adb::Result<Box<Shell>> {
    let shell_protocol = match self.shell_protocol {
      Some(value) => value,
      None => {
        return Err(adb::Error::UnimplementedOperation(
          "feature detection not implemented yet".into(),
        ))
      }
    };

    if shell_protocol {
      let mut service = "shell,v2".to_string();
      if self.tty.unwrap_or(false) {
        if let Some(term) = &self.term {
          service.push_str(",TERM=");
          service.push_str(term);
        }
        service.push_str(",pty");
      } else {
        service.push_str(",raw");
      }
      service.push(':');

      if let Some(cmd) = &self.command {
        service.push_str(&cmd.join(" "));
      }

      let (_, channel) = remote.open_device_channel(device_criteria, service).await?;
      let shell: Box<Shell> = Box::new(ProtocolShell::new(channel));
      Ok(shell)
    } else {
      let service = if let Some(cmd) = &self.command {
        "shell:".to_string() + &cmd.join(" ")
      } else {
        "shell:".into()
      };

      let (_, channel) = remote.open_device_channel(device_criteria, service).await?;
      let shell: Box<Shell> = Box::new(RawShell::new(channel));
      Ok(shell)
    }
  }
}
