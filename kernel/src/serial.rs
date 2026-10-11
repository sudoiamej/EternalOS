use core::arch::asm;
use core::fmt::{self, Write};
use core::sync::atomic::{AtomicBool, Ordering};

pub struct SerialPort;

static LOCK: AtomicBool = AtomicBool::new(false);

impl SerialPort {
    const COM1: u16 = 0x3F8;

    #[inline]
    unsafe fn outb(port: u16, val: u8) {
        unsafe {
            asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack, preserves_flags));
        }
    }

    #[inline]
    unsafe fn inb(port: u16) -> u8 {
        let val: u8;
        unsafe {
            asm!("in al, dx", in("dx") port, out("al") val, options(nomem, nostack, preserves_flags));
        }
        val
    }

    pub fn init() {
        unsafe {
            Self::outb(Self::COM1 + 1, 0x00); // Disable all interrupts
            Self::outb(Self::COM1 + 3, 0x80); // Enable DLAB (set baud rate divisor)
            Self::outb(Self::COM1 + 0, 0x03); // Set divisor to 3 (38400 baud)
            Self::outb(Self::COM1 + 1, 0x00);
            Self::outb(Self::COM1 + 3, 0x03); // 8 bits, no parity, one stop bit
            Self::outb(Self::COM1 + 2, 0xC7); // Enable FIFO, clear them, 14-byte threshold
            Self::outb(Self::COM1 + 4, 0x0B); // IRQs enabled, RTS/DSR set
        }
    }

    pub fn write_byte(byte: u8) {
        unsafe {
            let mut attempts = 0usize;
            while (Self::inb(Self::COM1 + 5) & 0x20) == 0 {
                attempts += 1;
                if attempts > 100_000 {
                    // Prevent deadlock if UART hardware hangs
                    return;
                }
            }
            Self::outb(Self::COM1, byte);
        }
    }
}

impl Write for SerialPort {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            if b == b'\n' {
                SerialPort::write_byte(b'\r');
            }
            SerialPort::write_byte(b);
        }
        Ok(())
    }
}


#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

impl LogLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            LogLevel::Trace => "[TRACE]",
            LogLevel::Debug => "[DEBUG]",
            LogLevel::Info  => "[INFO] ",
            LogLevel::Warn  => "[WARN] ",
            LogLevel::Error => "[ERROR]",
            LogLevel::Fatal => "[FATAL]",
        }
    }
}

pub fn emergency_write_str(s: &str) {
    for b in s.bytes() {
        if b == b'\n' {
            SerialPort::write_byte(b'\r');
        }
        SerialPort::write_byte(b);
    }
}

pub fn emergency_print(args: fmt::Arguments) {
    // Force reset LOCK in emergency path to guarantee output even if a lock was held during panic
    LOCK.store(false, Ordering::Release);
    let mut serial = SerialPort;
    let _ = serial.write_fmt(args);
}

pub fn _print(args: fmt::Arguments) {
    // Single-core BSP re-entrancy safe lock with recursion/deadlock protection
    let mut spin_count = 0usize;
    while LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
        spin_count += 1;
        if spin_count > 1_000_000 {
            // Break lock to prevent system lockup if an interrupt or panic interrupted active printing
            break;
        }
    }

    let mut serial = SerialPort;
    let _ = serial.write_fmt(args);

    LOCK.store(false, Ordering::Release);
}

pub fn log(level: LogLevel, args: fmt::Arguments) {
    _print(format_args!("{} {}\n", level.as_str(), args));
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::serial::_print(format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

#[macro_export]
macro_rules! log_trace {
    ($($arg:tt)*) => {
        $crate::serial::log($crate::serial::LogLevel::Trace, format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => {
        $crate::serial::log($crate::serial::LogLevel::Debug, format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        $crate::serial::log($crate::serial::LogLevel::Info, format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {
        $crate::serial::log($crate::serial::LogLevel::Warn, format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        $crate::serial::log($crate::serial::LogLevel::Error, format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_fatal {
    ($($arg:tt)*) => {
        $crate::serial::log($crate::serial::LogLevel::Fatal, format_args!($($arg)*))
    };
}
