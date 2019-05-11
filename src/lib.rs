#![feature(async_await)]

pub mod core;

#[cfg(feature = "client")]
pub mod client;

#[cfg(feature = "host")]
pub mod host;

pub(crate) mod util;

pub use crate::core::*;
