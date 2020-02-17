use crate::atags::raw;
use core::slice;
use core::str::{from_utf8};

pub use crate::atags::raw::{Core, Mem};

/// An ATAG.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Atag {
    Core(raw::Core),
    Mem(raw::Mem),
    Cmd(&'static str),
    Unknown(u32),
    None,
}

impl Atag {
    /// Returns `Some` if this is a `Core` ATAG. Otherwise returns `None`.
    pub fn core(self) -> Option<Core> {
        match self {
            Atag::Core(raw) => Some(raw),
            _ => None
        }
    }

    /// Returns `Some` if this is a `Mem` ATAG. Otherwise returns `None`.
    pub fn mem(self) -> Option<Mem> {
        match self {
            Atag::Mem(raw) => Some(raw),
            _ => None
        }
    }

    /// Returns `Some` with the command line string if this is a `Cmd` ATAG.
    /// Otherwise returns `None`.
    pub fn cmd(self) -> Option<&'static str> {
        match self {
            Atag::Cmd(cmdline) => Some(cmdline),
            _ => None
        }
    }
}

// FIXME: Implement `From<&raw::Atag> for `Atag`.
impl From<&'static raw::Atag> for Atag {
    fn from(atag: &'static raw::Atag) -> Atag {
        unsafe {
            match (atag.tag, &atag.kind) {
                (raw::Atag::CORE, &raw::Kind { core }) => {
                    Atag::Core(core)
                },
                (raw::Atag::MEM, &raw::Kind { mem }) => {
                    Atag::Mem(mem)
                },
                (raw::Atag::CMDLINE, &raw::Kind { ref cmd }) => {
                    let cmdline = unsafe {
                        // find the size of the string by incrementing the u8 pointer
                        // until we find null terminator
                        let mut str_size = 0;
                        let mut curr = &cmd.cmd as *const u8;
                        while *curr != 0 {
                            str_size += 1;
                            curr = curr.add(1);
                        }
                        // get a string slice using the original pointer and size of string
                        let str_slice = slice::from_raw_parts(&cmd.cmd as *const u8, str_size);

                        // convert slice to &str
                        match core::str::from_utf8(str_slice) {
                            Ok(converted_str) => converted_str,
                            _ => "Error" 
                        }
                    };
                    Atag::Cmd(cmdline)
                },
                (raw::Atag::NONE, _) => Atag::None,
                (id, _) => Atag::Unknown(id),
            }
        }
    }
}
