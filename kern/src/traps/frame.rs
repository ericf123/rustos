use core::fmt;

#[repr(C)]
#[derive(Default, Copy, Clone, Debug)]
pub struct TrapFrame {
    pub ttbr0: u64,
    pub ttbr1: u64,
    pub tpidr: u64,
    pub sp: u64,
    pub spsr: u64,
    pub elr: u64,
    pub q_regs: [u128; 32],
    pub x_regs: [u64; 32],
}


