#![feature(asm)]
#![feature(global_asm)]

#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]

#[cfg(not(test))]
mod init;

use rand_core::{RngCore, Error, impls};
use rand::Rng;

const GPIO_BASE: usize = 0x3F000000 + 0x200000;

const GPIO_FSEL1: *mut u32 = (GPIO_BASE + 0x04) as *mut u32;
const GPIO_SET0: *mut u32 = (GPIO_BASE + 0x1C) as *mut u32;
const GPIO_CLR0: *mut u32 = (GPIO_BASE + 0x28) as *mut u32;

const MMIO_BASE: usize = 0x3F000000;
const RNG_CTRL: *mut u32  = (MMIO_BASE + 0x00104000) as *mut u32;
const RNG_STATUS: *mut u32 = (MMIO_BASE + 0x00104004) as *mut u32;
const RNG_DATA: *mut u32  = (MMIO_BASE + 0x00104008) as *mut u32;
const RNG_INT_MASK: *mut u32  = (MMIO_BASE + 0x00104010) as *mut u32;

#[inline(never)]
fn spin_sleep_ms(ms: usize) {
    for _ in 0..(ms * 6000) {
        unsafe { asm!("nop" :::: "volatile"); }
    }
}

unsafe fn rand_init() {
    RNG_STATUS.write_volatile(0x40000);
    let curr_mask: u32 = RNG_INT_MASK.read_volatile();
    RNG_INT_MASK.write_volatile(curr_mask | 1);
    let curr_ctrl: u32 = RNG_CTRL.read_volatile();
    RNG_INT_MASK.write_volatile(curr_ctrl | 1);

    while !(RNG_STATUS.read_volatile() >> 24) == 0 {
        asm!("nop" :::: "volatile");
    }

}

#[derive(Default)]
struct RdRand;

impl RngCore for RdRand {
    fn next_u32(&mut self) -> u32 {
        // implement!
        unsafe { RNG_DATA.read_volatile() as u32 } 
    }

    fn next_u64(&mut self) -> u64 {
        // implement!
        unsafe { ((RNG_DATA.read_volatile() << 32) as u64) | (RNG_DATA.read_volatile() as u64) } 
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        impls::fill_bytes_via_next(self, dest)
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), Error> {
        Ok(self.fill_bytes(dest))
    }
}

unsafe fn kmain() -> ! {
    rand_init();

    let curr_fsel1 = GPIO_FSEL1.read_volatile(); 
    GPIO_FSEL1.write_volatile(curr_fsel1 | (0b001 << 18));
    let mut rng: RdRand = Default::default();
    loop {
        let curr_set0 = GPIO_SET0.read_volatile();
        GPIO_SET0.write_volatile(curr_set0 | (0b1 << 16));
        spin_sleep_ms(rng.gen_range(0, 1000));

        let curr_clr0 = GPIO_CLR0.read_volatile();
        GPIO_CLR0.write_volatile(curr_clr0 | (0b1 << 16));
        spin_sleep_ms(rng.gen_range(0, 1000));
    }
}
