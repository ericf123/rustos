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
use core::time::Duration;

const GPIO_BASE: usize = 0x3F000000 + 0x200000;
const GPIO_FSEL1: *mut u32 = (GPIO_BASE + 0x04) as *mut u32;
const GPIO_SET0: *mut u32 = (GPIO_BASE + 0x1C) as *mut u32;
const GPIO_CLR0: *mut u32 = (GPIO_BASE + 0x28) as *mut u32;

// FIXME: You need to add dependencies here to
// test your drivers (Phase 2). Add them as needed.

unsafe fn kmain() -> ! {
    /*let curr_fsel1 = GPIO_FSEL1.read_volatile(); 
    GPIO_FSEL1.write_volatile(curr_fsel1 | (0b001 << 18));
    loop {
        let curr_set0 = GPIO_SET0.read_volatile();
        GPIO_SET0.write_volatile(curr_set0 | (0b1 << 16));
        timer::spin_sleep(Duration::from_millis(1000));

        let curr_clr0 = GPIO_CLR0.read_volatile();
        GPIO_CLR0.write_volatile(curr_clr0 | (0b1 << 16));
        timer::spin_sleep(Duration::from_millis(1000));
    }
    */
    let mut led = Gpio::new(16).into_output();

    loop {
        led.set();
        timer::spin_sleep(Duration::from_millis(1000));
        led.clear();
        timer::spin_sleep(Duration::from_millis(1000));
    }

}
