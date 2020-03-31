#![feature(asm)]
#![no_std]
#![no_main]

mod cr0;

use kernel_api::println;
use kernel_api::syscall::{sleep, time, getpid};
use core::time::Duration;

fn main() {
    println!("Hello from Process #{}...this is a syscall test.", getpid());
    println!("The current time is {:#?}", time());
    println!("Sleeping for 5 seconds...");
    sleep(Duration::from_secs(5)).unwrap();
    println!("It's Process #{}...I'm exiting. Bye!", getpid());
}
