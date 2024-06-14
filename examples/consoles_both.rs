//! An example showcasing using both Console and ConsoleLite.
//! Reads some bytes from UART-full, then writes them to UART-lite
//! and writes some next letters from the alphabet to UART-full.

#![no_main]
#![no_std]
use core::fmt::Write;
use libtock::alarm::Alarm;
use libtock::console::Console;
use libtock::console_lite::ConsoleLite;
use libtock::runtime::{set_main, stack_size};
use libtock_alarm::Milliseconds;

set_main! {main}
stack_size! {0x400}

fn main() {
    writeln!(ConsoleLite::writer(), "Hello ConsoleLite World!").unwrap();
    writeln!(Console::writer(), "Hello Console World!").unwrap();

    let mut buf = [0u8; 0x20];
    const PROMPT: &str = "\r\nYour input: ";
    buf[..PROMPT.len()].copy_from_slice(PROMPT.as_bytes());

    let mut buf_lite = [0u8; 0x10];
    let mut data_lite = (b'a'..=b'z').cycle();

    loop {
        // Read from UART-full, write the same to UART-lite.
        {
            let (read, result) = Console::read(&mut buf[PROMPT.len()..]);
            result.unwrap();
            // Alarm::sleep_for(Milliseconds(2000)).unwrap();
            ConsoleLite::write(&buf[..PROMPT.len() + read]).unwrap();
        }

        // Write the next part of the alphabet cycle to UART-full.
        {
            for (idx, data) in data_lite.by_ref().take(buf_lite.len() - 1).enumerate() {
                buf_lite[idx] = data;
                *buf_lite.last_mut().unwrap() = b'\n';
            }
            Console::write(&buf_lite[..]).unwrap();
        }
    }
}
