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
use libtock::console::Console as ConsoleFull;
use libtock::console_lite::ConsoleLite;
use libtock::ieee802154::{Ieee802154, RxBufferAlternatingOperator, RxOperator as _, RxRingBuffer};
use libtock::runtime::{set_main, stack_size};
use libtock::temperature::Temperature;
use libtock_ieee802154::Frame;
use libtock_platform::ErrorCode;

set_main! {main}
stack_size! {0xA00}

/// Number of frames that can fit the userspace frame buffer.
const BUF_SIZE: usize = 2;

/// Interval between frame transmissions.
const N_SECS: u32 = 3;
// const SLEEP_TIME: Milliseconds = Milliseconds(1000 * N_SECS);

fn get_cherry_id() -> Option<&'static str> {
    option_env!("HENI_DEVICE")
}

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
            timestamp,
            get_cherry_id().unwrap_or("<unknown id>"),
            sequence_no,
            temperature_displayer
        )
        .unwrap();

        self.offset
    }

    fn inner(&self) -> &[u8] {
        &self.buf
    }
}

fn configure_radio() {
    // Configure the radio
    const PAN: u16 = 0xcafe;
    const ADDR_SHORT: u16 = 0xdead;
    const ADDR_LONG: u64 = 0xdead_bead;
    const TX_POWER: i8 = 5;
    const CHANNEL: u8 = 11;

    Ieee802154::set_pan(PAN);
    Ieee802154::set_address_short(ADDR_SHORT);
    Ieee802154::set_address_long(ADDR_LONG);
    Ieee802154::set_tx_power(TX_POWER).unwrap();
    Ieee802154::set_channel(CHANNEL).unwrap();

    // Don't forget to commit the config!
    Ieee802154::commit_config();
}

fn main() {
    configure_radio();

    // Turn the radio on
    Ieee802154::radio_on().unwrap();
    assert!(Ieee802154::is_on());

    let mut broadcast_temperature_measurement = {
        let mut sequence_no = 0_usize;
        let mut msg_buf = MsgBuf::<MSG_BUF_LEN>::new();

        move || {
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
                "Transmitted frame {} of len {}!\n",
                sequence_no,
                msg_len,
            )
            .unwrap();

            sequence_no += 1;
        }
    };

    let cherry_id = get_cherry_id()
        .map(str::parse::<u32>)
        .transpose()
        .unwrap()
        .unwrap_or(0);
    let sleep_len = Milliseconds(N_SECS * 1000 + cherry_id);

    let mut frames_buf1 = RxRingBuffer::<{ BUF_SIZE + 1 }>::new();
    let mut frames_buf2 = RxRingBuffer::<{ BUF_SIZE + 1 }>::new();
    let mut operator =
        RxBufferAlternatingOperator::new(&mut frames_buf1, &mut frames_buf2).unwrap();

    let mut rx_callback = |frame_res: Result<&mut Frame, ErrorCode>| {
        let frame = frame_res.unwrap();

        struct Ascii<'a>(&'a [u8]);
        impl Display for Ascii<'_> {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                for b in self.0.iter().copied() {
                    let c = char::from_u32(b as u32).unwrap_or('*');
                    f.write_char(c)?;
                }
                Ok(())
            }
        }

        let body_len = frame.payload_len as usize;
        let raw_body = &frame.body[..body_len];
        let decoded_frame = core::str::from_utf8(raw_body);
        match decoded_frame {
            Ok(body) => {
                writeln!(
                    ConsoleLite::writer(),
                    "Received frame (len={}):\n{}\n\n",
                    body_len,
                    body
                ).unwrap()
            }
            Err(err) => writeln!(
                ConsoleLite::writer(),
                "Received frame (len={}):\n<error decoding> {}, parsed part: {}\n, remaining raw body:\n{:?}\n",
                body_len,
                err,
                Ascii(raw_body),
                &raw_body[err.valid_up_to()..],
            )
            .unwrap(),
        }
    };

    let mut read_callback = |len: usize, buf_res: Result<&mut [u8], ErrorCode>| {
        if let Ok(buf) = buf_res {
            match len {
                0 => unreachable!("Empty read!"),
                1 => {
                    match buf[0] {
                        b't' => {
                            broadcast_temperature_measurement();
                            true
                        }
                        _ => {
                            // terminate
                            false
                        }
                    }
                }
                _ => unreachable!("Read bigger than buf len!"),
            }
        } else {
            false
        }
    };

    operator
        .rx_scope(&mut rx_callback, || {
            let mut uart_full_buf = [0u8; 1];
            ConsoleFull::read_scope(&mut uart_full_buf, &mut read_callback, || {
                loop {
                    ConsoleFull::write(b"Press 't' for temperature read.\n").unwrap();

                    // Sleep for a predefined period of time, so that each mote sends its message.
                    Alarm::sleep_for(sleep_len).unwrap();
                }
            })
            .unwrap();
        })
        .unwrap();
}
