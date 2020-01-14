// FIXME: Make me compile. Diff budget: 12 line additions and 2 characters.

use std::{error::Error as StdError, fmt};

#[derive(Debug)]
struct ErrorA;
#[derive(Debug)]
struct ErrorB;

#[derive(Debug)]
enum Error {
    A(ErrorA),
    B(ErrorB),
}

impl StdError for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::A(_) => write!(f, "Oh no! Error A"),
            Error::B(_) => write!(f, "Oh no! Error B")
        }
    }
}

// What traits does `Error` need to implement?
fn do_a() -> Result<u16, ErrorA> {
    Err(ErrorA)
}

fn do_b() -> Result<u32, ErrorB> {
    Err(ErrorB)
}

fn do_both() -> Result<(u16, u32), Error> {
    Ok((do_a().unwrap(), do_b().unwrap()))
}

fn main() {}
