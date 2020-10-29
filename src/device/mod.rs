use super::OdroidC2Error;
use memmap::{MmapMut, MmapOptions};
use nix::fcntl::{open, OFlag};
use nix::sys::stat::Mode;
use std::convert::AsRef;
use std::fs::File;
use std::path::Path;

pub mod error;
mod memory;
mod pin;

use crate::{pin_map, OdroidResult};
use error::DeviceError;
use memory::Memory;

/// The main abstraction for Odroid C2's device.
///
/// This struct owns the memory mapping needed to access GPIO-related registers. It also owns the file handle for device files provided by your operating system.
/// This is the main class you need to instantiate in order to produce input/output GPIO pins to be set by your program.
///
/// **Keep in mind that currently only revision 2 of Odroid C2 is supported by this crate.**
#[derive(Debug)]
pub struct Device {
    _file_handle: File,
    memory: Memory,
}

impl Device {
    const GPIO_BASE_ADDR: u64 = 0xC8834000;
    const BLOCK_SIZE: usize = 4096;

    /// Instantiates new device, opening and memory-mapping an appropriate device file.
    ///
    /// This constructor can fail - if you have no access to device file or memory mapping will fail.
    /// Follow [rootless GPIO access](https://wiki.odroid.com/troubleshooting/gpiomem) article on ODroid wiki if you want to create a device without being a superuser..
    pub fn new() -> OdroidResult<Self> {
        use nix::unistd::Uid;

        let is_root = Uid::current().is_root();

        let (file_handle, map) = if is_root {
            Self::load_device_file("/dev/mem")
        } else {
            Self::load_device_file("/dev/gpiomem")
        }?;

        Ok(Self {
            _file_handle: file_handle,
            memory: Memory::new(map),
        })
    }

    /// Lease a physical GPIO pin, ready for writing.
    ///
    /// This method can fail if there is already an input pin leased by this device.
    /// A `pin_id` argument can be taken from `pin_map::PinId` enum.
    ///
    /// **Note:** This is a potentially expensive method call - it uses synchronized primitives to keep track of lease rules.
    /// Try to initialize required pins before hot paths in your code.
    pub fn output_pin(&mut self, pin_id: pin_map::PinId) -> OdroidResult<pin::OutputPin<'_>> {
        use pin::Direction;
        self.memory.lease_output(pin_id)?;
        let mut pointer_pin = pin::UnsafePointerPin::new(pin_id, &self.memory)?;
        pointer_pin.direction(Direction::Output);

        Ok(pin::OutputPin(pointer_pin))
    }

    /// Lease a physical GPIO pin, ready for reading.
    ///
    /// This method can fail if there is already an output pin leased by this device.
    /// A `pin_id` argument can be taken from `pin_map::PinId` enum.
    ///
    /// **Note:** This is a potentially expensive method call - it uses synchronized primitives to keep track of lease rules.
    /// Try to initialize required pins before hot paths in your code.
    pub fn input_pin(&self, pin_id: pin_map::PinId) -> OdroidResult<pin::InputPin<'_>> {
        use pin::Direction;
        self.memory.lease_input(pin_id)?;
        let mut pointer_pin = pin::UnsafePointerPin::new(pin_id, &self.memory)?;
        pointer_pin.direction(Direction::Input);

        Ok(pin::InputPin(pin::UnsafePointerPin::new(
            pin_id,
            &self.memory,
        )?))
    }

    fn load_device_file<T: AsRef<Path>>(device_path: T) -> OdroidResult<(File, MmapMut)> {
        use std::os::unix::io::FromRawFd;
        use DeviceError::*;

        let mut open_flags = OFlag::empty();
        open_flags.insert(OFlag::O_RDWR);
        open_flags.insert(OFlag::O_SYNC);
        open_flags.insert(OFlag::O_CLOEXEC);

        let file_fd = open(device_path.as_ref(), open_flags, Mode::empty())
            .map_err(|err| OdroidC2Error::DeviceError(DeviceAccessFailed(err)))?;

        // SAFETY: Validity of file_fd is checked by Nix.
        let handle = unsafe { File::from_raw_fd(file_fd) };

        let mut map_opts = MmapOptions::new();
        map_opts.offset(Self::GPIO_BASE_ADDR);
        map_opts.len(Self::BLOCK_SIZE);

        // SAFETY: File handle is valid at this point.
        let map = unsafe {
            map_opts
                .map_mut(&handle)
                .map_err(|err| OdroidC2Error::DeviceError(MemoryMapFailed(err)))?
        };

        Ok((handle, map))
    }
}

pub use pin::{InputPin, OutputPin, Value};
