use stack_vec::StackVec;

use crate::console::{kprint, kprintln, CONSOLE};
use core::fmt::Write;

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
/// returns if the `exit` command is called.
pub fn shell(prefix: &str) -> ! {
    kprint!("\r");
    //let mut my_console = CONSOLE.lock();
    loop {
        kprint!("\n{}", prefix);
        let mut parsed_buf: [&str; 64] = [""; 64];
        let mut command_buf = [0u8; 512];
        let mut count = 0;

        while count < 512 {
            let byte = CONSOLE.lock().read_byte();

            match byte {
                b'\r' | b'\n' => break,
                8 | 127 => {
                    // backspace/delete
                    if count > 0 {
                        kprint!("\x08 \x08");
                        count -= 1;
                        command_buf[count] = b' ';
                    }
                },
                32..=126 | b'\t' => {
                    command_buf[count] = byte;
                    count += 1;
                    kprint!("{}", byte as char);
                },
                _ => { 
                    kprint!("\x07");
                }
            }
        }

        if count == 0 {
            continue;
        }

        let command_str = core::str::from_utf8(&command_buf).unwrap_or_default();
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
    }
}
