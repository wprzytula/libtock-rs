#![no_std]

use core::cell::Cell;
use core::fmt;
use core::marker::PhantomData;
use libtock_platform as platform;
use libtock_platform::allow_ro::AllowRo;
use libtock_platform::share;
use libtock_platform::subscribe::Subscribe;
use libtock_platform::{DefaultConfig, ErrorCode, Syscalls};

/// The lite console driver.
///
/// It allows libraries to pass strings to the kernel's console driver.
///
/// # Example
/// ```ignore
/// use libtock::ConsoleLite;
///
/// // Writes "foo", followed by a newline, to the console
/// let mut writer = ConsoleLite::writer();
/// writeln!(writer, foo).unwrap();
/// ```
pub struct ConsoleLite<S: Syscalls, C: Config = DefaultConfig>(S, C);

impl<S: Syscalls, C: Config> ConsoleLite<S, C> {
    /// Run a check against the console capsule to ensure it is present.
    ///
    /// Returns `true` if the driver was present. This does not necessarily mean
    /// that the driver is working, as it may still fail to allocate grant
    /// memory.
    #[inline(always)]
    pub fn exists() -> bool {
        S::command(DRIVER_NUM, command::EXISTS, 0, 0).is_success()
    }

    /// Writes bytes.
    /// This is an alternative to `fmt::Write::write`
    /// because this can actually return an error code.
    pub fn write(s: &[u8]) -> Result<(), ErrorCode> {
        let called: Cell<Option<(u32,)>> = Cell::new(None);
        share::scope::<
            (
                AllowRo<_, DRIVER_NUM, { allow_ro::WRITE }>,
                Subscribe<_, DRIVER_NUM, { subscribe::WRITE }>,
            ),
            _,
            _,
        >(|handle| {
            let (allow_ro, subscribe) = handle.split();

            S::allow_ro::<C, DRIVER_NUM, { allow_ro::WRITE }>(allow_ro, s)?;

            S::subscribe::<_, _, C, DRIVER_NUM, { subscribe::WRITE }>(subscribe, &called)?;

            S::command(DRIVER_NUM, command::WRITE, s.len() as u32, 0).to_result()?;

            loop {
                S::yield_wait();
                if let Some((_,)) = called.get() {
                    return Ok(());
                }
            }
        })
    }

    pub fn writer() -> ConsoleLiteWriter<S> {
        ConsoleLiteWriter {
            syscalls: Default::default(),
        }
    }
}

pub struct ConsoleLiteWriter<S: Syscalls> {
    syscalls: PhantomData<S>,
}

impl<S: Syscalls> fmt::Write for ConsoleLiteWriter<S> {
    fn write_str(&mut self, s: &str) -> Result<(), fmt::Error> {
        ConsoleLite::<S>::write(s.as_bytes()).map_err(|_e| fmt::Error)
    }
}

/// System call configuration trait for `ConsoleLite`.
pub trait Config:
    platform::allow_ro::Config + platform::allow_rw::Config + platform::subscribe::Config
{
}
impl<T: platform::allow_ro::Config + platform::allow_rw::Config + platform::subscribe::Config>
    Config for T
{
}

#[cfg(test)]
mod tests;

// -----------------------------------------------------------------------------
// Driver number and command IDs
// -----------------------------------------------------------------------------

const DRIVER_NUM: u32 = 2137;

// Command IDs
#[allow(unused)]
mod command {
    pub const EXISTS: u32 = 0;
    pub const WRITE: u32 = 1;
}

#[allow(unused)]
mod subscribe {
    pub const WRITE: u32 = 1;
}

mod allow_ro {
    pub const WRITE: u32 = 1;
}
