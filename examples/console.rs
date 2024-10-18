//! An extremely simple libtock-rs example. Just prints out a message
//! using the Console capsule, then terminates.

#![no_main]
#![no_std]
use core::fmt::Write;
use libtock::alarm::Alarm;
use libtock::console::Console;
use libtock::runtime::{set_main, stack_size};
use libtock_alarm::Milliseconds;

set_main! {main}
stack_size! {0x400}

fn main() {
    writeln!(Console::writer(), "Hello Console World!").unwrap();

    let mut buf = [0u8; 0x20];
    const PROMPT: &str = "\r\nYour input: ";
    buf[..PROMPT.len()].copy_from_slice(PROMPT.as_bytes());

    loop {
        let (read, result) = Console::read(&mut buf[PROMPT.len()..]);
        result.unwrap();
        Alarm::sleep_for(Milliseconds(2000)).unwrap();
        Console::write(&buf[..PROMPT.len() + read]).unwrap();
    }
}
