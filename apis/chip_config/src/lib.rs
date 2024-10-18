#![no_std]

use libtock_platform::{ErrorCode, Syscalls};

/// The chip configuration driver. // FIXME
///
/// It allows libraries to retrieve chip-configured values.
///
/// # Example
/// ```ignore
/// use libtock::ChipConfiguration;
///
/// // Retrieves IEEE MAC.
/// let mac = ChipConfiguration::ieee_mac()?;
/// ```
pub struct ChipConfiguration<S: Syscalls>(S);

impl<S: Syscalls> ChipConfiguration<S> {
    /// Run a check against the console capsule to ensure it is present.
    ///
    /// Returns `true` if the driver was present.
    #[inline(always)]
    pub fn exists() -> bool {
        S::command(DRIVER_NUM, command::EXISTS, 0, 0).is_success()
    }

    /// Gets IEEE MAC address configured in the chip.
    pub fn ieee_mac() -> Result<u64, ErrorCode> {
        S::command(DRIVER_NUM, command::IEEE_MAC, 0, 0).to_result()
    }
}

// -----------------------------------------------------------------------------
// Driver number and command IDs
// -----------------------------------------------------------------------------

const DRIVER_NUM: u32 = 0x90067;

// Command IDs
#[allow(unused)]
mod command {
    pub const EXISTS: u32 = 0;
    pub const IEEE_MAC: u32 = 1;
}
