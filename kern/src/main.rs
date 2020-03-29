#![feature(alloc_error_handler)]
#![feature(const_fn)]
#![feature(decl_macro)]
#![feature(asm)]
#![feature(global_asm)]
#![feature(optin_builtin_traits)]
#![feature(ptr_internals)]
#![feature(raw_vec_internals)]
#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]
#![feature(panic_info_message)]

#[cfg(not(test))]
mod init;

extern crate alloc;

pub mod allocator;
pub mod console;
pub mod fs;
pub mod mutex;
pub mod shell;
pub mod param;
pub mod process;
pub mod traps;
pub mod vm;

use console::kprintln;

use allocator::Allocator;
use fs::FileSystem;
use process::GlobalScheduler;
use traps::irq::Irq;
use vm::VMManager;
use aarch64;

#[cfg_attr(not(test), global_allocator)]
pub static ALLOCATOR: Allocator = Allocator::uninitialized();
pub static FILESYSTEM: FileSystem = FileSystem::uninitialized();
pub static SCHEDULER: GlobalScheduler = GlobalScheduler::uninitialized();
pub static VMM: VMManager = VMManager::uninitialized();
pub static IRQ: Irq = Irq::uninitialized();


use core::time::Duration;
extern crate pi;
use pi::timer;


pub extern "C" fn start_shell() {
    loop { shell::shell("user> "); } 
}

pub extern fn tp1() {
    let mut i = 0;
    while true { 
        kprintln!("hello from process 1 ({})", i); 
        timer::spin_sleep(Duration::from_millis(250));
        i += 1;
    }
}

pub extern fn tp2() {
    let mut i = 100000000;
    while true { 
        kprintln!("hello from process 2 ({})", i); 
        timer::spin_sleep(Duration::from_millis(250));
        i -= 1;
    }
}

fn kmain() -> ! {
    timer::spin_sleep(Duration::from_secs(5));
    unsafe {
        ALLOCATOR.initialize();
        //FILESYSTEM.initialize();
        IRQ.initialize();
        SCHEDULER.initialize();
        SCHEDULER.start();
    } 

    //aarch64::brk!(2);

    //loop { shell::shell("> "); } 
}
