use shim::io;
use shim::path::{Path, PathBuf};

use stack_vec::StackVec;

use pi::atags::Atags;

use fat32::traits::FileSystem;
use fat32::traits::{Dir, Entry};

use crate::console::{kprint, kprintln, CONSOLE};
use pi::gpio::Gpio;
use pi::timer;
use core::time::Duration;
use crate::ALLOCATOR;
use crate::FILESYSTEM;

const BOOTLOADER_START_ADDR: usize = 0x4000000;
const BOOTLOADER_START: *mut u8 = BOOTLOADER_START_ADDR as *mut u8;

unsafe fn jump_to(addr: *mut u8) -> ! {
    asm!("br $0" : : "r"(addr as usize));
    loop {
        asm!("wfe" :::: "volatile")
    }
}

fn ldkern() -> ! {
    kprintln!("\nEntering bootloader!!! Screen will likely exit when kernel download starts.");
    unsafe {
        jump_to(BOOTLOADER_START); 
    }
}

fn blinkyboi() {
    // blink the built in led
    let mut led = Gpio::new(16).into_output(); 
    kprintln!();
    while !CONSOLE.lock().has_byte() {
        kprint!("ON \r");
        led.set();
        timer::spin_sleep(Duration::from_millis(150));
        kprint!("OFF\r");
        led.clear();
        timer::spin_sleep(Duration::from_millis(150));
    }
}

/// Error type for `Command` parse failures.
#[derive(Debug)]
enum Error {
    Empty,
    TooManyArgs,
}

/// A structure representing a single shell command.
struct Command<'a> {
    args: StackVec<'a, &'a str>,
}

impl<'a> Command<'a> {
    /// Parse a command from a string `s` using `buf` as storage for the
    /// arguments.
    ///
    /// # Errors
    ///
    /// If `s` contains no arguments, returns `Error::Empty`. If there are more
    /// arguments than `buf` can hold, returns `Error::TooManyArgs`.
    fn parse(s: &'a str, buf: &'a mut [&'a str]) -> Result<Command<'a>, Error> {
        let mut args = StackVec::new(buf);
        for arg in s.split(' ').filter(|a| !a.is_empty()) {
            args.push(arg).map_err(|_| Error::TooManyArgs)?;
        }

        if args.is_empty() {
            return Err(Error::Empty);
        }

        Ok(Command { args })
    }

    /// Returns this command's path. This is equivalent to the first argument.
    fn path(&self) -> &str {
        if !self.args.is_empty() {
            return self.args.as_slice()[0];
        }
        return "";
    }
}

/// Starts a shell using `prefix` as the prefix for each line. This function
/// never returns.
pub fn shell(prefix: &str) -> ! {
    // wait for user to be ready
    loop {
        kprint!("\r{}", prefix);
        if CONSOLE.lock().has_byte() {
            break;
        }
        // sleep to avoid rapid cursor movement at start
        timer::spin_sleep(Duration::from_millis(100));
    }

    loop {
        let mut parsed_buf: [&str; 64] = [""; 64];
        let mut command_buf = [0u8; 512];
        let mut count = 0;

        loop {
            let byte = CONSOLE.lock().read_byte();

            match byte {
                b'\r' | b'\n' => break,
                8 | 127 => {
                    // backspace/delete
                    if count > 0 {
                        kprint!("\x08 \x08");
                        count -= 1;
                        command_buf[count] = b' ';
                    } else { 
                        kprint!("\x07");
                    }

                },
                32..=126 | b'\t' => {
                    if count < 512 {
                        command_buf[count] = byte;
                        count += 1;
                        kprint!("{}", byte as char);
                    } else { 
                        kprint!("\x07");
                    }
                },
                _ => { 
                    kprint!("\x07");
                }
            }
        }

        let command_str = core::str::from_utf8(&command_buf[..count]).unwrap_or_default();
        match Command::parse(command_str, &mut parsed_buf) {
            Ok(command) => {
                match command.path() {
                    "echo" => {
                        let mut first = true;
                        kprintln!();
                        for arg in command.args {
                            if !first {
                                kprint!("{} ", arg);
                            } else {
                                first = false;
                            }
                        }
                    },
                    "ldkern" => ldkern(),
                    "blinkyboi" => blinkyboi(),
                    "panic" => panic!(),
                    "lsatags" => {
                        kprintln!("");
                        for atag in Atags::get() {
                            kprintln!("{:#?}", atag);
                        }
                    },
                    "memmap" => {
                        kprintln!("\n{:#?}", crate::allocator::memory_map().unwrap());
                    },
                    /*"test_string" => {
                        String::from("hello");
                    }*/
                    other => {
                        kprint!("\nunknown command: {}", other);
                    }
                } 
            },
            Err(Error::TooManyArgs) => {
                kprint!("\nerror: too many arguments");
            },
            Err(Error::Empty) => ()
        }
        kprint!("\n{}", prefix);
    }
}
