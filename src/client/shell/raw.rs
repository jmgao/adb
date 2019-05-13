use futures::future::Future;
use futures::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use std::pin::Pin;

use crate as adb;
use crate::client::shell::{Shell, ShellInput, ShellOutput, ShellRead, ShellWrite};
use crate::core::Socket;

pub(crate) struct RawShell {
  read: RawShellRead,
  write: RawShellWrite,
}

struct RawShellRead {
  read: Box<AsyncRead + Send + Unpin>,
}

struct RawShellWrite {
  write: Box<AsyncWrite + Send + Unpin>,
}

impl RawShell {
  pub(crate) fn new(channel: Box<Socket>) -> RawShell {
    let (read, write) = channel.split();
    RawShell {
      read: RawShellRead { read: Box::new(read) },
      write: RawShellWrite { write: Box::new(write) },
    }
  }
}

impl Shell for RawShell {
  fn split(self: Box<Self>) -> (Box<ShellRead>, Box<ShellWrite>) {
    (Box::new(self.read), Box::new(self.write))
  }
}

impl ShellRead for RawShell {
  fn read(&mut self) -> Pin<Box<dyn Future<Output = adb::Result<ShellOutput>> + Send + '_>> {
    self.read.read()
  }
}

impl ShellWrite for RawShell {
  fn write(&mut self, event: ShellInput) -> Pin<Box<dyn Future<Output = adb::Result<()>> + Send + '_>> {
    self.write.write(event)
  }
}

impl ShellRead for RawShellRead {
  fn read(&mut self) -> Pin<Box<dyn Future<Output = adb::Result<ShellOutput>> + Send + '_>> {
    Box::pin(async move {
      let mut buf = [0u8; 2048];
      let len = self.read.read(&mut buf).await;
      let event = match len {
        Ok(0) | Err(_) => ShellOutput::Exit(1),
        Ok(len) => ShellOutput::Stdout(buf[..len].to_vec()),
      };
      Ok(event)
    })
  }
}

impl ShellWrite for RawShellWrite {
  fn write(&mut self, event: ShellInput) -> Pin<Box<dyn Future<Output = adb::Result<()>> + Send + '_>> {
    Box::pin(async move {
      match event {
        ShellInput::Stdin(data) => {
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
