use futures::future::Future;
use futures::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use std::pin::Pin;

use byteorder::{ByteOrder, LittleEndian};
use num_derive::{FromPrimitive, ToPrimitive};
use num_traits::{FromPrimitive, ToPrimitive};

use crate as adb;
use crate::client::shell::{Shell, ShellInput, ShellOutput, ShellRead, ShellWrite};
use crate::core::Socket;

#[derive(FromPrimitive, ToPrimitive)]
#[repr(u8)]
enum Id {
  Stdin = 0,
  Stdout = 1,
  Stderr = 2,

  Exit = 3,

  CloseStdin = 4,
}

pub(crate) struct ProtocolShell {
  read: ProtocolShellRead,
  write: ProtocolShellWrite,
}

struct ProtocolShellRead {
  read: Box<AsyncRead + Send + Unpin>,
}

struct ProtocolShellWrite {
  write: Box<AsyncWrite + Send + Unpin>,
}

impl ProtocolShell {
  pub(crate) fn new(channel: Box<Socket>) -> ProtocolShell {
    let (read, write) = channel.split();
    ProtocolShell {
      read: ProtocolShellRead { read: Box::new(read) },
      write: ProtocolShellWrite { write: Box::new(write) },
    }
  }
}

impl Shell for ProtocolShell {
  fn split(self: Box<Self>) -> (Box<ShellRead>, Box<ShellWrite>) {
    (Box::new(self.read), Box::new(self.write))
  }
}

impl ShellRead for ProtocolShell {
  fn read(&mut self) -> Pin<Box<dyn Future<Output = adb::Result<ShellOutput>> + Send + '_>> {
    self.read.read()
  }
}

impl ShellWrite for ProtocolShell {
  fn write(&mut self, event: ShellInput) -> Pin<Box<dyn Future<Output = adb::Result<()>> + Send + '_>> {
    self.write.write(event)
  }
}

impl ShellRead for ProtocolShellRead {
  fn read(&mut self) -> Pin<Box<dyn Future<Output = adb::Result<ShellOutput>> + Send + '_>> {
    Box::pin(async move {
      let mut id = [0u8; 1];
      if self.read.read_exact(&mut id).await.is_err() {
        return Err(adb::Error::UnexpectedData("failed to read shell packet header".into()));
      }

      let mut data_len_buf = [0u8; 4];
      if self.read.read_exact(&mut data_len_buf).await.is_err() {
        return Err(adb::Error::UnexpectedData("failed to read shell data length".into()));
      }

      let data_len = LittleEndian::read_u32(&data_len_buf) as usize;
      let mut data = vec![0u8; data_len];
      if self.read.read_exact(&mut data).await.is_err() {
        return Err(adb::Error::UnexpectedData("failed to read shell data".into()));
      }

      match FromPrimitive::from_u8(id[0]) {
        Some(Id::Stdin) => Err(adb::Error::UnexpectedData(
          "received unexpected Stdin packet from device".into(),
        )),
        Some(Id::CloseStdin) => Err(adb::Error::UnexpectedData(
          "received unexpected CloseStdin packet from device".into(),
        )),

        Some(Id::Stdout) => Ok(ShellOutput::Stdout(data)),
        Some(Id::Stderr) => Ok(ShellOutput::Stderr(data)),

        Some(Id::Exit) => {
          if data.len() != 1 {
            Err(adb::Error::UnexpectedData(format!(
              "received exit packet with incorrect size: {}",
              data.len()
            )))
          } else {
            Ok(ShellOutput::Exit(data[0]))
          }
        }

        None => Err(adb::Error::UnexpectedData(format!(
          "received unexpected packet from device: {}",
          id[0]
        ))),
      }
    })
  }
}

impl ShellWrite for ProtocolShellWrite {
  fn write(&mut self, event: ShellInput) -> Pin<Box<dyn Future<Output = adb::Result<()>> + Send + '_>> {
    Box::pin(async move {
      match event {
        ShellInput::Stdin(data) => {
          let id = [Id::Stdin.to_u8().unwrap()];
          let mut buf = [0u8; 4];
          LittleEndian::write_u32(&mut buf, data.len() as u32);
          self.write.write_all(&id).await?;
          self.write.write_all(&buf).await?;
          self.write.write_all(&data).await?;
          Ok(())
        }

        ShellInput::WindowSizeChange { .. } => Ok(()),

        ShellInput::CloseStdin => {
          self.write.close().await?;
          Ok(())
        }
      }
    })
  }
}
