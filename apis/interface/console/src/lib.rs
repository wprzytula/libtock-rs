#![no_std]

use core::cell::Cell;
use core::fmt;
use core::marker::PhantomData;
use libtock_platform::allow_ro::AllowRo;
use libtock_platform::allow_rw::AllowRw;
use libtock_platform::share;
use libtock_platform::subscribe::{OneId, Subscribe};
use libtock_platform::{self as platform, Upcall};
use libtock_platform::{DefaultConfig, ErrorCode, Syscalls};
use tock_cells::map_cell::MapCell;

/// The console driver.
///
/// It allows libraries to pass strings to the kernel's console driver.
///
/// # Example
/// ```ignore
/// use libtock::Console;
///
/// // Writes "foo", followed by a newline, to the console
/// let mut writer = Console::writer();
/// writeln!(writer, foo).unwrap();
/// ```
pub struct Console<S: Syscalls, C: Config = DefaultConfig>(S, C);

impl<S: Syscalls, C: Config> Console<S, C> {
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

    /// Reads bytes
    /// Reads from the device and writes to `buf`, starting from index 0.
    /// No special guarantees about when the read stops.
    /// Returns count of bytes written to `buf`.
    pub fn read(buf: &mut [u8]) -> (usize, Result<(), ErrorCode>) {
        let called: Cell<Option<(u32, u32)>> = Cell::new(None);
        let mut bytes_received = 0;
        let r = share::scope::<
            (
                AllowRw<_, DRIVER_NUM, { allow_rw::READ }>,
                Subscribe<_, DRIVER_NUM, { subscribe::READ }>,
            ),
            _,
            _,
        >(|handle| {
            let (allow_rw, subscribe) = handle.split();
            let len = buf.len();
            S::allow_rw::<C, DRIVER_NUM, { allow_rw::READ }>(allow_rw, buf)?;
            S::subscribe::<_, _, C, DRIVER_NUM, { subscribe::READ }>(subscribe, &called)?;

            // When this fails, `called` is guaranteed unmodified,
            // because upcalls are never processed until we call `yield`.
            S::command(DRIVER_NUM, command::READ, len as u32, 0).to_result()?;

            loop {
                S::yield_wait();
                if let Some((status, bytes_pushed_count)) = called.get() {
                    bytes_received = bytes_pushed_count as usize;
                    return match status {
                        0 => Ok(()),
                        e_status => Err(e_status.try_into().unwrap_or(ErrorCode::Fail)),
                    };
                }
            }
        });
        (bytes_received, r)
    }

    pub fn read_scope<ResT>(
        buf: &mut [u8],
        read_callback: &mut (dyn FnMut(usize, Result<&mut [u8], ErrorCode>) -> bool),
        scoped_code: impl FnOnce() -> ResT,
    ) -> Result<ResT, ErrorCode> {
        let len = buf.len();

        // SAFETY: this is dropped before or when this function returns.
        let allowed_buf = unsafe { AllowedBuf::<S, C>::share(buf) }?;

        let guard = ScopedRead::new(read_callback, allowed_buf);

        let subscribe_guard = platform::Subscribe::<S, DRIVER_NUM, { subscribe::READ }>::default();
        let subscribe_handle = unsafe { share::Handle::new(&subscribe_guard) };
        S::subscribe::<_, _, C, DRIVER_NUM, { subscribe::READ }>(subscribe_handle, &guard)?;

        // When this fails, `called` is guaranteed unmodified,
        // because upcalls are never processed until we call `yield`.
        S::command(DRIVER_NUM, command::READ, len as u32, 0).to_result::<(), ErrorCode>()?;

        let res = scoped_code();

        // Abort RX before `guard` is dropped, causing `allowed_buf` to be dropped.
        // `allowed_buf` ends `AllowRW` share, so let's stop DMA before something bad happens.
        S::command(DRIVER_NUM, command::ABORT, 0, 0).to_result::<(), ErrorCode>()?;

        Ok(res)
    }

    pub fn writer() -> ConsoleWriter<S> {
        ConsoleWriter {
            syscalls: Default::default(),
        }
    }
}

mod allowed_buf {
    use super::*;

    pub struct AllowedBuf<'share, S: Syscalls, C: Config> {
        lent_buf: *mut [u8],
        len: usize,
        covariance_phantom: PhantomData<&'share mut [u8]>,
        s_c: PhantomData<(S, C)>,
    }

    impl<'share, S: Syscalls, C: Config> AllowedBuf<'share, S, C> {
        pub fn len(&self) -> usize {
            self.len
        }

        unsafe fn share_unscoped(buf: &'share mut [u8]) -> Result<(), ErrorCode> {
            // SAFETY: The buffer being allowed here is going to be enclosed in an opaque type
            // until it's unallowed again. This prevents concurrent access to the buffer by process and kernel.

            let allow_rw = platform::AllowRw::<S, DRIVER_NUM, { allow_rw::READ }>::default();
            let allow_rw_handle = unsafe { share::Handle::new(&allow_rw) };
            S::allow_rw::<C, DRIVER_NUM, { allow_rw::READ }>(allow_rw_handle, buf)?;

            // This is crucial. This prevents unallowing the buffer at the end of scope.
            // Thanks to that, some buffer is constantly allowed for kernel to write there,
            // preventing data loss at any point.
            core::mem::forget(allow_rw);

            Ok(())
        }

        // SAFETY: caller must guarantee that the Drop impl is run.
        pub unsafe fn share(buf: &'share mut [u8]) -> Result<Self, ErrorCode> {
            let covariance_phantom = PhantomData::<&'share mut [u8]>;
            let len = buf.len();
            let lent_buf = buf as *mut [u8];
            unsafe {
                Self::share_unscoped(buf)?;
            }

            Ok(Self {
                len,
                lent_buf,
                covariance_phantom,
                s_c: PhantomData,
            })
        }

        pub fn inspect(&mut self, f: impl FnOnce(&mut [u8])) -> Result<(), ErrorCode> {
            S::unallow_rw(DRIVER_NUM, allow_rw::READ);

            // SAFETY: `lent_buf` was created from a mutable reference, so recreation of that mutable
            // reference is sound. Lifetimes and aliasing rules were enforced all the time by
            // `covariance_phantom`, which by covariance with the original mutable reference
            // kept it valid.
            let returned_buf = unsafe { &mut *self.lent_buf };

            f(returned_buf);

            unsafe { Self::share_unscoped(returned_buf) }
        }
    }

    impl<'share, S: Syscalls, C: Config> Drop for AllowedBuf<'share, S, C> {
        fn drop(&mut self) {
            S::unallow_rw(DRIVER_NUM, allow_rw::READ);
        }
    }
}
pub use allowed_buf::AllowedBuf;

struct ScopedRead<'scope, S: Syscalls, C: Config> {
    callback: MapCell<&'scope mut (dyn FnMut(usize, Result<&mut [u8], ErrorCode>) -> bool)>,
    allowed_buf: MapCell<AllowedBuf<'scope, S, C>>,
}

impl<'scope, S: Syscalls, C: Config> ScopedRead<'scope, S, C> {
    fn new(
        read_callback: &'scope mut (dyn FnMut(usize, Result<&mut [u8], ErrorCode>) -> bool),
        allowed_buf: AllowedBuf<'scope, S, C>,
    ) -> Self {
        Self {
            callback: MapCell::new(read_callback),
            allowed_buf: MapCell::new(allowed_buf),
        }
    }
}

impl<'scope, S: Syscalls, C: Config> Upcall<OneId<DRIVER_NUM, { subscribe::READ }>>
    for ScopedRead<'scope, S, C>
{
    fn upcall(&self, status: u32, bytes_pushed_count: u32, _arg2: u32) {
        // There is no way to propagate this error anywhere.
        let _res = self.callback.map(|callback| {
            self.allowed_buf.map(|allowed_buf| {
                allowed_buf.inspect(|buf| {
                    callback(bytes_pushed_count as usize, {
                        match status {
                            0 => Ok(buf),
                            e_status => Err(e_status.try_into().unwrap_or(ErrorCode::Fail)),
                        }
                    });
                })?;
                S::command(DRIVER_NUM, command::READ, allowed_buf.len() as u32, 0)
                    .to_result::<(), ErrorCode>()
            })
        });
    }
}

pub struct ConsoleWriter<S: Syscalls> {
    syscalls: PhantomData<S>,
}

impl<S: Syscalls> fmt::Write for ConsoleWriter<S> {
    fn write_str(&mut self, s: &str) -> Result<(), fmt::Error> {
        Console::<S>::write(s.as_bytes()).map_err(|_e| fmt::Error)
    }
}

/// System call configuration trait for `Console`.
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

const DRIVER_NUM: u32 = 1;

// Command IDs
#[allow(unused)]
mod command {
    pub const EXISTS: u32 = 0;
    pub const WRITE: u32 = 1;
    pub const READ: u32 = 2;
    pub const ABORT: u32 = 3;
}

#[allow(unused)]
mod subscribe {
    pub const WRITE: u32 = 1;
    pub const READ: u32 = 2;
}

mod allow_ro {
    pub const WRITE: u32 = 1;
}

mod allow_rw {
    pub const READ: u32 = 1;
}
