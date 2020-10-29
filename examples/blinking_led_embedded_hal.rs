//! This example demonstrates the usage of embedded_hal trait usage.
//!
//! The main benefit over the blinking_led example is that `blink_led`
//! can be used for _any_ device with embedded-hal digital pins abstraction.
//!
//! This example assumes that physical pin #7 is connected to diode's anode (+).
//! Make sure to put resistor to reduce current flowing through the diode.

use c2_mmap_gpio::{Device, PinId};
use embedded_hal::digital::v2::*;
use std::error::Error;
use std::thread::sleep;
use std::time::Duration;

fn blink_led<T: OutputPin<Error = impl Error + 'static>>(mut pin: T) -> Result<(), Box<dyn Error>> {
    let blink_interval = Duration::from_millis(500);

    loop {
        // In our case these operations cannot fail.
        // However, if we want to keep `blink_led` generic we need to anticipate errors here.
        pin.set_high()?;
        sleep(blink_interval);
        pin.set_low()?;
        sleep(blink_interval);
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut odroid = Device::new()?;
    let led_pin = odroid.output_pin(PinId::Phy7)?;

    blink_led(led_pin)?;
    Ok(())
}
