//! Unified Input Event Subsystem for EternalOS
//!
//! Provides structured input event definitions (`InputEvent`, `KeyCode`),
//! an interrupt-safe bounded lock-free/spin-locked event queue, modifier state tracking,
//! and pointer state coordination.

use crate::sync::SpinLock;

/// Standard key codes independent of hardware scancodes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCode {
    Unknown,
    // Letters
    A, B, C, D, E, F, G, H, I, J, K, L, M,
    N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
    // Numbers
    Key0, Key1, Key2, Key3, Key4, Key5, Key6, Key7, Key8, Key9,
    // Functional keys
    Escape,
    Enter,
    Backspace,
    Tab,
    Space,
    // Modifiers
    LeftShift,
    RightShift,
    LeftCtrl,
    RightCtrl,
    LeftAlt,
    RightAlt,
    CapsLock,
    // Navigation & Editing
    UpArrow,
    DownArrow,
    LeftArrow,
    RightArrow,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Delete,
    // Punctuation & Symbols
    Minus,
    Equals,
    LeftBracket,
    RightBracket,
    Backslash,
    Semicolon,
    Apostrophe,
    Grave,
    Comma,
    Period,
    Slash,
    // Function keys
    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
}

/// Bitflags for active modifier keys
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub caps_lock: bool,
}

/// Mouse buttons
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Other(u8),
}

/// Unified Input Event
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEvent {
    KeyDown {
        key: KeyCode,
        modifiers: Modifiers,
        ascii: Option<u8>,
    },
    KeyUp {
        key: KeyCode,
        modifiers: Modifiers,
    },
    PointerMove {
        rel_x: i32,
        rel_y: i32,
        abs_x: usize,
        abs_y: usize,
    },
    PointerButtonDown {
        button: MouseButton,
        abs_x: usize,
        abs_y: usize,
    },
    PointerButtonUp {
        button: MouseButton,
        abs_x: usize,
        abs_y: usize,
    },
    Scroll {
        delta: i32,
    },
}

/// Bounded queue capacity
pub const EVENT_QUEUE_CAPACITY: usize = 256;

pub struct InputEventQueue {
    buffer: [Option<InputEvent>; EVENT_QUEUE_CAPACITY],
    head: usize,
    tail: usize,
    count: usize,
    dropped_events: usize,
}

impl InputEventQueue {
    pub const fn new() -> Self {
        Self {
            buffer: [None; EVENT_QUEUE_CAPACITY],
            head: 0,
            tail: 0,
            count: 0,
            dropped_events: 0,
        }
    }

    pub fn push(&mut self, event: InputEvent) -> bool {
        if self.count >= EVENT_QUEUE_CAPACITY {
            self.dropped_events = self.dropped_events.saturating_add(1);
            return false;
        }
        self.buffer[self.tail] = Some(event);
        self.tail = (self.tail + 1) % EVENT_QUEUE_CAPACITY;
        self.count += 1;
        true
    }

    pub fn pop(&mut self) -> Option<InputEvent> {
        if self.count == 0 {
            return None;
        }
        let ev = self.buffer[self.head].take();
        self.head = (self.head + 1) % EVENT_QUEUE_CAPACITY;
        self.count -= 1;
        ev
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn dropped_count(&self) -> usize {
        self.dropped_events
    }
}

pub static EVENT_QUEUE: SpinLock<InputEventQueue> = SpinLock::new(InputEventQueue::new());

// Character buffer specifically for console stdin reading (keystrokes + serial)
pub const CHAR_BUFFER_CAPACITY: usize = 256;

pub struct CharBuffer {
    buffer: [u8; CHAR_BUFFER_CAPACITY],
    head: usize,
    tail: usize,
    count: usize,
}

impl CharBuffer {
    pub const fn new() -> Self {
        Self {
            buffer: [0; CHAR_BUFFER_CAPACITY],
            head: 0,
            tail: 0,
            count: 0,
        }
    }

    pub fn push(&mut self, b: u8) -> bool {
        if self.count >= CHAR_BUFFER_CAPACITY {
            return false;
        }
        self.buffer[self.tail] = b;
        self.tail = (self.tail + 1) % CHAR_BUFFER_CAPACITY;
        self.count += 1;
        true
    }

    pub fn pop(&mut self) -> Option<u8> {
        if self.count == 0 {
            return None;
        }
        let b = self.buffer[self.head];
        self.head = (self.head + 1) % CHAR_BUFFER_CAPACITY;
        self.count -= 1;
        Some(b)
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}

pub static CHAR_QUEUE: SpinLock<CharBuffer> = SpinLock::new(CharBuffer::new());

/// Global pointer tracking state
pub struct PointerState {
    pub x: usize,
    pub y: usize,
    pub max_x: usize,
    pub max_y: usize,
    pub left_button: bool,
    pub right_button: bool,
    pub middle_button: bool,
}

impl PointerState {
    pub const fn new() -> Self {
        Self {
            x: 640,
            y: 400,
            max_x: 1280,
            max_y: 800,
            left_button: false,
            right_button: false,
            middle_button: false,
        }
    }

    pub fn set_bounds(&mut self, max_x: usize, max_y: usize) {
        self.max_x = max_x.saturating_sub(1);
        self.max_y = max_y.saturating_sub(1);
        if self.x > self.max_x {
            self.x = self.max_x;
        }
        if self.y > self.max_y {
            self.y = self.max_y;
        }
    }

    pub fn apply_relative(&mut self, dx: i32, dy: i32) {
        let new_x = (self.x as i64) + (dx as i64);
        let new_y = (self.y as i64) + (dy as i64);

        self.x = if new_x < 0 {
            0
        } else if (new_x as usize) > self.max_x {
            self.max_x
        } else {
            new_x as usize
        };

        self.y = if new_y < 0 {
            0
        } else if (new_y as usize) > self.max_y {
            self.max_y
        } else {
            new_y as usize
        };
    }
}

pub static POINTER: SpinLock<PointerState> = SpinLock::new(PointerState::new());

/// Push an input event to the queue
pub fn post_event(event: InputEvent) {
    let mut q = EVENT_QUEUE.lock();
    q.push(event);
}

/// Pop an input event from the queue
pub fn poll_event() -> Option<InputEvent> {
    let mut q = EVENT_QUEUE.lock();
    q.pop()
}

/// Feed a character into the console input buffer
pub fn push_char(b: u8) {
    let mut q = CHAR_QUEUE.lock();
    q.push(b);
}

/// Fetch a character from the console input buffer
pub fn pop_char() -> Option<u8> {
    let mut q = CHAR_QUEUE.lock();
    q.pop()
}
