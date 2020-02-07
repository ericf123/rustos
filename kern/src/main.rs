#![feature(alloc_error_handler)]
#![feature(const_fn)]
#![feature(decl_macro)]
#![feature(asm)]
#![feature(global_asm)]
#![feature(optin_builtin_traits)]
#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]

#[cfg(not(test))]
mod init;

pub mod console;
pub mod mutex;
pub mod shell;

use console::kprintln;
use pi::timer;
use pi::gpio::*;
use pi::uart::*;
use core::time::Duration;
use core::fmt::Write;

const GPIO_BASE: usize = 0x3F000000 + 0x200000;
const GPIO_FSEL1: *mut u32 = (GPIO_BASE + 0x04) as *mut u32;
const GPIO_SET0: *mut u32 = (GPIO_BASE + 0x1C) as *mut u32;
const GPIO_CLR0: *mut u32 = (GPIO_BASE + 0x28) as *mut u32;

// FIXME: You need to add dependencies here to
// test your drivers (Phase 2). Add them as needed.

unsafe fn kmain() -> ! {
    /*let mut led = Gpio::new(18).into_output();

    loop {
        led.set();
        timer::spin_sleep(Duration::from_millis(200));
        led.clear();
        timer::spin_sleep(Duration::from_millis(200));
    }*/

    //let mut my_uart = MiniUart::new();
    loop {
        //let mut my_console = console::CONSOLE.lock();
        //let byte = my_console.read_byte();
        //kprintln!("You typed: {}", byte as char);
        //kprintln!("hello eric");
        //my_uart.write_str("hello world\n");
        shell::shell("> ");
    }
}
