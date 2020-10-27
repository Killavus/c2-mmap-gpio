# ODROID-C2 GPIO bindings

This project provides a way to interact with [ODROID-C2](https://www.hardkernel.com/shop/odroid-c2/) GPIO pins directly through [memory mapped](https://en.wikipedia.org/wiki/Mmap) GPIO registers.

## Requirements

* ODROID-C2 SBC, revision 2. Currently values for GPIO pins are hardcoded in the library so rev1 aren't going to work.
* Linux OS on ODROID-C2. This library has been tested on [Ubuntu 18.04 LTS](https://www.hardkernel.com/blog-2/ubuntu-18-04-for-odroid-c2/).

## Rationale

This library provides a direct access to GPIO pins - bypassing [sysfs](https://en.wikipedia.org/wiki/Sysfs) layer (which is used by [sysfs-gpio](https://github.com/rust-embedded/rust-sysfs-gpio) crate, for example) to make accesses faster.
I needed this capability while interacting with [DHT11](https://www.mouser.com/datasheet/2/758/DHT11-Technical-Data-Sheet-Translated-Version-1143054.pdf) thermal sensor because it was hard to implement its protocol with speeds provided by `sysfs-gpio`.
This is propably not because `sysfs` by itself is slow - but `sysfs-gpio` crate was reopening sysfs resources every call which unfortunately took too long.

Core idea of this library and implementation is basically a rewrite in Rust of [wiringPi](https://wiki.odroid.com/odroid-c2/application_note/gpio/wiringpi) library forked for this device.

## What's missing?

* Tests. I have little to none experience with embedded systems so I need to figure out how to do it. Contributions are welcome!

## License

See [LICENSE](./LICENSE) file.
