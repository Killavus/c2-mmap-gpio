//! A very basic example of a program blinking a LED diode using native library API.
//!
//! This example assumes that physical pin #7 is connected to diode's anode (+).
//! Make sure to put resistor to reduce current flowing through the diode.

use c2_mmap_gpio::{Device, PinId, Value};
use std::error::Error;
use std::thread::sleep;
use std::time::Duration;

fn main() -> Result<(), Box<dyn Error>> {
    let mut odroid = Device::new()?;
    let mut led_pin = odroid.output_pin(PinId::Phy7)?;
    let blink_interval = Duration::from_millis(500);

    loop {
        led_pin.set_value(Value::High);
        sleep(blink_interval);
        led_pin.set_value(Value::Low);
        sleep(blink_interval);
    }
}
