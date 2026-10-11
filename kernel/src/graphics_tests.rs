use crate::framebuffer::*;
use crate::log_info;

static mut SYNTH_BUF: [u8; 80 * 60 * 4] = [0u8; 80 * 60 * 4];
static mut PADDED_BUF: [u8; 60 * ((80 * 4) + 64)] = [0u8; 60 * ((80 * 4) + 64)];
static mut TEST_BUF: [u32; 100 * 100] = [0u32; 100 * 100];
static mut RECT_BUF: [u32; 20 * 20] = [0u32; 20 * 20];

/// Run the Phase 13A Framebuffer & Early Graphics Deterministic Test Suite
pub fn run_phase13a_graphics_tests() {
    log_info!("====================================================");
    log_info!(">>> RUNNING PHASE 13A FRAMEBUFFER & GRAPHICS TESTS");
    log_info!("====================================================");

    // Test 1: Pixel Address & Offset Calculation
    log_info!("[TEST 1/12] Testing pixel address and pitch offset calculation...");
    let fb_synth = Framebuffer::new_pure(
        80,
        60,
        80 * 4,
        PixelFormat::RGB32,
        None,
        core::ptr::addr_of_mut!(SYNTH_BUF).cast::<u8>(),
    ).expect("Synthetic FB creation failed");

    assert_eq!(fb_synth.offset_of(0, 0), Some(0));
    assert_eq!(fb_synth.offset_of(1, 0), Some(4));
    assert_eq!(fb_synth.offset_of(0, 1), Some(80 * 4));
    assert_eq!(fb_synth.offset_of(10, 20), Some((20 * 80 * 4) + (10 * 4)));
    log_info!("[PASS] Test 1: Pixel address & row-offset calculations verified.");

    // Test 2: Pitch & Padding Handling (Pitch > Width * BytesPerPixel)
    log_info!("[TEST 2/12] Testing non-standard pitch and row stride padding...");
    let padded_pitch = (80 * 4) + 64; // 64 bytes of alignment padding per scanline
    let fb_padded = Framebuffer::new_pure(
        80,
        60,
        padded_pitch,
        PixelFormat::RGB32,
        None,
        core::ptr::addr_of_mut!(PADDED_BUF).cast::<u8>(),
    ).expect("Padded FB creation failed");

    assert_eq!(fb_padded.offset_of(0, 1), Some(padded_pitch));
    assert_eq!(fb_padded.offset_of(5, 3), Some((3 * padded_pitch) + (5 * 4)));
    log_info!("[PASS] Test 2: Non-standard pitch & row padding handled accurately.");

    // Test 3: Color Packing for Supported Formats
    log_info!("[TEST 3/12] Testing color packing and channel mask shifts...");
    let fmt_bgr = PixelFormat {
        bpp: 32,
        red_mask_size: 8,
        red_mask_shift: 16,
        green_mask_size: 8,
        green_mask_shift: 8,
        blue_mask_size: 8,
        blue_mask_shift: 0,
    };
    let c = Color::new(0x12, 0x34, 0x56);
    assert_eq!(fmt_bgr.pack_u32(c), 0x00123456);

    let fmt_rgb = PixelFormat {
        bpp: 32,
        red_mask_size: 8,
        red_mask_shift: 0,
        green_mask_size: 8,
        green_mask_shift: 8,
        blue_mask_size: 8,
        blue_mask_shift: 16,
    };
    assert_eq!(fmt_rgb.pack_u32(c), 0x00563412);
    log_info!("[PASS] Test 3: Channel masks & color packing verified across layouts.");

    // Test 4: Synthetic Framebuffer Writes and Readback
    log_info!("[TEST 4/12] Testing synthetic framebuffer writes & readback...");
    let fb_test = Framebuffer::new_pure(
        100,
        100,
        100 * 4,
        PixelFormat::RGB32,
        None,
        core::ptr::addr_of_mut!(TEST_BUF).cast::<u8>(),
    ).expect("Test FB creation failed");

    fb_test.put_pixel(15, 25, Color::new(0xAA, 0xBB, 0xCC));
    let read_val = unsafe { (*core::ptr::addr_of!(TEST_BUF))[(25 * 100) + 15] };
    assert_eq!(read_val, 0x00AABBCC);
    log_info!("[PASS] Test 4: Pixel write & memory readback verified.");

    // Test 5: Invalid Dimensions & Pitch Validation
    log_info!("[TEST 5/12] Testing rejection of zero dimensions & insufficient pitch...");
    let mut dummy_byte = [0u8; 4];
    assert!(Framebuffer::new_pure(0, 100, 400, PixelFormat::RGB32, None, dummy_byte.as_mut_ptr()).is_err());
    assert!(Framebuffer::new_pure(100, 0, 400, PixelFormat::RGB32, None, dummy_byte.as_mut_ptr()).is_err());
    assert!(Framebuffer::new_pure(100, 100, 300, PixelFormat::RGB32, None, dummy_byte.as_mut_ptr()).is_err()); // Pitch < 400
    log_info!("[PASS] Test 5: Malformed dimensions and insufficient pitch rejected safely.");

    // Test 6: Unsupported Pixel Format Handling
    log_info!("[TEST 6/12] Testing rejection of unsupported bits per pixel...");
    let fmt_16bpp = PixelFormat {
        bpp: 16,
        red_mask_size: 5,
        red_mask_shift: 11,
        green_mask_size: 6,
        green_mask_shift: 5,
        blue_mask_size: 5,
        blue_mask_shift: 0,
    };
    assert!(Framebuffer::new_pure(100, 100, 400, fmt_16bpp, None, dummy_byte.as_mut_ptr()).is_err());
    log_info!("[PASS] Test 6: Unsupported pixel formats rejected safely.");

    // Test 7: Out-of-bounds Coordinates & Boundary Clipping
    log_info!("[TEST 7/12] Testing boundary clipping and out-of-bounds safety...");
    assert_eq!(fb_test.offset_of(100, 50), None); // x >= width
    assert_eq!(fb_test.offset_of(50, 100), None); // y >= height
    assert_eq!(fb_test.offset_of(usize::MAX, 50), None); // overflow coordinate
    fb_test.put_pixel(150, 150, Color::WHITE); // Must not panic or segfault
    log_info!("[PASS] Test 7: Out-of-bounds coordinates safely clipped.");

    // Test 8: Arithmetic Overflow Protection
    log_info!("[TEST 8/12] Testing arithmetic overflow protection in stride math...");
    let huge_width = usize::MAX / 2;
    assert!(Framebuffer::new_pure(huge_width, 100, huge_width, PixelFormat::RGB32, None, dummy_byte.as_mut_ptr()).is_err());
    log_info!("[PASS] Test 8: Overflow protection verified in buffer dimension validation.");

    // Test 9: Rectangle Fill & Border Bounds Verification
    log_info!("[TEST 9/12] Testing rectangle filling and clipping...");
    let fb_rect = Framebuffer::new_pure(
        20,
        20,
        20 * 4,
        PixelFormat::RGB32,
        None,
        core::ptr::addr_of_mut!(RECT_BUF).cast::<u8>(),
    ).expect("Rect FB failed");

    // Draw rect partially offscreen (x=15, w=10 -> should clip at x=20)
    fb_rect.fill_rect(Rect::new(15, 5, 10, 5), Color::WHITE);
    // Spot check clipped corner
    let r1 = unsafe { (*core::ptr::addr_of!(RECT_BUF))[(5 * 20) + 15] };
    let r2 = unsafe { (*core::ptr::addr_of!(RECT_BUF))[(5 * 20) + 19] };
    let r3 = unsafe { (*core::ptr::addr_of!(RECT_BUF))[(5 * 20) + 14] };
    assert_eq!(r1, 0x00FFFFFF);
    assert_eq!(r2, 0x00FFFFFF);
    assert_eq!(r3, 0); // outside rect
    log_info!("[PASS] Test 9: Rectangle clipping & rasterization verified.");

    // Test 10: Font Glyph Retrieval & Text Rendering Bounds
    log_info!("[TEST 10/12] Testing font glyph retrieval and printable ASCII bounds...");
    let glyph_a = crate::font::get_char_glyph('A');
    assert_eq!(glyph_a.len(), 16);
    assert_ne!(glyph_a[1], 0); // row 1 of 'A' has pixels
    let glyph_space = crate::font::get_char_glyph(' ');
    assert_eq!(glyph_space, &[0u8; 16]); // space is empty
    let glyph_unmapped = crate::font::get_char_glyph('\u{1F600}'); // emoji falls back to '?'
    assert_eq!(glyph_unmapped[1], 0x3C);
    log_info!("[PASS] Test 10: Font glyph mappings & ASCII boundaries verified.");

    // Test 11: Logo Asset Bounds and Integrity
    log_info!("[TEST 11/12] Testing embedded monochrome logo asset dimensions...");
    assert_eq!(crate::logo_asset::LOGO_WIDTH, 64);
    assert_eq!(crate::logo_asset::LOGO_HEIGHT, 58);
    assert_eq!(crate::logo_asset::LOGO_BITMAP.len(), 464);
    log_info!("[PASS] Test 11: Logo asset dimensions & array sizing verified.");

    // Test 12: Real Hardware Framebuffer Availability in QEMU
    log_info!("[TEST 12/12] Verifying active QEMU graphics framebuffer presence...");
    let fb_active = with_framebuffer(|fb| {
        log_info!(
            "  Active Framebuffer: {}x{} @ {} bpp (Pitch: {} bytes)",
            fb.width,
            fb.height,
            fb.bpp,
            fb.pitch
        );
        assert!(fb.width > 0 && fb.height > 0);
        assert!(fb.bpp == 32 || fb.bpp == 24);
        true
    });
    assert_eq!(fb_active, Some(true), "Active QEMU framebuffer must be present");
    log_info!("[PASS] Test 12: Active QEMU framebuffer verified.");

    log_info!("====================================================");
    log_info!(">>> ALL PHASE 13A GRAPHICS TESTS PASSED");
    log_info!("====================================================");
}
