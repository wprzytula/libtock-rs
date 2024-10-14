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
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

set_main! {main}
stack_size! {0xA00}

/// Number of frames that can fit the userspace frame buffer.
const BUF_SIZE: usize = 2;

/// Interval between frame transmissions.
const N_SECS: u32 = 5;
// const SLEEP_TIME: Milliseconds = Milliseconds(1000 * N_SECS);

fn get_cherry_id() -> Option<&'static str> {
    option_env!("HENI_DEVICE")
}

#[repr(C)]
#[derive(FromBytes, IntoBytes, Immutable, KnownLayout)]
struct TemperatureMeasurementMsg {
    cherry_mote_id: u32,
    sequential_no: u32,
    timestamp: u32,
    temperature_centigrades_celsius: i32,
}

impl TemperatureMeasurementMsg {
    fn print(&self) {
        let &TemperatureMeasurementMsg {
            cherry_mote_id,
            sequential_no,
            timestamp,
            temperature_centigrades_celsius,
        } = self;

        writeln!(
            ConsoleLite::writer(),
            "Timestamp {}, CherryMote {}, measurement no. {}, temperature {}.",
            timestamp,
            cherry_mote_id,
            sequential_no,
            TemperatureDisplay(temperature_centigrades_celsius)
        )
        .unwrap()
    }
}

struct TemperatureDisplay(i32);

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
    let cherry_mote_id = get_cherry_id().map(str::parse::<u32>).unwrap().unwrap();

    configure_radio();

    // Turn the radio on
    Ieee802154::radio_on().unwrap();
    assert!(Ieee802154::is_on());

    let mut broadcast_temperature_measurement = {
        const MSG_LEN: usize = core::mem::size_of::<TemperatureMeasurementMsg>();
        let mut msg_buf = [0_u8; MSG_LEN];
        let mut sequence_no = 0_u32;

        move || {
            // Measure temperature.
            let temperature_centigrades_celsius = Temperature::read_temperature_sync().unwrap();

            // Get current time.
            let timestamp = Alarm::get_ticks().unwrap();

            // Fill the buffer with current data.
            let msg = TemperatureMeasurementMsg {
                cherry_mote_id,
                sequential_no: sequence_no,
                timestamp,
                temperature_centigrades_celsius,
            };
            msg.write_to(&mut msg_buf).unwrap();

            // Transmit a frame
            Ieee802154::transmit_frame(&msg_buf).unwrap();

            writeln!(
                ConsoleLite::writer(),
                "Transmitted frame {} of len {}!\n",
                sequence_no,
                MSG_LEN,
            )
            .unwrap();

            sequence_no += 1;
        }
    };

    let sleep_len = Milliseconds(N_SECS * 1000 + cherry_mote_id);

    let mut frames_buf1 = RxRingBuffer::<{ BUF_SIZE + 1 }>::new();
    let mut frames_buf2 = RxRingBuffer::<{ BUF_SIZE + 1 }>::new();
    let mut operator =
        RxBufferAlternatingOperator::new(&mut frames_buf1, &mut frames_buf2).unwrap();

    let mut rx_callback = |frame_res: Result<&mut Frame, ErrorCode>| {
        let frame = frame_res.unwrap();

        let body_len = frame.payload_len as usize;
        let raw_body = &frame.body[..body_len];

        let msg_res = TemperatureMeasurementMsg::read_from_bytes(raw_body);
        match msg_res {
            Ok(msg) => {
                msg.print();
            }
            Err(err) => {
                writeln!(
                    ConsoleLite::writer(),
                    "Failed to parse frame as TemperatureMeasurementMsg: {err}\n"
                )
                .unwrap();
                ConsoleLite::write(
                    b"Received frame does not fit TemperatureMeasurementMsg! Parsing as str...\n",
                )
                .unwrap();

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
            }
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
