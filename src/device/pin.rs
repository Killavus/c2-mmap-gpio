use super::error::PinError;
use crate::{device::Memory, OdroidResult};
use crate::{pin_map, OdroidC2Error};
use byteorder::{ByteOrder, NativeEndian};
use derive_try_from_primitive::TryFromPrimitive;
use embedded_hal::digital::v2 as eh;
use std::convert::Infallible;
use std::mem::size_of;
use std::ops::RangeInclusive;
use std::ptr::{self, NonNull};

#[derive(Copy, Clone, Debug)]
pub struct RegistersOffsets {
    pin_id: pin_map::PinId,
    range_type: RegistersRangeType,
}

#[derive(Copy, Clone, Debug)]
enum RegistersRangeType {
    DV,
    Y,
    X,
}

/// Enum representing the state of a given pin.
///
/// This usually correlates to electric low/high state of voltage for GPIO pins,
/// but keep in mind that this can be changed by memory pull-up/pull-down resistor registers.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive)]
pub enum Value {
    High = 1,
    Low = 0,
}

#[derive(Copy, Clone, Debug)]
pub enum Direction {
    Input,
    Output,
}

impl RegistersOffsets {
    /* In ODROID-C2 GPIO registers are mapped in three different parts of the contigous memory (DV / Y / X ranges).
     * There is a direct mapping between GPIO internal identifiers and these memory parts.
     * This struct handles the calculation of all required offsets needed to access these registers for a given pin id.
     *
     * This is mostly to simplify calculations where actually using pins - range computations are performed and stored
     * when initializing this struct.
     */
    const PIN_BASE: u8 = 136;
    const DV_PINS_RANGE: RangeInclusive<u8> = ((Self::PIN_BASE + 45)..=(Self::PIN_BASE + 74));
    const Y_PINS_RANGE: RangeInclusive<u8> = ((Self::PIN_BASE + 75)..=(Self::PIN_BASE + 91));
    const X_PINS_RANGE: RangeInclusive<u8> = ((Self::PIN_BASE + 92)..=(Self::PIN_BASE + 114));

    pub fn id(&self) -> pin_map::PinId {
        self.pin_id
    }

    pub fn new(pin_id: pin_map::PinId) -> Option<Self> {
        use RegistersRangeType::*;
        let pin_u8 = pin_id as u8;

        if Self::DV_PINS_RANGE.contains(&pin_u8) {
            Some(DV)
        } else if Self::Y_PINS_RANGE.contains(&pin_u8) {
            Some(Y)
        } else if Self::X_PINS_RANGE.contains(&pin_u8) {
            Some(X)
        } else {
            None
        }
        .map(|range_type| Self { range_type, pin_id })
    }

    pub fn gplev(&self) -> usize {
        use RegistersRangeType::*;
        (match self.range_type {
            DV => 0x10E,
            Y => 0x111,
            X => 0x11A,
        } * size_of::<u32>())
    }

    pub fn gpset(&self) -> usize {
        use RegistersRangeType::*;
        (match self.range_type {
            DV => 0x10D,
            Y => 0x110,
            X => 0x119,
        } * size_of::<u32>())
    }

    pub fn gpfsel(&self) -> usize {
        use RegistersRangeType::*;
        (match self.range_type {
            DV => 0x10C,
            Y => 0x10F,
            X => 0x118,
        } * size_of::<u32>())
    }

    pub fn puen(&self) -> usize {
        use RegistersRangeType::*;
        (match self.range_type {
            DV => 0x13A,
            Y => 0x149,
            X => 0x14C,
        } * size_of::<u32>())
    }

    /* This is right now not used.
    pub fn pupd(&self) -> usize {
        use RegistersRangeType::*;
        (match self.range_type {
            DV => 0x148,
            Y => 0x13B,
            X => 0x13E,
        } * size_of::<u32>())
    }
    */

    pub fn pin_bitmap_offset(&self) -> u8 {
        self.pin_id as u8 - self.range().start()
    }

    fn range(&self) -> &'static RangeInclusive<u8> {
        use RegistersRangeType::*;
        match self.range_type {
            DV => &Self::DV_PINS_RANGE,
            Y => &Self::Y_PINS_RANGE,
            X => &Self::X_PINS_RANGE,
        }
    }
}

/// Foundational block of pin abstractions provided by this crate.
///
/// This foundational block is *very* unsafe. It uses direct pointer manipulation to achieve highly performant access to registers
/// and violates Rust shared/unique references rules.
///
/// It is up to clients of this code to maintain these invariants. This abstraction is also not thread-safe - you can expect undefined results when
/// write operations are performed by multiple threads at once.
#[derive(Copy, Clone, Debug)]
pub struct UnsafePointerPin<'memory> {
    memory: &'memory Memory,
    registers: RegistersOffsets,
}

impl<'memory> UnsafePointerPin<'memory> {
    pub fn new(pin_id: pin_map::PinId, memory: &'memory Memory) -> OdroidResult<Self> {
        use PinError::*;

        Ok(Self {
            registers: RegistersOffsets::new(pin_id)
                .ok_or(OdroidC2Error::PinError(WrongPinId(pin_id as u8)))?,
            memory,
        })
    }

    pub fn direction(&mut self, direction: Direction) {
        use Direction::*;

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

    pub fn write(&mut self, value: Value) {
        use Value::*;

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

    pub fn read(&self) -> Value {
        use Value::*;
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

/// Abstraction over GPIO pin set to input (reading) direction.
///
/// This is obtainable by using `input_pin` method of `Device` struct.
/// You can have multiple leases of input pin in your program as long as you don't keep an output pin lease for the same PinId.
/// In such case `InputPin` abstraction can't be instantiated.
#[derive(Clone, Debug)]
pub struct InputPin<'memory>(pub UnsafePointerPin<'memory>);

/// Abstraction over GPIO pin set to output (writing) direction.
///
/// This is obtainable by using `input_pin` method of `Device` struct.
/// You can have multiple leases of output pin in your program as long as you don't keep an input pin lease for the same PinId.
/// In such case `OutputPin` abstraction can't be instantiated.
#[derive(Clone, Debug)]
pub struct OutputPin<'memory>(pub UnsafePointerPin<'memory>);

impl<'memory> Drop for OutputPin<'memory> {
    fn drop(&mut self) {
        self.0
            .memory
            .release_output(self.0.registers.id())
            .expect("failed to release output pin");
    }
}

impl<'memory> Drop for InputPin<'memory> {
    fn drop(&mut self) {
        self.0
            .memory
            .release_input(self.0.registers.id())
            .expect("failed to release input pin");
    }
}

impl<'memory> InputPin<'memory> {
    pub fn into_output(self) -> OdroidResult<OutputPin<'memory>> {
        let mut internal = self.0;
        std::mem::forget(self);
        internal.memory.release_input(internal.registers.id())?;
        internal.memory.lease_output(internal.registers.id())?;

        internal.direction(Direction::Output);
        Ok(OutputPin(internal))
    }

    pub fn get_value(&self) -> Value {
        self.0.read()
    }
}

impl<'memory> OutputPin<'memory> {
    pub fn into_input(self) -> OdroidResult<InputPin<'memory>> {
        let mut internal = self.0;
        std::mem::forget(self);
        internal.memory.release_output(internal.registers.id())?;
        internal.memory.lease_input(internal.registers.id())?;

        internal.direction(Direction::Input);
        Ok(InputPin(internal))
    }

    pub fn set_value(&mut self, value: Value) {
        self.0.write(value);
    }
}

impl<'memory> eh::InputPin for InputPin<'memory> {
    type Error = Infallible;

    fn is_high(&self) -> Result<bool, Self::Error> {
        use Value::*;

        Ok(match self.get_value() {
            High => true,
            Low => false,
        })
    }

    fn is_low(&self) -> Result<bool, Self::Error> {
        self.is_high().map(|v| !v)
    }
}

impl<'memory> eh::OutputPin for OutputPin<'memory> {
    type Error = Infallible;

    fn set_low(&mut self) -> Result<(), Self::Error> {
        self.set_value(Value::Low);
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Self::Error> {
        self.set_value(Value::High);
        Ok(())
    }
}
