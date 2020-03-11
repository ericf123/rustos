use aarch64::ESR_EL1;

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum Fault {
    AddressSize,
    Translation,
    AccessFlag,
    Permission,
    Alignment,
    TlbConflict,
    Other(u8),
}

impl From<u32> for Fault {
    fn from(val: u32) -> Fault {
        match ESR_EL1::get_value(val as u64, ESR_EL1::ISS) & 0x3F {
            0..=3 => Fault::AddressSize,
            4..=7 => Fault::Translation,
            9..=11 => Fault::AccessFlag,
            13..=15 => Fault::Permission,
            33 => Fault::Alignment,
            48 => Fault::TlbConflict,
            v @ _ => Fault::Other(v as u8)
        }
    }
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum Syndrome {
    Unknown,
    WfiWfe,
    SimdFp,
    IllegalExecutionState,
    Svc(u16),
    Hvc(u16),
    Smc(u16),
    MsrMrsSystem,
    InstructionAbort { kind: Fault, level: u8 },
    PCAlignmentFault,
    DataAbort { kind: Fault, level: u8 },
    SpAlignmentFault,
    TrappedFpu,
    SError,
    Breakpoint,
    Step,
    Watchpoint,
    Brk(u16),
    Other(u32),
}

/// Converts a raw syndrome value (ESR) into a `Syndrome` (ref: D1.10.4).
impl From<u32> for Syndrome {
    fn from(esr: u32) -> Syndrome {
        use self::Syndrome::*;

        match ESR_EL1::get_value(esr as u64, ESR_EL1::EC) {
            0 => Unknown,
            1 => WfiWfe,
            7 => SimdFp,
            14 => IllegalExecutionState,
            17 | 21 => Svc(ESR_EL1::get_value(esr as u64, ESR_EL1::ISS_HSVC_IMM) as u16),
            18 | 22 => Hvc(ESR_EL1::get_value(esr as u64, ESR_EL1::ISS_HSVC_IMM) as u16),
            19 | 23 => Smc(ESR_EL1::get_value(esr as u64, ESR_EL1::ISS_HSVC_IMM) as u16),
            24 => MsrMrsSystem,
            32 | 33 => InstructionAbort { kind: esr.into(), level: (ESR_EL1::get_value(esr as u64, ESR_EL1::ISS) & 0x3) as u8 },
            34 => PCAlignmentFault,
            36 | 37 => DataAbort { kind: esr.into(), level: (ESR_EL1::get_value(esr as u64, ESR_EL1::ISS) & 0x3) as u8 },
            38 => SpAlignmentFault,
            40 | 44 => TrappedFpu,
            47 => SError,
            48 | 49 => Breakpoint,
            50 | 51 => Step,
            52 | 53 => Watchpoint,
            60 => Brk(ESR_EL1::get_value(esr as u64, ESR_EL1::ISS_BRK_CMMT) as u16),
            _ => Other(esr)
        }

    }
}
