//! An extremely simple libtock-rs example. Just prints out a message
//! using the ConsoleLite capsule, then terminates.

#![no_main]
#![no_std]
use core::fmt::Write as _;
use libtock::alarm::Alarm;
use libtock::console_lite::ConsoleLite;
use libtock::runtime::{set_main, stack_size};
use libtock_alarm::Milliseconds;

set_main! {main}
stack_size! {0x400}

fn main() {
    writeln!(ConsoleLite::writer(), "Hello ConsoleLite World!").unwrap();

    let mut buf = [b'?'; 0x10];
    let mut data = (b'a'..=b'z').cycle();
    loop {
        Alarm::sleep_for(Milliseconds(2000)).unwrap();
        for (idx, data) in data.by_ref().take(buf.len() - 1).enumerate() {
            buf[idx] = data;
            *buf.last_mut().unwrap() = b'\n';
        }
        ConsoleLite::write(&buf[..]).unwrap();
    }
}
