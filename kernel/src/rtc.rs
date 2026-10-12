//! CMOS Real-Time Clock (RTC) Driver
//!
//! Reads hardware date and time from standard IBM PC CMOS registers (I/O ports 0x70 / 0x71).
//! Decodes BCD (Binary-Coded Decimal) or standard binary format, and accounts for 12/24 hour modes.

use core::arch::asm;

const CMOS_ADDR_PORT: u16 = 0x70;
const CMOS_DATA_PORT: u16 = 0x71;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateTime {
    pub year: u16,   // Full 4-digit year, e.g. 2026
    pub month: u8,   // 1..=12
    pub day: u8,     // 1..=31
    pub hour: u8,    // 0..=23
    pub minute: u8,  // 0..=59
    pub second: u8,  // 0..=59
}

impl DateTime {
    pub const fn zero() -> Self {
        Self {
            year: 2026,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
            second: 0,
        }
    }

    /// Format as "YYYY-MM-DD HH:MM:SS" into a 32-byte buffer
    pub fn format_iso<'a>(&self, buf: &'a mut [u8; 32]) -> &'a str {
        let y = self.year;
        let m = self.month;
        let d = self.day;
        let h = self.hour;
        let min = self.minute;
        let s = self.second;

        buf[0] = b'0' + ((y / 1000) % 10) as u8;
        buf[1] = b'0' + ((y / 100) % 10) as u8;
        buf[2] = b'0' + ((y / 10) % 10) as u8;
        buf[3] = b'0' + (y % 10) as u8;
        buf[4] = b'-';
        buf[5] = b'0' + (m / 10) as u8;
        buf[6] = b'0' + (m % 10) as u8;
        buf[7] = b'-';
        buf[8] = b'0' + (d / 10) as u8;
        buf[9] = b'0' + (d % 10) as u8;
        buf[10] = b' ';
        buf[11] = b'0' + (h / 10) as u8;
        buf[12] = b'0' + (h % 10) as u8;
        buf[13] = b':';
        buf[14] = b'0' + (min / 10) as u8;
        buf[15] = b'0' + (min % 10) as u8;
        buf[16] = b':';
        buf[17] = b'0' + (s / 10) as u8;
        buf[18] = b'0' + (s % 10) as u8;

        core::str::from_utf8(&buf[..19]).unwrap_or("2026-01-01 00:00:00")
    }

    /// Format as short time "HH:MM:SS" into a 16-byte buffer
    pub fn format_time<'a>(&self, buf: &'a mut [u8; 16]) -> &'a str {
        let h = self.hour;
        let min = self.minute;
        let s = self.second;

        buf[0] = b'0' + (h / 10) as u8;
        buf[1] = b'0' + (h % 10) as u8;
        buf[2] = b':';
        buf[3] = b'0' + (min / 10) as u8;
        buf[4] = b'0' + (min % 10) as u8;
        buf[5] = b':';
        buf[6] = b'0' + (s / 10) as u8;
        buf[7] = b'0' + (s % 10) as u8;

        core::str::from_utf8(&buf[..8]).unwrap_or("00:00:00")
    }
}

#[inline]
unsafe fn cmos_read(reg: u8) -> u8 {
    unsafe {
        asm!("out dx, al", in("dx") CMOS_ADDR_PORT, in("al") reg, options(nomem, nostack, preserves_flags));
        let val: u8;
        asm!("in al, dx", in("dx") CMOS_DATA_PORT, out("al") val, options(nomem, nostack, preserves_flags));
        val
    }
}

#[inline]
fn is_updating() -> bool {
    unsafe { (cmos_read(0x0A) & 0x80) != 0 }
}

/// Read the current hardware date and time from the CMOS RTC.
/// Performs two consecutive reads to avoid reading during an in-flight clock roll-over.
pub fn read_datetime() -> DateTime {
    // Wait until RTC update cycle is complete
    while is_updating() {
        core::hint::spin_loop();
    }

    let mut dt = read_registers();

    // Re-read until consecutive reads match to guarantee consistency
    loop {
        while is_updating() {
            core::hint::spin_loop();
        }
        let dt2 = read_registers();
        if dt == dt2 {
            break;
        }
        dt = dt2;
    }

    // Read register B to determine format (BCD vs Binary, 12h vs 24h)
    let reg_b = unsafe { cmos_read(0x0B) };
    let is_bcd = (reg_b & 0x04) == 0;
    let is_24h = (reg_b & 0x02) != 0;

    let mut second = dt.second;
    let mut minute = dt.minute;
    let mut hour = dt.hour;
    let mut day = dt.day;
    let mut month = dt.month;
    let mut year = dt.year as u8;

    if is_bcd {
        second = ((second >> 4) * 10) + (second & 0x0F);
        minute = ((minute >> 4) * 10) + (minute & 0x0F);
        hour = (((hour & 0x7F) >> 4) * 10) + (hour & 0x0F);
        day = ((day >> 4) * 10) + (day & 0x0F);
        month = ((month >> 4) * 10) + (month & 0x0F);
        year = ((year >> 4) * 10) + (year & 0x0F);
    }

    // Convert 12 hour clock to 24 hour if necessary
    if !is_24h && (dt.hour & 0x80) != 0 {
        hour = ((hour & 0x7F) + 12) % 24;
    }

    // Assume 21st century (2000-2099)
    let full_year = 2000u16 + (year as u16);

    DateTime {
        year: full_year,
        month,
        day,
        hour,
        minute,
        second,
    }
}

fn read_registers() -> DateTime {
    unsafe {
        let second = cmos_read(0x00);
        let minute = cmos_read(0x02);
        let hour = cmos_read(0x04);
        let day = cmos_read(0x07);
        let month = cmos_read(0x08);
        let year = cmos_read(0x09) as u16;

        DateTime {
            year,
            month,
            day,
            hour,
            minute,
            second,
        }
    }
}
