//! PS/2 Keyboard Driver (i8042 Controller, Scancode Set 1 & Extended)
//!
//! Handles scancodes, make/break states, modifier state (Shift, Ctrl, Alt, CapsLock),
//! keycode mapping to `KeyCode`, and translation to ASCII characters.

use core::arch::asm;
use crate::input::{post_event, push_char, InputEvent, KeyCode, Modifiers};
use crate::sync::SpinLock;
use crate::timer::{send_eoi_irq, unmask_irq};

const PS2_DATA_PORT: u16 = 0x60;
const PS2_STATUS_PORT: u16 = 0x64;
const PS2_COMMAND_PORT: u16 = 0x64;

#[inline]
unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    unsafe {
        asm!("in al, dx", in("dx") port, out("al") val, options(nomem, nostack, preserves_flags));
    }
    val
}

#[inline]
unsafe fn outb(port: u16, val: u8) {
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack, preserves_flags));
    }
}

#[inline]
unsafe fn io_wait() {
    unsafe {
        outb(0x80, 0);
    }
}

/// Wait until controller input buffer is clear (ready to accept command/data)
pub fn wait_controller_ready() -> bool {
    for _ in 0..10_000 {
        if unsafe { inb(PS2_STATUS_PORT) & 0x02 } == 0 {
            return true;
        }
        core::hint::spin_loop();
    }
    false
}

/// Check if PS/2 controller has keyboard data ready (Bit 0 set, Bit 5 clear)
pub unsafe fn has_data() -> bool {
    let status = unsafe { inb(PS2_STATUS_PORT) };
    (status & 0x01) != 0 && (status & 0x20) == 0
}

/// Wait until controller output buffer has data to read
pub fn wait_controller_output() -> bool {
    for _ in 0..10_000 {
        if unsafe { inb(PS2_STATUS_PORT) & 0x01 } != 0 {
            return true;
        }
        core::hint::spin_loop();
    }
    false
}

pub struct KeyboardDecoder {
    pub extended: bool,
    pub left_shift: bool,
    pub right_shift: bool,
    pub left_ctrl: bool,
    pub right_ctrl: bool,
    pub left_alt: bool,
    pub right_alt: bool,
    pub caps_lock: bool,
}

impl KeyboardDecoder {
    pub const fn new() -> Self {
        Self {
            extended: false,
            left_shift: false,
            right_shift: false,
            left_ctrl: false,
            right_ctrl: false,
            left_alt: false,
            right_alt: false,
            caps_lock: false,
        }
    }

    pub fn modifiers(&self) -> Modifiers {
        Modifiers {
            shift: self.left_shift || self.right_shift,
            ctrl: self.left_ctrl || self.right_ctrl,
            alt: self.left_alt || self.right_alt,
            caps_lock: self.caps_lock,
        }
    }

    /// Process a raw PS/2 scancode byte and produce an `InputEvent`
    pub fn process_byte(&mut self, byte: u8) -> Option<InputEvent> {
        if byte == 0xE0 {
            self.extended = true;
            return None;
        }

        let is_extended = self.extended;
        self.extended = false; // Reset for next byte

        let is_break = (byte & 0x80) != 0;
        let scancode = byte & 0x7F;

        if is_extended {
            self.handle_extended(scancode, is_break)
        } else {
            self.handle_standard(scancode, is_break)
        }
    }

    fn handle_standard(&mut self, scancode: u8, is_break: bool) -> Option<InputEvent> {
        let key = match scancode {
            0x01 => KeyCode::Escape,
            0x02 => KeyCode::Key1,
            0x03 => KeyCode::Key2,
            0x04 => KeyCode::Key3,
            0x05 => KeyCode::Key4,
            0x06 => KeyCode::Key5,
            0x07 => KeyCode::Key6,
            0x08 => KeyCode::Key7,
            0x09 => KeyCode::Key8,
            0x0A => KeyCode::Key9,
            0x0B => KeyCode::Key0,
            0x0C => KeyCode::Minus,
            0x0D => KeyCode::Equals,
            0x0E => KeyCode::Backspace,
            0x0F => KeyCode::Tab,
            0x10 => KeyCode::Q,
            0x11 => KeyCode::W,
            0x12 => KeyCode::E,
            0x13 => KeyCode::R,
            0x14 => KeyCode::T,
            0x15 => KeyCode::Y,
            0x16 => KeyCode::U,
            0x17 => KeyCode::I,
            0x18 => KeyCode::O,
            0x19 => KeyCode::P,
            0x1A => KeyCode::LeftBracket,
            0x1B => KeyCode::RightBracket,
            0x1C => KeyCode::Enter,
            0x1D => {
                self.left_ctrl = !is_break;
                KeyCode::LeftCtrl
            }
            0x1E => KeyCode::A,
            0x1F => KeyCode::S,
            0x20 => KeyCode::D,
            0x21 => KeyCode::F,
            0x22 => KeyCode::G,
            0x23 => KeyCode::H,
            0x24 => KeyCode::J,
            0x25 => KeyCode::K,
            0x26 => KeyCode::L,
            0x27 => KeyCode::Semicolon,
            0x28 => KeyCode::Apostrophe,
            0x29 => KeyCode::Grave,
            0x2A => {
                self.left_shift = !is_break;
                KeyCode::LeftShift
            }
            0x2B => KeyCode::Backslash,
            0x2C => KeyCode::Z,
            0x2D => KeyCode::X,
            0x2E => KeyCode::C,
            0x2F => KeyCode::V,
            0x30 => KeyCode::B,
            0x31 => KeyCode::N,
            0x32 => KeyCode::M,
            0x33 => KeyCode::Comma,
            0x34 => KeyCode::Period,
            0x35 => KeyCode::Slash,
            0x36 => {
                self.right_shift = !is_break;
                KeyCode::RightShift
            }
            0x38 => {
                self.left_alt = !is_break;
                KeyCode::LeftAlt
            }
            0x39 => KeyCode::Space,
            0x3A => {
                if !is_break {
                    self.caps_lock = !self.caps_lock;
                }
                KeyCode::CapsLock
            }
            0x3B => KeyCode::F1,
            0x3C => KeyCode::F2,
            0x3D => KeyCode::F3,
            0x3E => KeyCode::F4,
            0x3F => KeyCode::F5,
            0x40 => KeyCode::F6,
            0x41 => KeyCode::F7,
            0x42 => KeyCode::F8,
            0x43 => KeyCode::F9,
            0x44 => KeyCode::F10,
            0x57 => KeyCode::F11,
            0x58 => KeyCode::F12,
            _ => KeyCode::Unknown,
        };

        let mods = self.modifiers();

        if is_break {
            Some(InputEvent::KeyUp {
                key,
                modifiers: mods,
            })
        } else {
            let ascii = key_to_ascii(key, mods);
            Some(InputEvent::KeyDown {
                key,
                modifiers: mods,
                ascii,
            })
        }
    }

    fn handle_extended(&mut self, scancode: u8, is_break: bool) -> Option<InputEvent> {
        let key = match scancode {
            0x1D => {
                self.right_ctrl = !is_break;
                KeyCode::RightCtrl
            }
            0x38 => {
                self.right_alt = !is_break;
                KeyCode::RightAlt
            }
            0x48 => KeyCode::UpArrow,
            0x50 => KeyCode::DownArrow,
            0x4B => KeyCode::LeftArrow,
            0x4D => KeyCode::RightArrow,
            0x47 => KeyCode::Home,
            0x4F => KeyCode::End,
            0x49 => KeyCode::PageUp,
            0x51 => KeyCode::PageDown,
            0x52 => KeyCode::Insert,
            0x53 => KeyCode::Delete,
            0x1C => KeyCode::Enter, // Keypad enter
            0x35 => KeyCode::Slash, // Keypad slash
            _ => KeyCode::Unknown,
        };

        let mods = self.modifiers();

        if is_break {
            Some(InputEvent::KeyUp {
                key,
                modifiers: mods,
            })
        } else {
            let ascii = match key {
                KeyCode::Enter => Some(b'\n'),
                KeyCode::Slash => Some(b'/'),
                _ => None,
            };
            Some(InputEvent::KeyDown {
                key,
                modifiers: mods,
                ascii,
            })
        }
    }
}

/// Convert KeyCode and active Modifiers into standard US QWERTY ASCII byte
pub fn key_to_ascii(key: KeyCode, mods: Modifiers) -> Option<u8> {
    let shift = mods.shift;
    let caps = mods.caps_lock;
    // Letters shift with (shift ^ caps)
    let letter_upper = shift ^ caps;

    match key {
        KeyCode::A => Some(if letter_upper { b'A' } else { b'a' }),
        KeyCode::B => Some(if letter_upper { b'B' } else { b'b' }),
        KeyCode::C => Some(if letter_upper { b'C' } else { b'c' }),
        KeyCode::D => Some(if letter_upper { b'D' } else { b'd' }),
        KeyCode::E => Some(if letter_upper { b'E' } else { b'e' }),
        KeyCode::F => Some(if letter_upper { b'F' } else { b'f' }),
        KeyCode::G => Some(if letter_upper { b'G' } else { b'g' }),
        KeyCode::H => Some(if letter_upper { b'H' } else { b'h' }),
        KeyCode::I => Some(if letter_upper { b'I' } else { b'i' }),
        KeyCode::J => Some(if letter_upper { b'J' } else { b'j' }),
        KeyCode::K => Some(if letter_upper { b'K' } else { b'k' }),
        KeyCode::L => Some(if letter_upper { b'L' } else { b'l' }),
        KeyCode::M => Some(if letter_upper { b'M' } else { b'm' }),
        KeyCode::N => Some(if letter_upper { b'N' } else { b'n' }),
        KeyCode::O => Some(if letter_upper { b'O' } else { b'o' }),
        KeyCode::P => Some(if letter_upper { b'P' } else { b'p' }),
        KeyCode::Q => Some(if letter_upper { b'Q' } else { b'q' }),
        KeyCode::R => Some(if letter_upper { b'R' } else { b'r' }),
        KeyCode::S => Some(if letter_upper { b'S' } else { b's' }),
        KeyCode::T => Some(if letter_upper { b'T' } else { b't' }),
        KeyCode::U => Some(if letter_upper { b'U' } else { b'u' }),
        KeyCode::V => Some(if letter_upper { b'V' } else { b'v' }),
        KeyCode::W => Some(if letter_upper { b'W' } else { b'w' }),
        KeyCode::X => Some(if letter_upper { b'X' } else { b'x' }),
        KeyCode::Y => Some(if letter_upper { b'Y' } else { b'y' }),
        KeyCode::Z => Some(if letter_upper { b'Z' } else { b'z' }),

        KeyCode::Key1 => Some(if shift { b'!' } else { b'1' }),
        KeyCode::Key2 => Some(if shift { b'@' } else { b'2' }),
        KeyCode::Key3 => Some(if shift { b'#' } else { b'3' }),
        KeyCode::Key4 => Some(if shift { b'$' } else { b'4' }),
        KeyCode::Key5 => Some(if shift { b'%' } else { b'5' }),
        KeyCode::Key6 => Some(if shift { b'^' } else { b'6' }),
        KeyCode::Key7 => Some(if shift { b'&' } else { b'7' }),
        KeyCode::Key8 => Some(if shift { b'*' } else { b'8' }),
        KeyCode::Key9 => Some(if shift { b'(' } else { b'9' }),
        KeyCode::Key0 => Some(if shift { b')' } else { b'0' }),

        KeyCode::Space => Some(b' '),
        KeyCode::Enter => Some(b'\n'),
        KeyCode::Backspace => Some(0x08),
        KeyCode::Tab => Some(b'\t'),
        KeyCode::Escape => Some(0x1B),

        KeyCode::Minus => Some(if shift { b'_' } else { b'-' }),
        KeyCode::Equals => Some(if shift { b'+' } else { b'=' }),
        KeyCode::LeftBracket => Some(if shift { b'{' } else { b'[' }),
        KeyCode::RightBracket => Some(if shift { b'}' } else { b']' }),
        KeyCode::Backslash => Some(if shift { b'|' } else { b'\\' }),
        KeyCode::Semicolon => Some(if shift { b':' } else { b';' }),
        KeyCode::Apostrophe => Some(if shift { b'"' } else { b'\'' }),
        KeyCode::Grave => Some(if shift { b'~' } else { b'`' }),
        KeyCode::Comma => Some(if shift { b'<' } else { b',' }),
        KeyCode::Period => Some(if shift { b'>' } else { b'.' }),
        KeyCode::Slash => Some(if shift { b'?' } else { b'/' }),

        _ => None,
    }
}

pub static KEYBOARD_DECODER: SpinLock<KeyboardDecoder> = SpinLock::new(KeyboardDecoder::new());

/// Initialize PS/2 Keyboard on IRQ1 (Vector 33)
pub fn init() {
    crate::log_info!("[KEYBOARD] Initializing PS/2 Keyboard via i8042 controller...");

    // 1. Drain any pending output bytes in i8042 buffer
    while unsafe { inb(PS2_STATUS_PORT) & 0x01 } != 0 {
        let _ = unsafe { inb(PS2_DATA_PORT) };
    }

    // 2. Enable keyboard port (command 0xAE)
    if wait_controller_ready() {
        unsafe {
            outb(PS2_COMMAND_PORT, 0xAE);
            io_wait();
        }
    }

    // 3. Read Controller Configuration Byte (command 0x20)
    if wait_controller_ready() {
        unsafe {
            outb(PS2_COMMAND_PORT, 0x20);
            io_wait();
        }
        if wait_controller_output() {
            let mut config = unsafe { inb(PS2_DATA_PORT) };
            // Bit 0 = Port 1 (Keyboard) interrupt enabled
            // Bit 6 = Port 1 translation enabled (scancode set 1 emulation)
            config |= 0x01 | 0x40;

            if wait_controller_ready() {
                unsafe {
                    outb(PS2_COMMAND_PORT, 0x60);
                    io_wait();
                    outb(PS2_DATA_PORT, config);
                    io_wait();
                }
            }
        }
    }

    // 4. Reset & enable scanning on keyboard device (command 0xF4)
    if wait_controller_ready() {
        unsafe {
            outb(PS2_DATA_PORT, 0xF4);
            io_wait();
        }
    }

    // 5. Unmask IRQ1 on Master PIC
    unmask_irq(1);

    crate::log_info!("[KEYBOARD] PS/2 Keyboard initialized, IRQ1 unmasked.");
}

/// Handle Keyboard Interrupt (IRQ1 -> Vector 33)
pub fn handle_interrupt() {
    let scancode = unsafe { inb(PS2_DATA_PORT) };
    send_eoi_irq(1);

    let mut decoder = KEYBOARD_DECODER.lock();
    if let Some(event) = decoder.process_byte(scancode) {
        post_event(event);

        if let InputEvent::KeyDown { ascii: Some(ch), .. } = event {
            push_char(ch);
        }
    }
}
