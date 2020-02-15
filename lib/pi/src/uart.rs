use core::fmt;
use core::time::Duration;
use core::str;

use shim::io;
use shim::ioerr;
use shim::const_assert_size;

use volatile::prelude::*;
use volatile::{ReadVolatile, Reserved, Volatile};

use crate::common::IO_BASE;
use crate::gpio::{Function, Gpio};
use crate::timer;

/// The base address for the `MU` registers.
const MU_REG_BASE: usize = IO_BASE + 0x215040;

/// The `AUXENB` register from page 9 of the BCM2837 documentation.
const AUX_ENABLES: *mut Volatile<u8> = (IO_BASE + 0x215004) as *mut Volatile<u8>;

/// Enum representing bit fields of the `AUX_MU_LSR_REG` register.
#[repr(u8)]
enum LsrStatus {
    DataReady = 1,
    TxAvailable = 1 << 5,
}

#[repr(C)]
#[allow(non_snake_case)]
struct Registers {
    AUX_MU_IO: Volatile<u8>,
    _r0: [Reserved<u8>; 3],
    AUX_MU_IER: Volatile<u8>,
    _r1: [Reserved<u8>; 3],
    AUX_MU_IIR: Volatile<u8>,
    _r2: [Reserved<u8>; 3],
    AUX_MU_LCR: Volatile<u8>,
    _r3: [Reserved<u8>; 3],
    AUX_MU_MCR: Volatile<u8>,
    _r4: [Reserved<u8>; 3],
    AUX_MU_LSR: ReadVolatile<u8>,
    _r5: [Reserved<u8>; 3],
    AUX_MU_MSR: ReadVolatile<u8>,
    _r6: [Reserved<u8>; 3],
    AUX_MU_SCRATCH: Volatile<u8>,
    _r7: [Reserved<u8>; 3],
    AUX_MU_CNTL: Volatile<u8>,
    _r8: [Reserved<u8>; 3],
    AUX_MU_STAT: ReadVolatile<u32>,
    AUX_MU_BAUD: Volatile<u16>,
}

const_assert_size!(Registers, 0x7E21506C - 0x7E215040);

/// The Raspberry Pi's "mini UART".
pub struct MiniUart {
    registers: &'static mut Registers,
    timeout: Option<Duration>,
}

impl MiniUart {
    /// Initializes the mini UART by enabling it as an auxiliary peripheral,
    /// setting the data size to 8 bits, setting the BAUD rate to ~115200 (baud
    /// divider of 270), setting GPIO pins 14 and 15 to alternative function 5
    /// (TXD1/RDXD1), and finally enabling the UART transmitter and receiver.
    ///
    /// By default, reads will never time out. To set a read timeout, use
    /// `set_read_timeout()`.
    pub fn new() -> MiniUart {
        let registers = unsafe {
            // Enable the mini UART as an auxiliary device.
            (*AUX_ENABLES).or_mask(1);
            &mut *(MU_REG_BASE as *mut Registers)
        };

        // set up GPIO 14 and 15
        let _rxd1 = Gpio::new(14).into_alt(Function::Alt5);
        let _txd1 = Gpio::new(15).into_alt(Function::Alt5);

        registers.AUX_MU_LCR.or_mask(0b11); // set 8 bit data length
        registers.AUX_MU_BAUD.write(270); // set the baud rate to ~115200
        registers.AUX_MU_CNTL.write(0b11); // make sure TX/RX enabled

        MiniUart {
            registers: registers,
            timeout: None
        }
    }

    /// Set the read timeout to `t` duration.
    pub fn set_read_timeout(&mut self, t: Duration) {
        self.timeout = Some(t);
    }

    /// Write the byte `byte`. This method blocks until there is space available
    /// in the output FIFO.
    pub fn write_byte(&mut self, byte: u8) {
        let mut tx_fifo_ready = (self.registers.AUX_MU_LSR.read() & (LsrStatus::TxAvailable as u8)) > 0;

        // block until fifo is not full
        while !tx_fifo_ready {
            tx_fifo_ready = (self.registers.AUX_MU_LSR.read() & (LsrStatus::TxAvailable as u8)) > 0;
        }

        self.registers.AUX_MU_IO.write(byte);
    }

    /// Returns `true` if there is at least one byte ready to be read. If this
    /// method returns `true`, a subsequent call to `read_byte` is guaranteed to
    /// return immediately. This method does not block.
    pub fn has_byte(&self) -> bool {
        return self.registers.AUX_MU_LSR.read() & (LsrStatus::DataReady as u8) > 0;
    }

    /// Blocks until there is a byte ready to read. If a read timeout is set,
    /// this method blocks for at most that amount of time. Otherwise, this
    /// method blocks indefinitely until there is a byte to read.
    ///
    /// Returns `Ok(())` if a byte is ready to read. Returns `Err(())` if the
    /// timeout expired while waiting for a byte to be ready. If this method
    /// returns `Ok(())`, a subsequent call to `read_byte` is guaranteed to
    /// return immediately.
    pub fn wait_for_byte(&self) -> Result<(), ()> {
        let mut timed_out = false;
        let start = timer::current_time();
        while !self.has_byte() && !timed_out {
            timed_out = match self.timeout {
                Some(t) => timer::current_time() - start > t,
                _ => false
            };
        }

        if !timed_out {
            Ok(())
        } else {
            Err(())
        }
    }

    /// Reads a byte. Blocks indefinitely until a byte is ready to be read.
    pub fn read_byte(&mut self) -> u8 {
        while !self.has_byte() {
           unsafe { asm!("nop" :::: "volatile"); } 
        }

        self.registers.AUX_MU_IO.read()
    }
}

// FIXME: Implement `fmt::Write` for `MiniUart`. A b'\r' byte should be written
// before writing any b'\n' byte.
impl fmt::Write for MiniUart {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            if byte == b'\n' {
                self.write_byte(b'\r');
            }

            self.write_byte(byte);
        }
        Ok(())
    }
}

mod uart_io {
    use super::io;
    use super::ioerr;
    use super::MiniUart;
    use volatile::prelude::*;

    // FIXME: Implement `io::Read` and `io::Write` for `MiniUart`.
    //
    // The `io::Read::read()` implementation must respect the read timeout by
    // waiting at most that time for the _first byte_. It should not wait for
    // any additional bytes but _should_ read as many bytes as possible. If the
    // read times out, an error of kind `TimedOut` should be returned.
    //
    impl io::Read for MiniUart {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            match self.wait_for_byte() {
                Err(_) => return ioerr!(TimedOut, "waiting for byte timed out"),
                _ => ()
            };

            let mut count = 0;
            while count < buf.len() && self.has_byte() {
                buf[count] = self.read_byte();
                count += 1;
            }

            Ok(count)
        }
    }
    // The `io::Write::write()` method must write all of the requested bytes
    // before returning.
    impl io::Write for MiniUart {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            for byte in buf {
                self.write_byte(*byte);
            }

            Ok(buf.len())
        }

        fn flush(&mut self) -> Result<(), io::Error> {
            unimplemented!()
        }
    }
}
