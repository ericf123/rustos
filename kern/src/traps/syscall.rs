use alloc::boxed::Box;
use core::time::Duration;

use crate::console::{kprint};
use crate::process::State;
use crate::process::state::EventPollFn;
use crate::traps::TrapFrame;
use crate::SCHEDULER;
use kernel_api::*;
extern crate pi;
use pi::timer;

/// Sleep for `ms` milliseconds.
///
/// This system call takes one parameter: the number of milliseconds to sleep.
///
/// In addition to the usual status value, this system call returns one
/// parameter: the approximate true elapsed time from when `sleep` was called to
/// when `sleep` returned.
pub fn sys_sleep(ms: u32, tf: &mut TrapFrame) {
    let done_time = timer::current_time() + Duration::from_millis(ms as u64);
    let done_sleeping: EventPollFn = Box::new(move |process| {
        //kprintln!("hello from poll fn");
        if timer::current_time() >= done_time {
            // save return value and ecode
            let ret_val = Duration::as_millis(&(timer::current_time() - done_time)) as u64;
            process.context.x_regs[0] = ret_val; // set actually elapsed ms
            process.context.x_regs[7] = 1; // no error
            return true;
        }
        return false;
    });

    SCHEDULER.switch(State::Waiting(done_sleeping), tf);
}

/// Returns current time.
///
/// This system call does not take parameter.
///
/// In addition to the usual status value, this system call returns two
/// parameter:
///  - current time as seconds
///  - fractional part of the current time, in nanoseconds.
pub fn sys_time(tf: &mut TrapFrame) {
    let curr_time = timer::current_time();
    tf.x_regs[0] = curr_time.as_secs();
    tf.x_regs[1] = curr_time.subsec_nanos() as u64;
    tf.x_regs[7] = OsError::Ok as u64;
}

/// Kills current process.
///
/// This system call does not take paramer and does not return any value.
pub fn sys_exit(tf: &mut TrapFrame) {
    let _ = SCHEDULER.kill(tf);
}

/// Write to console.
///
/// This system call takes one parameter: a u8 character to print.
///
/// It only returns the usual status value.
pub fn sys_write(b: u8, tf: &mut TrapFrame) {
    kprint!("{}", b as char);
    tf.x_regs[7] = OsError::Ok as u64;
}

/// Returns current process's ID.
///
/// This system call does not take parameter.
///
/// In addition to the usual status value, this system call returns a
/// parameter: the current process's ID.
pub fn sys_getpid(tf: &mut TrapFrame) {
    tf.x_regs[0] = tf.tpidr;
    tf.x_regs[7] = OsError::Ok as u64;
}

pub fn handle_syscall(num: u16, tf: &mut TrapFrame) {
    match num as usize {
        NR_SLEEP => sys_sleep(tf.x_regs[0] as u32, tf),
        NR_WRITE => sys_write(tf.x_regs[0] as u8, tf),
        NR_TIME => sys_time(tf),
        NR_EXIT => sys_exit(tf),
        NR_GETPID => sys_getpid(tf),
        _ => unimplemented!("unimplemented syscall!!")
    };
}
