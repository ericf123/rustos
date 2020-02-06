mod parsers;

use serial;
use structopt;
use structopt_derive::StructOpt;
use xmodem::Xmodem;
use xmodem::Progress;

use std::path::PathBuf;
use std::time::Duration;

use structopt::StructOpt;
use serial::core::{CharSize, BaudRate, StopBits, FlowControl, SerialDevice, SerialPortSettings};

use parsers::{parse_width, parse_stop_bits, parse_flow_control, parse_baud_rate};

#[derive(StructOpt, Debug)]
#[structopt(about = "Write to TTY using the XMODEM protocol by default.")]
struct Opt {
    #[structopt(short = "i", help = "Input file (defaults to stdin if not set)", parse(from_os_str))]
    input: Option<PathBuf>,

    #[structopt(short = "b", long = "baud", parse(try_from_str = "parse_baud_rate"),
                help = "Set baud rate", default_value = "115200")]
    baud_rate: BaudRate,

    #[structopt(short = "t", long = "timeout", parse(try_from_str),
                help = "Set timeout in seconds", default_value = "10")]
    timeout: u64,

    #[structopt(short = "w", long = "width", parse(try_from_str = "parse_width"),
                help = "Set data character width in bits", default_value = "8")]
    char_width: CharSize,

    #[structopt(help = "Path to TTY device", parse(from_os_str))]
    tty_path: PathBuf,

    #[structopt(short = "f", long = "flow-control", parse(try_from_str = "parse_flow_control"),
                help = "Enable flow control ('hardware' or 'software')", default_value = "none")]
    flow_control: FlowControl,

    #[structopt(short = "s", long = "stop-bits", parse(try_from_str = "parse_stop_bits"),
                help = "Set number of stop bits", default_value = "1")]
    stop_bits: StopBits,

    #[structopt(short = "r", long = "raw", help = "Disable XMODEM")]
    raw: bool,
}

fn main() {
    use std::fs::File;
    use std::io::{self, BufReader, BufRead};

    let opt = Opt::from_args();
    let mut port = serial::open(&opt.tty_path).expect("path points to invalid TTY");

    // FIXME: Implement the `ttywrite` utility.

    let mut input: Box<dyn BufRead> = match opt.input {
        Some(path) => Box::new(BufReader::new(File::open(path).expect("input file not found"))),
        None => Box::new(BufReader::new(io::stdin()))
    };

    // handle settings
    let mut settings = port.read_settings().expect("settings to be valid");
    settings.set_baud_rate(opt.baud_rate).expect("set baud rate failed");
    settings.set_char_size(opt.char_width);
    settings.set_stop_bits(opt.stop_bits);
    settings.set_flow_control(opt.flow_control);
    port.write_settings(&settings).expect("write settings failed");
    port.set_timeout(Duration::new(opt.timeout, 0)).expect("set timeout failed");

    let num_written: usize;
    if opt.raw {
        num_written = io::copy(&mut input, &mut port).expect("raw write failed") as usize;
    } else {
        num_written = Xmodem::transmit_with_progress(&mut input, &mut port, progress_fn).expect("XMODEM failed");
    }
    println!("wrote {} bytes to input", num_written);
}


fn progress_fn(progress: Progress) {
    println!("Progress: {:?}", progress);
}
