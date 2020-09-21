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

#[derive(Error, Debug)]
pub enum OdroidC2GPIOError {
    #[error("failed to open descriptor")]
    OpenFdFailed(#[source] nix::Error),
    #[error("failed to open device file")]
    OpenFailed(#[source] io::Error),
    #[error("failed to map device memory")]
    MapError(#[source] io::Error),
    #[error("pin not in any range known by the library")]
    PinError,
}

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
    DVRange,
    YRange,
    XRange,
}

impl PinRegisters {
    const PIN_BASE: u8 = 136;
    const DV_PINS_RANGE: RangeInclusive<u8> = ((Self::PIN_BASE + 45)..=(Self::PIN_BASE + 74));
    const Y_PINS_RANGE: RangeInclusive<u8> = ((Self::PIN_BASE + 75)..=(Self::PIN_BASE + 91));
    const X_PINS_RANGE: RangeInclusive<u8> = ((Self::PIN_BASE + 92)..=(Self::PIN_BASE + 114));

    pub fn new(pin: pins::Pin) -> Option<Self> {
        let pin_u8 = pin as u8;

        if Self::DV_PINS_RANGE.contains(&pin_u8) {
            Some(PinRangeType::DVRange)
        } else if Self::Y_PINS_RANGE.contains(&pin_u8) {
            Some(PinRangeType::YRange)
        } else if Self::X_PINS_RANGE.contains(&pin_u8) {
            Some(PinRangeType::XRange)
        } else {
            None
        }
        .map(|range_type| Self { range_type, pin_u8 })
    }

    fn range(&self) -> &'static RangeInclusive<u8> {
        use PinRangeType::*;
        match self.range_type {
            DVRange => &Self::DV_PINS_RANGE,
            YRange => &Self::Y_PINS_RANGE,
            XRange => &Self::X_PINS_RANGE,
        }
    }

    pub fn gplev(&self) -> usize {
        use PinRangeType::*;
        (match self.range_type {
            DVRange => 0x10E,
            YRange => 0x111,
            XRange => 0x11A,
        } * size_of::<u32>())
    }

    pub fn gpset(&self) -> usize {
        use PinRangeType::*;
        (match self.range_type {
            DVRange => 0x10D,
            YRange => 0x110,
            XRange => 0x119,
        } * size_of::<u32>())
    }

    pub fn gpfsel(&self) -> usize {
        use PinRangeType::*;
        (match self.range_type {
            DVRange => 0x10C,
            YRange => 0x10F,
            XRange => 0x118,
        } * size_of::<u32>())
    }

    pub fn puen(&self) -> usize {
        use PinRangeType::*;
        (match self.range_type {
            DVRange => 0x13A,
            YRange => 0x149,
            XRange => 0x14C,
        } * size_of::<u32>())
    }

    pub fn pupd(&self) -> usize {
        use PinRangeType::*;
        (match self.range_type {
            DVRange => 0x148,
            YRange => 0x13B,
            XRange => 0x13E,
        } * size_of::<u32>())
    }

    pub fn pin_bitmap_offset(&self) -> u8 {
        self.pin_u8 - self.range().start()
    }
}

#[derive(Debug)]
pub struct MemoryPin {
    pin: pins::Pin,
    registers: PinRegisters,
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
    pub fn new(map: MmapMut) -> Self {
        Self { map }
    }
}

pub struct OdroidC2GPIO {
    _file_handle: File,
    memory: Memory,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum PinValue {
    High = 0,
    Low = 1,
}

impl OdroidC2GPIO {
    const GPIO_BASE_ADDR: u64 = 0xC8834000;
    const BLOCK_SIZE: usize = 4096;

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

    pub fn pin(pin: pins::Pin) -> Result<MemoryPin, OdroidC2GPIOError> {
        MemoryPin::new(pin)
    }

    pub fn read(&self, pin: &MemoryPin) -> PinValue {
        pin.read(&self.memory)
    }

    pub fn direction(&mut self, pin: &MemoryPin, direction: PinDirection) {
        pin.mode(&mut self.memory, direction);
    }

    pub fn write(&mut self, pin: &MemoryPin, value: PinValue) {
        pin.write(&mut self.memory, value);
    }
}
