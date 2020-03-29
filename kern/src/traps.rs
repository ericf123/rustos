mod frame;
mod syndrome;
mod syscall;

pub mod irq;
pub use self::frame::TrapFrame;

use pi::interrupt::{Controller, Interrupt};

use self::syndrome::Syndrome;
use self::syscall::handle_syscall;
use crate::console::kprintln;
use crate::shell;
extern crate pi;
use pi::interrupt;
use pi::timer;
use core::time::Duration;
use aarch64;
use crate::IRQ;

#[repr(u16)]
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum Kind {
    Synchronous = 0,
    Irq = 1,
    Fiq = 2,
    SError = 3,
}

#[repr(u16)]
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum Source {
    CurrentSpEl0 = 0,
    CurrentSpElx = 1,
    LowerAArch64 = 2,
    LowerAArch32 = 3,
}

#[repr(C)]
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub struct Info {
    source: Source,
    kind: Kind,
}

/// This function is called when an exception occurs. The `info` parameter
/// specifies the source and kind of exception that has occurred. The `esr` is
/// the value of the exception syndrome register. Finally, `tf` is a pointer to
/// the trap frame for the exception.
#[no_mangle]
pub extern "C" fn handle_exception(info: Info, esr: u32, tf: &mut TrapFrame) {
    match info {
        Info {source, kind: Kind::Synchronous } => match Syndrome::from(esr) {
            Syndrome::Brk(b) => { 
                //loop { kprintln!("elr: {:x}", tf.elr); }
                //loop { kprintln!("tf: {:#?}", tf); timer::spin_sleep(Duration::from_millis(1000)); }
                kprintln!("source {:#?}, num {}", source, b);
                shell::shell("debug> "); 
                tf.elr += 4;
            },
            Syndrome::Svc(num) => handle_syscall(num, tf),
            syndrome @ _ => kprintln!("no handler: {:#?}", syndrome),
        },
        Info {source, kind: Kind::Irq} => {
            let int_controller = interrupt::Controller::new();
            for int in interrupt::Interrupt::iter() {
                if int_controller.is_pending(*int) {
                    if IRQ.handler_exists(*int) {
                        IRQ.invoke(*int, tf);
                    } else {
                        kprintln!("no handler for irq: {:#?}", *int);
                    }
                } 
            }
        }, 
        info @ _ => { 
            kprintln!("no handler: {:#?}", info);
            timer::spin_sleep(Duration::from_secs(10));
        },
    }
}
