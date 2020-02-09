#![feature(asm)]
#![feature(global_asm)]

#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]

#[cfg(not(test))]
mod init;

use xmodem::Xmodem;
use core::time::Duration;
use core::slice;
//use core::io::Write;
use core::fmt::Write;
use shim::io;
//use shim::ioerr;
use pi;

/// Start address of the binary to load and of the bootloader.
const BINARY_START_ADDR: usize = 0x80000;
const BOOTLOADER_START_ADDR: usize = 0x4000000;

/// Pointer to where the loaded binary expects to be laoded.
const BINARY_START: *mut u8 = BINARY_START_ADDR as *mut u8;

/// Free space between the bootloader and the loaded binary's start address.
const MAX_BINARY_SIZE: usize = BOOTLOADER_START_ADDR - BINARY_START_ADDR;

/// Branches to the address `addr` unconditionally.
unsafe fn jump_to(addr: *mut u8) -> ! {
    asm!("br $0" : : "r"(addr as usize));
    loop {
        asm!("wfe" :::: "volatile")
    }
}

fn kmain() -> ! {
    let mut my_uart = pi::uart::MiniUart::new();
    my_uart.set_read_timeout(Duration::from_millis(750));
    loop {
        let dest_buf: &mut [u8] = unsafe { slice::from_raw_parts_mut(BINARY_START, MAX_BINARY_SIZE) };
        match Xmodem::receive(&mut my_uart, dest_buf) {
            Ok(_) => unsafe {
                jump_to(BINARY_START) 
            },
            Err(e) => match e.kind() {
                io::ErrorKind::TimedOut => continue,
                _ => {
                    match my_uart.write_str("Error occurred. Retrying...\n") {
                        _ => ()
                    }
                }
            } 
        };
    }
}
