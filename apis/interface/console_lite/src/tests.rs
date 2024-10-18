use super::*;
use core::fmt::Write;
use libtock_platform::ErrorCode;
use libtock_unittest::{command_return, fake, ExpectedSyscall};

type ConsoleLite = super::ConsoleLite<fake::Syscalls>;

#[test]
fn no_driver() {
    let _kernel = fake::Kernel::new();
    assert!(!ConsoleLite::exists());
}

#[test]
fn exists() {
    let kernel = fake::Kernel::new();
    let driver = fake::Console::new();
    kernel.add_driver(&driver);

    assert!(ConsoleLite::exists());
    assert_eq!(driver.take_bytes(), &[]);
}

#[test]
fn write_bytes() {
    let kernel = fake::Kernel::new();
    let driver = fake::Console::new();
    kernel.add_driver(&driver);

    ConsoleLite::write(b"foo").unwrap();
    ConsoleLite::write(b"bar").unwrap();
    assert_eq!(driver.take_bytes(), b"foobar",);
}

#[test]
fn write_str() {
    let kernel = fake::Kernel::new();
    let driver = fake::Console::new();
    kernel.add_driver(&driver);

    write!(ConsoleLite::writer(), "foo").unwrap();
    assert_eq!(driver.take_bytes(), b"foo");
}

#[test]
fn failed_print() {
    let kernel = fake::Kernel::new();
    let driver = fake::Console::new();
    kernel.add_driver(&driver);
    kernel.add_expected_syscall(ExpectedSyscall::AllowRo {
        driver_num: DRIVER_NUM,
        buffer_num: allow_ro::WRITE,
        return_error: None,
    });
    kernel.add_expected_syscall(ExpectedSyscall::Subscribe {
        driver_num: DRIVER_NUM,
        subscribe_num: subscribe::WRITE,
        skip_with_error: None,
    });
    kernel.add_expected_syscall(ExpectedSyscall::Command {
        driver_id: DRIVER_NUM,
        command_id: command::WRITE,
        argument0: 5,
        argument1: 0,
        override_return: Some(command_return::failure(ErrorCode::Fail)),
    });

    assert_eq!(ConsoleLite::write(b"abcde"), Err(ErrorCode::Fail));
    // The fake driver still receives the command even if a fake error is injected.
    assert_eq!(driver.take_bytes(), b"abcde");
}
