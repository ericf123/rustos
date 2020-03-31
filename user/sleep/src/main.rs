#![feature(asm)]
#![no_std]
#![no_main]

use kernel_api::println;
mod cr0;

fn main() {
    loop {
        let ms = 10000;
        let error: u64;
        let elapsed_ms: u64;

        unsafe {
            asm!("mov x0, $2
                  svc 1
                  mov $0, x0
                  mov $1, x7"
                 : "=r"(elapsed_ms), "=r"(error)
                 : "r"(ms)
                 : "x0", "x7"
                 : "volatile");
        }
        println!("Wait error ms: {}", elapsed_ms);
    }
}
