use std::io;
use thiserror::Error;

/// Enum representing possible failures when initializing the device.
///
/// Initializing a device can fail in two ways:
/// - DeviceAccessFailed - There is no access to the device file, either because of insufficient permissions or operating system misconfiguration.
/// - MemoryMapFailed - There is an error when trying to create a mmaped piece of memory to represent device file.
#[derive(Error, Debug)]
pub enum DeviceError {
    #[error("failed to open memory device")]
    DeviceAccessFailed(#[source] nix::Error),
    #[error("failed to map device memory")]
    MemoryMapFailed(#[source] io::Error),
}

/// Enum representing possible failures when working with abstract GPIO pins.
///
/// - WrongLease - Client tries to obtain an input pin for a given pin_id when there is one or more output pins for a given pin_id.
/// - WrongPinId - Client tries to pass a bogus pin id which is not recognized by this crate registers mapping.
///   This may be caused when pin ID is not taken from `pin_map::PinId` enum but created manually by other means.
#[derive(Error, Debug)]
pub enum PinError {
    #[error("failed to obtain pin lease - pin is already leased with different mode")]
    WrongLease,
    #[error("unrecognized internal pin id: {0}")]
    WrongPinId(u8),
    #[error("lease map lock poisoned")]
    LeaseMapPoisoned,
}
