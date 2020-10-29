//! This crate exposes GPIO interface of [ODROID-C2](https://www.hardkernel.com/shop/odroid-c2/) SBC for programmatic use in Rust.
//!
//! **Only revision 2 of the board is supported by this crate.** I don't have an access to other devices. If you happen to have one and want to use this library,
//! you can fork it and replace certain parts of it. You may need to modify `RegistersOffsets`, base address inside `Device` struct and change `PinId` enum as needed.
//! If you happen to do it, contributions are more than welcome!
//!
//! Basic abstractions for GPIO pins (`InputPin`, `OutputPin`) implements relevant [`embedded_hal`](https://crates.io/crates/embedded-hal)
//! abstractions so this crate can be used with driver implementations using `embedded_hal` generic traits.
//!
//! Code has been tested on ODROID-C2 rev2 device using [Ubuntu 18.04 LTS](https://wiki.odroid.com/odroid-c2/os_images/ubuntu/v3.0) operating system.
//!
//! This project is in a very early stage of development and is done by embedded systems amateur. Complete rewrites of abstractions are possible!

use thiserror::Error;

mod device;
mod pin_map;

pub use device::error::DeviceError;
pub use device::error::PinError;
pub use device::{Device, InputPin, OutputPin, Value};
pub use pin_map::PinId;

/// Main error type for this crate.
///
/// For more details, see `PinError` and `DeviceError` enums documentation.
#[derive(Error, Debug)]
pub enum OdroidC2Error {
    #[error("error while operating on a pin")]
    PinError(#[source] device::error::PinError),
    #[error("error while operating on a device")]
    DeviceError(#[source] device::error::DeviceError),
}

pub type OdroidResult<T> = Result<T, OdroidC2Error>;
