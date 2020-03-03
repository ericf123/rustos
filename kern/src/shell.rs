use shim::io;
use shim::path::{Path, PathBuf, Component};
use alloc::vec::Vec;
use alloc::string::String;
use stack_vec::StackVec;

use pi::atags::Atags;

use fat32::traits::FileSystem;
use fat32::traits::{Dir, Entry};
use crate::fs::PiVFatHandle;

use crate::console::{kprint, kprintln, CONSOLE};
use pi::gpio::Gpio;
use pi::timer;
use core::time::Duration;
use crate::ALLOCATOR;
use crate::FILESYSTEM;
use core::str;

use fat32::vfat::File;

const BOOTLOADER_START_ADDR: usize = 0x4000000;
const BOOTLOADER_START: *mut u8 = BOOTLOADER_START_ADDR as *mut u8;

unsafe fn jump_to(addr: *mut u8) -> ! {
    asm!("br $0" : : "r"(addr as usize));
    loop {
        asm!("wfe" :::: "volatile")
    }
}

fn canonicalize(path: PathBuf) -> Result<PathBuf, ()> {
    let mut new_path = PathBuf::new();

    for comp in path.components() {
        match comp {
            Component::ParentDir => {
                let res = new_path.pop();
                if !res {
                    return Err(());
                }
            },
            Component::Normal(n) => new_path = new_path.join(n),
            Component::RootDir => new_path = ["/"].iter().collect(),
            _ => ()
        };
    }

    Ok(new_path)
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

fn ls<P: AsRef<Path>>(cwd: P, args: &mut StackVec<&str>) {
    use fat32::traits::Metadata;
    let mut dir_path: PathBuf; 
    let mut display_hidden = false;
    let mut path_arg = "";

    if args.len() > 1 && args.as_slice()[1] == "-a" {
        display_hidden = true;
    } 

    if display_hidden {
        if args.len() > 2 {
            path_arg = args.as_slice()[2];
        }
    } else if args.len() > 2 {
        path_arg = args.as_slice()[1];
    }
    
    
    if path_arg != "" {
        dir_path = cwd.as_ref().join(path_arg);
    } else {
        dir_path = cwd.as_ref().into();
    }

    dir_path = match canonicalize(dir_path.clone()) {
        Ok(p) => p,
        Err(_) => {
            kprintln!("\ninvalid path: {}", &dir_path.to_str().unwrap());
            return;
        }
    };

    let dir = match FILESYSTEM.open_dir(&dir_path) {
        Ok(d) => d,
        Err(_) => {
            kprintln!("\nno such file or directory: {}", &dir_path.to_str().unwrap());
            return;
        }
    };

    let entries = match dir.entries() {
        Ok(e) => e,
        Err(_) => {
            kprintln!("\ncan't list entries in dir!");
            return;
        }
    };

    kprintln!();
    for entry in entries {
        if !entry.metadata().hidden() || display_hidden {
            kprintln!("{}", entry.metadata());
        }
    }
}

fn cd(cwd: &mut PathBuf, args: StackVec<&str>) {
    if args.len() > 2 {
        kprintln!("\nusage: cd <dir>");
    } else {
        let mut path: PathBuf = [args.as_slice()[1]].iter().collect();
        if !path.is_absolute() {
            path = cwd.join(path);
        }     

        *cwd = match canonicalize(path) {
            Ok(p) => match FILESYSTEM.open_dir(&p) {
                Ok(_) => p,
                Err(_) => {
                    kprintln!("\nno such directory: {}", p.to_str().unwrap());
                    cwd.clone()
                }
            },
            Err(_) => {
                kprintln!("\nunable to cd to {} (bad path)", args.as_slice()[1]);
                cwd.clone()
            }
        };
    }
}

fn pwd<P: AsRef<Path>>(wd: P) {
    let print_me = match wd.as_ref().to_str() {
        Some(s) => s,
        None => "error printing working directory"
    };
    kprintln!("\n{}", print_me);
}

fn cat<P: AsRef<Path>>(cwd: P, args: StackVec<&str>) {
    use io::Read;
    if args.len() < 2 {
        kprintln!("\nusage: cat <file1> ... <filen>");
    } else {
        let mut concatenated: Vec<u8> = Vec::new();
        for arg in args.as_slice()[1..].iter() {
            let mut raw_path: PathBuf = [arg].iter().collect(); 
            if !raw_path.is_absolute() {
                raw_path = cwd.as_ref().join(raw_path);
            }

            let abs_path = match canonicalize(raw_path) {
                Ok(p) => p,
                Err(_) => {
                    kprintln!("\ninvalid arg: {}", arg);
                    break;
                }
            };
            let mut f: File<PiVFatHandle> = match FILESYSTEM.open_file(&abs_path) {
                Ok(res) => res,
                Err(_) => {
                    kprintln!("\ncan't open file: {}", &abs_path.to_str().unwrap());
                    break;
                }
            };


            match f.read_to_end(&mut concatenated) {
                Ok(_) => (),
                Err(_) => {
                    kprintln!("\nunable to read file: {}", f.name);
                    break;
                }
            };
        }

        let concat_str = match str::from_utf8(&concatenated) {
                Ok(s) => s,
                Err(_) => "\none or more files contained invalid UTF-8"
        };
        kprint!("\n{}", concat_str);
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
    let mut cwd: PathBuf = ["/"].iter().collect();
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
            Ok(mut command) => {
                match command.path() {
                    "alloc" => {
                        kprintln!("{:#?}", ALLOCATOR);
                        let mut v = Vec::new();
                        for i in 0..100000 {
                            //kprintln!("{}", i);
                            v.push(i);
                        }
                        for i in 133..150 {
                            kprintln!("{}", v.as_slice()[i]);
                        }
                        kprintln!("alloc str");
                    }
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
                    "pwd" => pwd(&cwd),
                    "ls" => ls(&cwd, &mut command.args),
                    "cat" => cat(&cwd, command.args),
                    "cd" => cd(&mut cwd, command.args),
                    "files" => {
                        use fat32::traits::Entry;
                        use fat32::traits::FileSystem;
                        use fat32::traits::Dir;

                        let root_dir = match (&FILESYSTEM).open("/") {
                            Ok(entry) => entry.into_dir().unwrap(),
                            _ => continue
                        };
                        kprintln!("");
                        for entry in root_dir.entries().unwrap() {
                            kprintln!("{}", entry.name());
                        }
                    }
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
