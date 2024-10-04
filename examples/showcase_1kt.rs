//! An example showing Tock's capabilitities ported to CherryMotes, on 1KT.
//! Every T seconds (a configurable constant) it infinitely sends a frame
//! with a tuple (timestamp, CherryMote id, sequence number, measured temperature)
//! and after each send receives N - 1 (a configurable constant) frames from other CherryMotes
//! and prints them to console.

#![no_main]
#![no_std]
#![forbid(unsafe_code)]

use core::fmt::{Display, Write as _};
use libtock::alarm::{Alarm, Milliseconds};
// use libtock::console::Console;
use libtock::console_lite::ConsoleLite;
use libtock::ieee802154::{Ieee802154, RxOperator as _, RxRingBuffer, RxSingleBufferOperator};
use libtock::runtime::{set_main, stack_size};
use libtock::temperature::Temperature;

set_main! {main}
stack_size! {0xA00}

/// Number of CherryMotes executing this app at the same time.
const N_MOTES: usize = 2;

/// Interval between frame transmissions.
const N_SECS: u32 = 3;
const SLEEP_TIME: Milliseconds = Milliseconds(1000 * N_SECS);

const CHERRY_MOTE_ID: u32 = 116;

struct TemperatureDisplay(i32);

impl TemperatureDisplay {
    const MAX_DISPLAYED_BYTES_LEN: usize =
        1 /* sign */ +
        4 /* +-1000*C is for sure a bound for measured temperature */ +
        1 /* decimal dot */ +
        2 /* decimal points */ +
        2 /* *C */;
}

impl Display for TemperatureDisplay {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{}{}.{}*C",
            if self.0 < 0 { "-" } else { "" },
            i32::abs(self.0) / 100,
            i32::abs(self.0) % 100
        )
    }
}

const U32_MAX_DISPLAY_LEN: usize = 10;

macro_rules! msg_template {
    () => {
        "Timestamp {}, CherryMote {}, measurement no. {}, temperature {}."
    };
}
const MSG_TEMPLATE_REAL_LEN: usize = msg_template!().len() - 4 /* number of substitutions */ * 2 /* pair of braces */;

const MSG_BUF_LEN: usize = MSG_TEMPLATE_REAL_LEN
    + U32_MAX_DISPLAY_LEN /* timestamp */
    + U32_MAX_DISPLAY_LEN /* CherryMote id */
    + TemperatureDisplay::MAX_DISPLAYED_BYTES_LEN;

struct MsgBuf<const N: usize> {
    buf: [u8; N],
    offset: usize,
}
impl<const N: usize> MsgBuf<N> {
    fn new() -> Self {
        Self {
            buf: [0_u8; N],
            offset: 0,
        }
    }

    fn fill(
        &mut self,
        timestamp: u32,
        sequence_no: usize,
        temperature_centigrades_celcius: i32,
    ) -> usize {
        let temperature_displayer = TemperatureDisplay(temperature_centigrades_celcius);

        // Each filling start overrides the buffer.
        self.offset = 0;

        struct Writer<'a> {
            buf: &'a mut [u8],
            offset: &'a mut usize,
        }
        impl core::fmt::Write for Writer<'_> {
            fn write_str(&mut self, s: &str) -> core::fmt::Result {
                let prefix = &mut self.buf[*self.offset..*self.offset + s.len()];
                prefix.copy_from_slice(s.as_bytes());
                *self.offset += s.len();
                Ok(())
            }
        }

        let mut w = Writer {
            buf: &mut self.buf,
            offset: &mut self.offset,
        };

        writeln!(
            w,
            msg_template!(),
            timestamp, CHERRY_MOTE_ID, sequence_no, temperature_displayer
        )
        .unwrap();

        self.offset
    }

    fn inner(&self) -> &[u8] {
        &self.buf
    }
}

fn main() {
    // Configure the radio
    let pan: u16 = 0xcafe;
    let addr_short: u16 = 0xdead;
    let addr_long: u64 = 0xdead_bead;
    let tx_power: i8 = 5;
    let channel: u8 = 11;

    Ieee802154::set_pan(pan);
    Ieee802154::set_address_short(addr_short);
    Ieee802154::set_address_long(addr_long);
    Ieee802154::set_tx_power(tx_power).unwrap();
    Ieee802154::set_channel(channel).unwrap();

    // Don't forget to commit the config!
    Ieee802154::commit_config();

    // Turn the radio on
    Ieee802154::radio_on().unwrap();
    assert!(Ieee802154::is_on());

    Alarm::sleep_for(Milliseconds(5 * 1000)).unwrap();

    let mut frames_buf = RxRingBuffer::<N_MOTES>::new();
    let mut operator = RxSingleBufferOperator::new(&mut frames_buf);

    let mut sequence_no = 0_usize;
    let mut msg_buf = MsgBuf::<MSG_BUF_LEN>::new();

    loop {
        // Measure temperature.
        let temperature_centigrades_celcius = Temperature::read_temperature_sync().unwrap();

        // Get current time.
        let timestamp = Alarm::get_ticks().unwrap();

        // Fill the buffer with current data.
        let msg_len = msg_buf.fill(timestamp, sequence_no, temperature_centigrades_celcius);

        // Transmit a frame
        Ieee802154::transmit_frame(&msg_buf.inner()[..msg_len]).unwrap();

        writeln!(
            ConsoleLite::writer(),
            "Transmitted frame {}!\n",
            sequence_no
        )
        .unwrap();

        // Sleep for a predefined period of time, so that each mote sends its message.
        Alarm::sleep_for(SLEEP_TIME).unwrap();

        // For each peer...
        for _ in 0..N_MOTES - 1 {
            // Receive a frame from it.
            let frame = operator.receive_frame().unwrap();

            let body_len = frame.payload_len;
            writeln!(
                ConsoleLite::writer(),
                "Received frame with body of len {}: {}!\n",
                body_len,
                core::str::from_utf8(
                    &frame.body[..frame.body.len() - core::mem::size_of::<usize>()]
                )
                .unwrap_or("<error decoding>"),
            )
            .unwrap();
        }

        sequence_no += 1;
    }
}
