/// Memory representation of Odroid C2 GPIO interface.
///
/// This module consists of two important structs:
///
/// * `OdroidC2GPIO` - represents the whole GPIO interface. To operate on a pin you take single pin representation using `OdroidC2GPIO::pin` function.
/// * `MemoryPin` - represents one physical pin of GPIO interface. You pass this pin to appropriate methods of your `OdroidC2GPIO` instance.
use byteorder::{ByteOrder, NativeEndian};
use memmap::{MmapMut, MmapOptions};
use nix::fcntl::{open, OFlag};
use nix::sys::stat::Mode;
use nix::unistd::Uid;
use std::fs::File;
use std::io;
use std::mem::size_of;
use std::ops::RangeInclusive;
use std::ops::{Deref, DerefMut};
use thiserror::Error;

use crate::pins;

/// Enum representing all possible errors returned by this crate.
///
/// `OpenFailed` may occur when there is an error trying to open appropriate devices (`/dev/mem` for root, `/dev/gpiomem` for non-root). This is usually a problem of permissions to read these devices.
/// `MapError` occurs when memory mapping fails. This may be caused by other GPIO kernel drivers interfering with memory-mapping process.
/// `PinError` can occur when you pass invalid pin value to `OdroidC2GPIO::pin` function.
#[derive(Error, Debug)]
pub enum OdroidC2GPIOError {
    #[error("failed to open descriptor")]
    OpenFdFailed(#[source] nix::Error),
    #[error("failed to map device memory")]
    MapError(#[source] io::Error),
    #[error("pin not in any range known by the library")]
    PinError,
}

#[derive(Debug)]
struct Memory {
    map: MmapMut,
}

impl Deref for Memory {
    type Target = MmapMut;

    fn deref(&self) -> &Self::Target {
        &self.map
    }
}

impl DerefMut for Memory {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.map
    }
}

/// Specifies the direction of a given pin.
///
/// In an `Input` mode you can read values from the pin and the line is set to low mode.
/// In an `Output` mode you can write values to the pin.
#[derive(Copy, Clone, Debug)]
pub enum PinDirection {
    Input,
    Output,
}

#[derive(Copy, Clone, Debug)]
struct PinRegisters {
    pin_u8: u8,
    range_type: PinRangeType,
}

#[derive(Copy, Clone, Debug)]
enum PinRangeType {
    DV,
    Y,
    X,
}

impl PinRegisters {
    const PIN_BASE: u8 = 136;
    const DV_PINS_RANGE: RangeInclusive<u8> = ((Self::PIN_BASE + 45)..=(Self::PIN_BASE + 74));
    const Y_PINS_RANGE: RangeInclusive<u8> = ((Self::PIN_BASE + 75)..=(Self::PIN_BASE + 91));
    const X_PINS_RANGE: RangeInclusive<u8> = ((Self::PIN_BASE + 92)..=(Self::PIN_BASE + 114));

    pub fn new(pin: pins::Pin) -> Option<Self> {
        let pin_u8 = pin as u8;

        if Self::DV_PINS_RANGE.contains(&pin_u8) {
            Some(PinRangeType::DV)
        } else if Self::Y_PINS_RANGE.contains(&pin_u8) {
            Some(PinRangeType::Y)
        } else if Self::X_PINS_RANGE.contains(&pin_u8) {
            Some(PinRangeType::X)
        } else {
            None
        }
        .map(|range_type| Self { range_type, pin_u8 })
    }

    fn range(&self) -> &'static RangeInclusive<u8> {
        use PinRangeType::*;
        match self.range_type {
            DV => &Self::DV_PINS_RANGE,
            Y => &Self::Y_PINS_RANGE,
            X => &Self::X_PINS_RANGE,
        }
    }

    pub fn gplev(&self) -> usize {
        use PinRangeType::*;
        (match self.range_type {
            DV => 0x10E,
            Y => 0x111,
            X => 0x11A,
        } * size_of::<u32>())
    }

    pub fn gpset(&self) -> usize {
        use PinRangeType::*;
        (match self.range_type {
            DV => 0x10D,
            Y => 0x110,
            X => 0x119,
        } * size_of::<u32>())
    }

    pub fn gpfsel(&self) -> usize {
        use PinRangeType::*;
        (match self.range_type {
            DV => 0x10C,
            Y => 0x10F,
            X => 0x118,
        } * size_of::<u32>())
    }

    pub fn puen(&self) -> usize {
        use PinRangeType::*;
        (match self.range_type {
            DV => 0x13A,
            Y => 0x149,
            X => 0x14C,
        } * size_of::<u32>())
    }

    pub fn pupd(&self) -> usize {
        use PinRangeType::*;
        (match self.range_type {
            DV => 0x148,
            Y => 0x13B,
            X => 0x13E,
        } * size_of::<u32>())
    }

    pub fn pin_bitmap_offset(&self) -> u8 {
        self.pin_u8 - self.range().start()
    }
}

/// Represents one physical pin in a memory-mapped model of the library.
///
/// This representation allows to do certain checks needed to operate on the pin only once.
/// All `OdroidC2GPIO` methods for reading, writing and pin management accepts this struct as an argument.
///
/// # Example
/// ```
/// use c2_mmap_gpio::{gpio::{OdroidC2GPIO, MemoryPin, PinDirection, PinValue}, pins::Pin};
///
/// fn main() -> Result<(), dyn std::error::Error> {
///   let mut gpio = OdroidC2GPIO::new()?;
///   // Get memory representation of physical pin 7.
///   let pin: MemoryPin = OdroidC2GPIO::pin(Pin::Phy7)?;
///   gpio.direction(&pin, PinDirection::Output);
///   gpio.write(&pin, PinValue::High);
///
///   Ok(())
/// }
#[derive(Debug)]
pub struct MemoryPin {
    pin: pins::Pin,
    registers: PinRegisters,
}

#[derive(Copy, Clone, Debug)]
struct UnsafePtrPin<'memory> {
    memory: &'memory Memory,
    registers: PinRegisters,
}

#[derive(Copy, Clone, Debug)]
pub struct InputPin<'memory>(UnsafePtrPin<'memory>);

#[derive(Copy, Clone, Debug)]
pub struct OutputPin<'memory>(UnsafePtrPin<'memory>);

use core::ptr::{self, NonNull};

impl<'memory> UnsafePtrPin<'memory> {
    pub fn new(pin: pins::Pin, memory: &'memory Memory) -> Result<Self, OdroidC2GPIOError> {
        Ok(Self {
            registers: PinRegisters::new(pin).ok_or(OdroidC2GPIOError::PinError)?,
            memory,
        })
    }

    pub fn mode(&mut self, direction: PinDirection) {
        use PinDirection::*;

        let mut fsel_reg = self.reg_ptr(self.registers.gpfsel());
        let mut puen_reg = self.reg_ptr(self.registers.puen());

        let pin_offset = self.registers.pin_bitmap_offset();
        let fsel = unsafe { fsel_reg.as_mut() };
        let puen = unsafe { puen_reg.as_mut() };

        let fsel_retval = NativeEndian::read_u32(fsel);

        match direction {
            Input => {
                NativeEndian::write_u32(fsel, fsel_retval | (1 << pin_offset));
                let puen_retval = NativeEndian::read_u32(puen);
                NativeEndian::write_u32(puen, puen_retval & !(1 << pin_offset));
            }
            Output => NativeEndian::write_u32(fsel, fsel_retval & !(1 << pin_offset)),
        }
    }

    pub fn write(&mut self, value: PinValue) {
        use PinValue::*;

        let mut gpset_reg = self.reg_ptr(self.registers.gpset());
        let pin_offset = self.registers.pin_bitmap_offset();
        let gpset = unsafe { gpset_reg.as_mut() };
        let gpset_retval = NativeEndian::read_u32(gpset);

        match value {
            High => {
                NativeEndian::write_u32(gpset, gpset_retval | (1 << pin_offset));
            }
            Low => {
                NativeEndian::write_u32(gpset, gpset_retval & !(1 << pin_offset));
            }
        }
    }

    pub fn read(&self) -> PinValue {
        use PinValue::*;
        let mut gplev_reg = self.reg_ptr(self.registers.gplev());
        let gplev = unsafe { gplev_reg.as_mut() };
        let gplev_retval = NativeEndian::read_u32(gplev);
        let pin_offset = self.registers.pin_bitmap_offset();

        let val = gplev_retval & (1 << pin_offset);

        if val == 0 {
            Low
        } else {
            High
        }
    }

    fn reg_ptr(&self, reg_offset: usize) -> NonNull<[u8]> {
        let base_addr: *const u8 = self.memory.as_ptr();
        unsafe {
            let ptr = base_addr.add(reg_offset) as *mut u8;
            NonNull::new_unchecked(ptr::slice_from_raw_parts_mut(ptr, size_of::<u32>()))
        }
    }
}

impl MemoryPin {
    fn new(pin: pins::Pin) -> Result<Self, OdroidC2GPIOError> {
        match PinRegisters::new(pin) {
            Some(registers) => Ok(Self { pin, registers }),
            None => Err(OdroidC2GPIOError::PinError),
        }
    }

    fn mode(&self, memory: &mut Memory, direction: PinDirection) {
        use PinDirection::*;

        let pin_offset = self.registers.pin_bitmap_offset();
        let fsel = self.registers.gpfsel();
        let puen = self.registers.puen();

        let fsel_rv = NativeEndian::read_u32(&memory[fsel..]);
        /*
         * In case of output we just need to flip pin bit.
         * In case of input mode we need to also disable the pull up/down resistor.
         */
        match direction {
            Input => {
                NativeEndian::write_u32(&mut memory[fsel..], fsel_rv | (1 << pin_offset));
                let puen_rv = NativeEndian::read_u32(&memory[puen..]);
                NativeEndian::write_u32(&mut memory[puen..], puen_rv & !(1 << pin_offset));
            }
            Output => {
                NativeEndian::write_u32(&mut memory[fsel..], fsel_rv & !(1 << pin_offset));
            }
        }
    }

    fn write(&self, memory: &mut Memory, value: PinValue) {
        let gpset = self.registers.gpset();
        let set_v = NativeEndian::read_u32(&memory[gpset..]);
        let pin_offset = self.registers.pin_bitmap_offset();

        match value {
            PinValue::High => {
                NativeEndian::write_u32(&mut memory[gpset..], set_v | (1 << pin_offset));
            }
            PinValue::Low => {
                NativeEndian::write_u32(&mut memory[gpset..], set_v & !(1 << pin_offset));
            }
        }
    }

    fn read(&self, memory: &Memory) -> PinValue {
        let read_v = NativeEndian::read_u32(&memory[self.registers.gplev()..]);
        let val = read_v & (1 << self.registers.pin_bitmap_offset());

        if val == 0 {
            PinValue::Low
        } else {
            PinValue::High
        }
    }
}

impl Memory {
    fn new(map: MmapMut) -> Self {
        Self { map }
    }
}

/// Struct representing the memory-mapped model of Odroid C2 pins.
///
/// It contains the memory-mapped region of device memory which allows to set up registers to operate GPIO pins. That means your requests to read/write/manage pins needs to go through this struct.
pub struct OdroidC2GPIO {
    _file_handle: File,
    memory: Memory,
}

/// Enum representing possible values for a pin. It corresponds to low and high voltage states of GPIO pins.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum PinValue {
    High = 0,
    Low = 1,
}

impl OdroidC2GPIO {
    const GPIO_BASE_ADDR: u64 = 0xC8834000;
    const BLOCK_SIZE: usize = 4096;

    /// Creates all necessary memory mappings to operate on GPIO pins of your device.
    ///
    /// It can fail with `OdroidC2GPIOError::OpenFdFailed` when you have no access to `/dev/mem` or `/dev/gpiomem` in your device.
    /// Follow [rootless GPIO access](https://wiki.odroid.com/troubleshooting/gpiomem) article on ODroid wiki if you want to operate on GPIO pins without root access.
    pub fn new() -> Result<Self, OdroidC2GPIOError> {
        let is_root = Uid::current().is_root();

        let (file_handle, map) = if is_root {
            Self::get_handle("/dev/mem")
        } else {
            Self::get_handle("/dev/gpiomem")
        }?;

        Ok(Self {
            _file_handle: file_handle,
            memory: Memory::new(map),
        })
    }

    fn get_handle(device_path: &str) -> Result<(File, MmapMut), OdroidC2GPIOError> {
        use std::os::unix::io::FromRawFd;
        use OdroidC2GPIOError::*;

        let mut open_flags = OFlag::empty();
        open_flags.insert(OFlag::O_RDWR);
        open_flags.insert(OFlag::O_SYNC);
        open_flags.insert(OFlag::O_CLOEXEC);

        let file_fd = open(device_path, open_flags, Mode::empty()).map_err(OpenFdFailed)?;

        let handle = unsafe { File::from_raw_fd(file_fd) };

        let mut map_opts = MmapOptions::new();
        map_opts.offset(Self::GPIO_BASE_ADDR);
        map_opts.len(Self::BLOCK_SIZE);

        let map = unsafe { map_opts.map_mut(&handle).map_err(MapError)? };

        Ok((handle, map))
    }

    pub fn input_pin(&self, pin: pins::Pin) -> Result<InputPin, OdroidC2GPIOError> {
        let mut ptr_pin = UnsafePtrPin::new(pin, &self.memory)?;
        ptr_pin.mode(PinDirection::Input);
        Ok(InputPin(ptr_pin))
    }

    pub fn output_pin(&self, pin: pins::Pin) -> Result<OutputPin, OdroidC2GPIOError> {
        let mut ptr_pin = UnsafePtrPin::new(pin, &self.memory)?;
        ptr_pin.mode(PinDirection::Output);
        Ok(OutputPin(ptr_pin))
    }

    /// Creates a memory representation of one physical pin of your device.
    ///
    /// It can fail if you provide an invalid value for a GPIO pin.
    /// Basically every value of `pins::Pin` struct can be safely passed to this method, but it can fail
    /// if you transmute an arbitrary `u8` value and try to interpret it as a GPIO pin using unsafe methods.
    pub fn pin(pin: pins::Pin) -> Result<MemoryPin, OdroidC2GPIOError> {
        MemoryPin::new(pin)
    }

    /// Reads a value of pin set to `Input` direction.
    ///
    /// The value of this call when pin is in an Output mode is undefined and should not be relied on.
    pub fn read(&self, pin: &MemoryPin) -> PinValue {
        pin.read(&self.memory)
    }

    /// Sets a direction for a pin.
    ///
    /// Setting direction to `Input` also disables pull-up resistor on this particular pin.
    pub fn direction(&mut self, pin: &MemoryPin, direction: PinDirection) {
        pin.mode(&mut self.memory, direction);
    }

    /// Writes a value to a pin set to `Output` direction.
    ///
    /// The behaviour of this call when pin is in an Input mode is undefined.
    pub fn write(&mut self, pin: &MemoryPin, value: PinValue) {
        pin.write(&mut self.memory, value);
    }
}
