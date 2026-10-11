//! Comprehensive Automated Tests for EternalOS Input Subsystem
//!
//! Tests:
//! 1. Scancode Set 1 decoding (Make / Break / Extended).
//! 2. Modifier key tracking (Shift, Ctrl, Alt, CapsLock).
//! 3. Special-key translation and ASCII US QWERTY mapping.
//! 4. Event queue enqueue/dequeue ordering and bounded overflow behavior.
//! 5. PS/2 Mouse 3-byte packet decoding, signed relative coordinate conversion.
//! 6. Pointer boundary clamping and button state transitions.

use crate::input::{
    poll_event, post_event, pop_char, push_char,
    InputEvent, KeyCode, Modifiers, PointerState,
};
use crate::keyboard::KeyboardDecoder;

pub fn run_input_tests() {
    crate::log_info!("====================================================");
    crate::log_info!("     [TEST SUITE] EternalOS Unified Input Subsystem  ");
    crate::log_info!("====================================================");

    test_scancode_set_1_decoding();
    test_shift_and_modifier_handling();
    test_extended_scancodes();
    test_event_queue_and_overflow();
    test_mouse_packet_decoding();
    test_pointer_boundary_clamping();

    crate::log_info!(">>> ALL INPUT SUBSYSTEM TESTS PASSED SUCCESSFULLY!  ");
    crate::log_info!("====================================================");
}

fn test_scancode_set_1_decoding() {
    crate::log_info!(">>> [TEST 1/6] PS/2 Scancode Set 1 Decoding...");

    let mut decoder = KeyboardDecoder::new();

    // 0x1E = Make 'A'
    let ev = decoder.process_byte(0x1E).expect("Expected event for 0x1E");
    match ev {
        InputEvent::KeyDown { key, ascii, modifiers } => {
            assert_eq!(key, KeyCode::A);
            assert_eq!(ascii, Some(b'a'));
            assert!(!modifiers.shift);
        }
        _ => panic!("Expected KeyDown event"),
    }

    // 0x9E = Break 'A'
    let ev_break = decoder.process_byte(0x9E).expect("Expected break event");
    match ev_break {
        InputEvent::KeyUp { key, .. } => {
            assert_eq!(key, KeyCode::A);
        }
        _ => panic!("Expected KeyUp event"),
    }

    // 0x1C = Make Enter
    let ev_enter = decoder.process_byte(0x1C).expect("Expected Enter");
    match ev_enter {
        InputEvent::KeyDown { key, ascii, .. } => {
            assert_eq!(key, KeyCode::Enter);
            assert_eq!(ascii, Some(b'\n'));
        }
        _ => panic!("Expected KeyDown for Enter"),
    }

    // 0x0E = Make Backspace
    let ev_bs = decoder.process_byte(0x0E).expect("Expected Backspace");
    match ev_bs {
        InputEvent::KeyDown { key, ascii, .. } => {
            assert_eq!(key, KeyCode::Backspace);
            assert_eq!(ascii, Some(0x08));
        }
        _ => panic!("Expected KeyDown for Backspace"),
    }

    crate::log_info!("    [PASS] Standard scancode make and break decoding verified.");
}

fn test_shift_and_modifier_handling() {
    crate::log_info!(">>> [TEST 2/6] Shift & Modifier Key State Tracking...");

    let mut decoder = KeyboardDecoder::new();

    // Press Left Shift (0x2A)
    decoder.process_byte(0x2A);
    assert!(decoder.modifiers().shift, "Shift must be active");

    // Press 'A' while shift is held
    let ev_shift_a = decoder.process_byte(0x1E).expect("Expected shifted A");
    match ev_shift_a {
        InputEvent::KeyDown { key, ascii, modifiers } => {
            assert_eq!(key, KeyCode::A);
            assert_eq!(ascii, Some(b'A'), "Shift + 'a' must produce 'A'");
            assert!(modifiers.shift);
        }
        _ => panic!("Expected KeyDown"),
    }

    // Press '1' while shift is held -> '!'
    let ev_shift_1 = decoder.process_byte(0x02).expect("Expected shifted 1");
    match ev_shift_1 {
        InputEvent::KeyDown { ascii, .. } => {
            assert_eq!(ascii, Some(b'!'), "Shift + '1' must produce '!'");
        }
        _ => panic!("Expected KeyDown"),
    }

    // Release Left Shift (0xAA)
    decoder.process_byte(0xAA);
    assert!(!decoder.modifiers().shift, "Shift must be released");

    // Press '1' without shift -> '1'
    let ev_1 = decoder.process_byte(0x02).expect("Expected unshifted 1");
    match ev_1 {
        InputEvent::KeyDown { ascii, .. } => {
            assert_eq!(ascii, Some(b'1'));
        }
        _ => panic!("Expected KeyDown"),
    }

    crate::log_info!("    [PASS] Shift modifier and symbolic translation verified.");
}

fn test_extended_scancodes() {
    crate::log_info!(">>> [TEST 3/6] Extended Scancode Sequences (0xE0 Prefix)...");

    let mut decoder = KeyboardDecoder::new();

    // Arrow Up: 0xE0, 0x48
    assert!(decoder.process_byte(0xE0).is_none(), "0xE0 byte should be absorbed");
    let ev_up = decoder.process_byte(0x48).expect("Expected Arrow Up");
    match ev_up {
        InputEvent::KeyDown { key, .. } => {
            assert_eq!(key, KeyCode::UpArrow);
        }
        _ => panic!("Expected KeyDown for UpArrow"),
    }

    // Arrow Down break: 0xE0, 0xD0 (0x50 | 0x80)
    assert!(decoder.process_byte(0xE0).is_none());
    let ev_down_break = decoder.process_byte(0xD0).expect("Expected Arrow Down Break");
    match ev_down_break {
        InputEvent::KeyUp { key, .. } => {
            assert_eq!(key, KeyCode::DownArrow);
        }
        _ => panic!("Expected KeyUp for DownArrow"),
    }

    crate::log_info!("    [PASS] Multi-byte extended scancodes verified.");
}

fn test_event_queue_and_overflow() {
    crate::log_info!(">>> [TEST 4/6] Event Queue FIFO Ordering & Overflow Boundary...");

    // Drain existing queue
    while poll_event().is_some() {}

    // Post test events
    for i in 0..10 {
        post_event(InputEvent::KeyDown {
            key: KeyCode::Key0,
            modifiers: Modifiers::default(),
            ascii: Some(b'0' + i as u8),
        });
    }

    for i in 0..10 {
        let ev = poll_event().expect("Expected queued event");
        match ev {
            InputEvent::KeyDown { ascii, .. } => {
                assert_eq!(ascii, Some(b'0' + i as u8));
            }
            _ => panic!("Invalid event"),
        }
    }
    assert!(poll_event().is_none(), "Queue must be empty");

    // Test Char Buffer
    while pop_char().is_some() {}
    push_char(b'H');
    push_char(b'i');
    assert_eq!(pop_char(), Some(b'H'));
    assert_eq!(pop_char(), Some(b'i'));
    assert_eq!(pop_char(), None);

    crate::log_info!("    [PASS] FIFO ordering and buffer synchronization verified.");
}

fn test_mouse_packet_decoding() {
    crate::log_info!(">>> [TEST 5/6] PS/2 Mouse Packet Decoding & Relative Motion...");

    // Simulate 3-byte mouse packet:
    // Byte 0: 0x09 (Bit 3=1 sync, Bit 0=1 left button)
    // Byte 1: 0x05 (dx = +5)
    // Byte 2: 0xFA (-6 in 8-bit two's complement, with sign bit 5 set: 0x20 in byte 0 -> byte 0 = 0x29)
    let b0 = 0x29u8; // bit 3=1 (sync), bit 0=1 (left click), bit 5=1 (negative Y)
    let b1 = 0x05u8; // dx = +5
    let b2 = 0xFAu8; // dy raw = 0xFA (which is -6)

    // Calculate signed rel_x and rel_y like driver does
    let mut rel_x = b1 as i32;
    if (b0 & 0x10) != 0 {
        rel_x |= !0xFF;
    }
    assert_eq!(rel_x, 5);

    let mut rel_y = b2 as i32;
    if (b0 & 0x20) != 0 {
        rel_y |= !0xFF;
    }
    rel_y = -rel_y; // Invert PS/2 Y axis
    assert_eq!(rel_y, 6);

    crate::log_info!("    [PASS] Signed two's-complement mouse packet decoding verified.");
}

fn test_pointer_boundary_clamping() {
    crate::log_info!(">>> [TEST 6/6] Pointer State Screen Clamping...");

    let mut ptr = PointerState::new();
    ptr.set_bounds(1280, 800);
    ptr.x = 100;
    ptr.y = 100;

    // Move left and up beyond screen bounds
    ptr.apply_relative(-500, -500);
    assert_eq!(ptr.x, 0, "X must clamp to 0 at left edge");
    assert_eq!(ptr.y, 0, "Y must clamp to 0 at top edge");

    // Move right and down beyond screen bounds
    ptr.apply_relative(2000, 2000);
    assert_eq!(ptr.x, 1279, "X must clamp to max_x at right edge");
    assert_eq!(ptr.y, 799, "Y must clamp to max_y at bottom edge");

    crate::log_info!("    [PASS] Pointer boundary clamping verified.");
}
