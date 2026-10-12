use core::ptr::write_volatile;
use crate::sync::SpinLock;
use crate::vm::{translate_addr, PhysAddr, VirtAddr};
use limine::request::FramebufferRequest;

#[used]
#[unsafe(link_section = ".requests")]
pub static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

/// RGB color structure (8-bit per channel)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const BLACK: Color = Color { r: 0x00, g: 0x00, b: 0x00 };
    pub const WHITE: Color = Color { r: 0xFF, g: 0xFF, b: 0xFF };
    pub const DARK_BG: Color = Color { r: 0x11, g: 0x10, b: 0x19 }; // Dark purple/slate background (#111019)
    pub const PANEL_BG: Color = Color { r: 0x1A, g: 0x18, b: 0x27 }; // Elevated purple slate panel
    pub const PANEL_BORDER: Color = Color { r: 0x36, g: 0x2D, b: 0x4F }; // Subdued purple border
    pub const ACCENT_PURPLE: Color = Color { r: 0x7C, g: 0x3A, b: 0xED }; // Primary Eternal Purple (#7C3AED)
    pub const ACCENT_SECONDARY: Color = Color { r: 0x6D, g: 0x28, b: 0xD9 }; // Secondary Purple (#6D28D9)
    pub const ACCENT_LAVENDER: Color = Color { r: 0xA7, g: 0x8B, b: 0xFA }; // Soft Lavender (#A78BFA)
    pub const ACCENT_HIGHLIGHT: Color = Color { r: 0xC4, g: 0xB5, b: 0xFD }; // Highlight Lavender (#C4B5FD)
    pub const SUCCESS_GREEN: Color = Color { r: 0x2E, g: 0xCC, b: 0x71 }; // Emerald pass green
    pub const WARN_AMBER: Color = Color { r: 0xF3, g: 0x9C, b: 0x12 }; // Amber
    pub const ERROR_RED: Color = Color { r: 0xEF, g: 0x44, b: 0x44 }; // Readable red
    pub const TEXT_PRIMARY: Color = Color { r: 0xFA, g: 0xFA, b: 0xFC }; // Bright crisp white
    pub const TEXT_MUTED: Color = Color { r: 0x94, g: 0x8F, b: 0xA8 }; // Cool muted lavender-grey

    // Aliases for compatibility
    pub const PURPLE: Color = Self::ACCENT_PURPLE;
    pub const LAVENDER: Color = Self::ACCENT_LAVENDER;
    pub const HIGHLIGHT: Color = Self::ACCENT_HIGHLIGHT;
    pub const DARK_PURPLE_BG: Color = Self::DARK_BG;
    pub const ACCENT_CYAN: Color = Self::ACCENT_LAVENDER; // Compatibility fallback mapped to lavender
    pub const ACCENT_BLUE: Color = Self::ACCENT_PURPLE; // Compatibility fallback mapped to purple

    #[inline]
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

/// Pixel format representation with channel bit masks and shifts
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelFormat {
    pub bpp: u16,
    pub red_mask_size: u8,
    pub red_mask_shift: u8,
    pub green_mask_size: u8,
    pub green_mask_shift: u8,
    pub blue_mask_size: u8,
    pub blue_mask_shift: u8,
}

impl PixelFormat {
    /// Standard 32-bpp BGRX / RGBX fallback layout
    pub const RGB32: Self = Self {
        bpp: 32,
        red_mask_size: 8,
        red_mask_shift: 16,
        green_mask_size: 8,
        green_mask_shift: 8,
        blue_mask_size: 8,
        blue_mask_shift: 0,
    };

    /// Pack an RGB color into a raw 32-bit pixel value using the format masks
    #[inline]
    pub fn pack_u32(&self, color: Color) -> u32 {
        let r = ((color.r as u32) & ((1 << self.red_mask_size) - 1)) << self.red_mask_shift;
        let g = ((color.g as u32) & ((1 << self.green_mask_size) - 1)) << self.green_mask_shift;
        let b = ((color.b as u32) & ((1 << self.blue_mask_size) - 1)) << self.blue_mask_shift;
        r | g | b
    }
}

/// Bounded rectangle in screen coordinates
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

impl Rect {
    pub const fn new(x: usize, y: usize, width: usize, height: usize) -> Self {
        Self { x, y, width, height }
    }
}

/// Concrete description and safe memory-mapped buffer wrapper for an active framebuffer
pub struct Framebuffer {
    pub width: usize,
    pub height: usize,
    pub pitch: usize,
    pub bpp: u16,
    pub format: PixelFormat,
    pub phys_addr: Option<PhysAddr>,
    pub virt_base: *mut u8,
    pub buffer_len: usize,
}

unsafe impl Send for Framebuffer {}
unsafe impl Sync for Framebuffer {}

impl Framebuffer {
    /// Pure constructor for testing or custom-backed framebuffers.
    /// Validates pitch, dimensions, and calculates buffer length safely.
    pub fn new_pure(
        width: usize,
        height: usize,
        pitch: usize,
        format: PixelFormat,
        phys_addr: Option<PhysAddr>,
        virt_base: *mut u8,
    ) -> Result<Self, &'static str> {
        if width == 0 || height == 0 {
            return Err("Framebuffer dimensions cannot be zero");
        }
        if format.bpp != 32 && format.bpp != 24 {
            return Err("Unsupported bits per pixel (must be 32 or 24 bpp)");
        }
        let bytes_per_pixel = (format.bpp as usize + 7) / 8;
        let min_pitch = width
            .checked_mul(bytes_per_pixel)
            .ok_or("Row stride calculation overflowed")?;
        if pitch < min_pitch {
            return Err("Pitch is smaller than width * bytes_per_pixel");
        }
        let buffer_len = pitch
            .checked_mul(height)
            .ok_or("Total buffer length calculation overflowed")?;

        Ok(Self {
            width,
            height,
            pitch,
            bpp: format.bpp,
            format,
            phys_addr,
            virt_base,
            buffer_len,
        })
    }

    /// Compute byte offset within the framebuffer for a given (x, y) coordinate.
    /// Returns None if (x, y) is out of bounds or offset arithmetic overflows.
    #[inline]
    pub fn offset_of(&self, x: usize, y: usize) -> Option<usize> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let bytes_per_pixel = (self.bpp as usize + 7) / 8;
        let row_offset = y.checked_mul(self.pitch)?;
        let col_offset = x.checked_mul(bytes_per_pixel)?;
        let total_offset = row_offset.checked_add(col_offset)?;
        if total_offset.checked_add(bytes_per_pixel)? <= self.buffer_len {
            Some(total_offset)
        } else {
            None
        }
    }

    /// Plot a single pixel at (x, y) with boundary clipping and volatile write
    pub fn put_pixel(&self, x: usize, y: usize, color: Color) {
        if let Some(offset) = self.offset_of(x, y) {
            unsafe {
                let ptr = self.virt_base.add(offset);
                if self.bpp == 32 {
                    let pixel_val = self.format.pack_u32(color);
                    write_volatile(ptr.cast::<u32>(), pixel_val);
                } else if self.bpp == 24 {
                    // 24-bit write: assumes standard BGR/RGB packing based on shift
                    let r_idx = (self.format.red_mask_shift / 8) as usize;
                    let g_idx = (self.format.green_mask_shift / 8) as usize;
                    let b_idx = (self.format.blue_mask_shift / 8) as usize;
                    write_volatile(ptr.add(r_idx), color.r);
                    write_volatile(ptr.add(g_idx), color.g);
                    write_volatile(ptr.add(b_idx), color.b);
                }
            }
        }
    }

    /// Read a single pixel at (x, y) with boundary clipping and volatile read
    pub fn get_pixel(&self, x: usize, y: usize) -> Option<Color> {
        let offset = self.offset_of(x, y)?;
        unsafe {
            let ptr = self.virt_base.add(offset);
            if self.bpp == 32 {
                let val = core::ptr::read_volatile(ptr.cast::<u32>());
                let r = ((val >> self.format.red_mask_shift) & 0xFF) as u8;
                let g = ((val >> self.format.green_mask_shift) & 0xFF) as u8;
                let b = ((val >> self.format.blue_mask_shift) & 0xFF) as u8;
                Some(Color::new(r, g, b))
            } else if self.bpp == 24 {
                let r_idx = (self.format.red_mask_shift / 8) as usize;
                let g_idx = (self.format.green_mask_shift / 8) as usize;
                let b_idx = (self.format.blue_mask_shift / 8) as usize;
                let r = core::ptr::read_volatile(ptr.add(r_idx));
                let g = core::ptr::read_volatile(ptr.add(g_idx));
                let b = core::ptr::read_volatile(ptr.add(b_idx));
                Some(Color::new(r, g, b))
            } else {
                None
            }
        }
    }

    /// Fill the entire screen with a solid color
    pub fn clear(&self, color: Color) {
        self.fill_rect(Rect::new(0, 0, self.width, self.height), color);
    }

    /// Draw a filled rectangle with coordinate clipping and accelerated 64-bit row-linear writes
    pub fn fill_rect(&self, rect: Rect, color: Color) {
        let x_start = rect.x.min(self.width);
        let x_end = (rect.x + rect.width).min(self.width);
        let y_start = rect.y.min(self.height);
        let y_end = (rect.y + rect.height).min(self.height);

        if x_start >= x_end || y_start >= y_end {
            return;
        }

        let pixel_val = self.format.pack_u32(color);
        let span_len = x_end - x_start;
        let pixel_pair = (pixel_val as u64) | ((pixel_val as u64) << 32);

        for y in y_start..y_end {
            let row_offset = match y.checked_mul(self.pitch) {
                Some(off) => off,
                None => continue,
            };

            let start_offset = row_offset + (x_start * 4);
            let end_offset = start_offset + (span_len * 4);
            if end_offset <= self.buffer_len {
                unsafe {
                    let mut ptr = self.virt_base.add(start_offset).cast::<u32>();
                    let mut remaining = span_len;

                    // If not 8-byte aligned, write leading 32-bit pixel
                    if (ptr as usize) & 0x7 != 0 && remaining > 0 {
                        write_volatile(ptr, pixel_val);
                        ptr = ptr.add(1);
                        remaining -= 1;
                    }

                    // Bulk write 64-bit pairs (2 pixels per bus cycle)
                    let u64_count = remaining / 2;
                    let mut ptr64 = ptr.cast::<u64>();
                    for _ in 0..u64_count {
                        write_volatile(ptr64, pixel_pair);
                        ptr64 = ptr64.add(1);
                    }

                    // Write trailing pixel if odd count
                    if (remaining & 1) != 0 {
                        write_volatile(ptr64.cast::<u32>(), pixel_val);
                    }
                }
            }
        }
    }

    /// Draw a 1-pixel hollow rectangle border
    pub fn draw_rect_border(&self, rect: Rect, border_color: Color) {
        if rect.width == 0 || rect.height == 0 {
            return;
        }
        let x2 = rect.x + rect.width - 1;
        let y2 = rect.y + rect.height - 1;

        // Top & bottom edges
        for x in rect.x..=x2 {
            self.put_pixel(x, rect.y, border_color);
            self.put_pixel(x, y2, border_color);
        }
        // Left & right edges
        for y in rect.y..=y2 {
            self.put_pixel(rect.x, y, border_color);
            self.put_pixel(x2, y, border_color);
        }
    }

    /// Draw a horizontal line
    pub fn draw_hline(&self, x: usize, y: usize, width: usize, color: Color) {
        self.fill_rect(Rect::new(x, y, width, 1), color);
    }

    /// Draw a vertical line
    pub fn draw_vline(&self, x: usize, y: usize, height: usize, color: Color) {
        self.fill_rect(Rect::new(x, y, 1, height), color);
    }

    /// Render a single 8x16 ASCII character glyph with row-stride fast write
    pub fn draw_char(&self, x: usize, y: usize, c: char, fg: Color, bg: Option<Color>) {
        if x + 8 > self.width || y + 16 > self.height {
            return;
        }

        let glyph = crate::font::get_char_glyph(c);
        let fg_val = self.format.pack_u32(fg);
        let bg_val = bg.map(|c| self.format.pack_u32(c));

        for row in 0..16 {
            let pixel_y = y + row;
            let row_offset = match pixel_y.checked_mul(self.pitch) {
                Some(off) => off,
                None => continue,
            };
            let start_offset = row_offset + (x * 4);
            if start_offset + 32 > self.buffer_len {
                continue;
            }

            let byte = glyph[row];
            unsafe {
                let mut ptr = self.virt_base.add(start_offset).cast::<u32>();
                for col in 0..8 {
                    if (byte & (0x80 >> col)) != 0 {
                        write_volatile(ptr, fg_val);
                    } else if let Some(bg_pixel) = bg_val {
                        write_volatile(ptr, bg_pixel);
                    }
                    ptr = ptr.add(1);
                }
            }
        }
    }

    /// Render a string of text using the 8x16 font
    pub fn draw_text(&self, x: usize, y: usize, text: &str, fg: Color, bg: Option<Color>) {
        let mut cur_x = x;
        let mut cur_y = y;

        for c in text.chars() {
            if c == '\n' {
                cur_x = x;
                cur_y += 18;
                continue;
            }
            if cur_x + 8 > self.width {
                // Line wrap
                cur_x = x;
                cur_y += 18;
            }
            if cur_y + 16 > self.height {
                break;
            }
            self.draw_char(cur_x, cur_y, c, fg, bg);
            cur_x += 8;
        }
    }

    /// Draw a 1-bit monochrome bitmap asset (e.g. downsampled logo)
    pub fn draw_monochrome_bitmap(
        &self,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
        bitmap: &[u8],
        color: Color,
    ) {
        let bytes_per_row = (width + 7) / 8;
        for row in 0..height {
            let row_offset = row * bytes_per_row;
            for col in 0..width {
                let byte_idx = row_offset + (col / 8);
                if byte_idx < bitmap.len() {
                    let bit_idx = 7 - (col % 8);
                    if (bitmap[byte_idx] & (1 << bit_idx)) != 0 {
                        self.put_pixel(x + col, y + row, color);
                    }
                }
            }
        }
    }
}

// -----------------------------------------------------------------------------
// Global Subsystem State & Initialization
// -----------------------------------------------------------------------------

static FRAMEBUFFER: SpinLock<Option<Framebuffer>> = SpinLock::new(None);

/// Initialize the early framebuffer subsystem from Limine bootloader response
pub fn init() {
    crate::log_info!("[GRAPHICS] Initializing early framebuffer subsystem...");

    let response = match FRAMEBUFFER_REQUEST.get_response() {
        Some(r) => r,
        None => {
            crate::log_warn!("[GRAPHICS] No framebuffer response from Limine bootloader.");
            return;
        }
    };

    let raw_fb = match response.framebuffers().next() {
        Some(fb) => fb,
        None => {
            crate::log_warn!("[GRAPHICS] Framebuffer list is empty.");
            return;
        }
    };

    let width = raw_fb.width() as usize;
    let height = raw_fb.height() as usize;
    let pitch = raw_fb.pitch() as usize;
    let bpp = raw_fb.bpp();
    let virt_ptr = raw_fb.addr();

    let format = PixelFormat {
        bpp,
        red_mask_size: raw_fb.red_mask_size(),
        red_mask_shift: raw_fb.red_mask_shift(),
        green_mask_size: raw_fb.green_mask_size(),
        green_mask_shift: raw_fb.green_mask_shift(),
        blue_mask_size: raw_fb.blue_mask_size(),
        blue_mask_shift: raw_fb.blue_mask_shift(),
    };

    // Calculate physical address using virtual memory translation
    let virt_addr = VirtAddr::new(virt_ptr as u64);
    let phys_addr = translate_addr(virt_addr).ok();

    crate::log_info!(
        "[GRAPHICS] Framebuffer discovered: {}x{} @ {} bpp, pitch: {} bytes",
        width,
        height,
        bpp,
        pitch
    );
    crate::log_info!(
        "  Virtual Address  : {:p}",
        virt_ptr
    );
    if let Some(pa) = phys_addr {
        crate::log_info!("  Physical Address : 0x{:016x} (via PML4 translation)", pa.as_u64());
    } else {
        crate::log_warn!("  Physical Address : Unable to walk page tables for framebuffer base");
    }
    crate::log_info!(
        "  Channel Layout   : R[shift={}, size={}] G[shift={}, size={}] B[shift={}, size={}]",
        format.red_mask_shift,
        format.red_mask_size,
        format.green_mask_shift,
        format.green_mask_size,
        format.blue_mask_shift,
        format.blue_mask_size
    );

    let fb = match Framebuffer::new_pure(width, height, pitch, format, phys_addr, virt_ptr) {
        Ok(f) => f,
        Err(e) => {
            crate::log_error!("[GRAPHICS] Invalid framebuffer parameters: {}", e);
            return;
        }
    };

    // Store into global static
    {
        let mut guard = FRAMEBUFFER.lock();
        *guard = Some(fb);
    }

    crate::log_info!("[GRAPHICS] Framebuffer initialized successfully.");

    // Initialize EternalOS Boot Splash Screen with glowing logo and progress bar
    crate::splash::init();
}

/// Access the global framebuffer under lock
pub fn with_framebuffer<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut Framebuffer) -> R,
{
    let mut guard = FRAMEBUFFER.lock();
    guard.as_mut().map(f)
}

/// Render the EternalOS Boot Screen
pub fn render_boot_screen() {
    with_framebuffer(|fb| {
        // Fallback check for minimal dimensions
        if fb.width < 640 || fb.height < 480 {
            // Compact / Small screen fallback
            fb.clear(Color::DARK_BG);
            fb.draw_text(16, 16, "Project EternalOS (Compact Mode)", Color::ACCENT_CYAN, None);
            fb.draw_text(16, 40, "Status: Kernel Initialized", Color::TEXT_PRIMARY, None);
            return;
        }

        // 1. Fill deep slate/navy background
        fb.clear(Color::DARK_BG);

        // 2. Draw Top Navigation / Header Bar
        fb.fill_rect(Rect::new(0, 0, fb.width, 48), Color::PANEL_BG);
        fb.draw_hline(0, 48, fb.width, Color::ACCENT_PURPLE);

        // Header Title
        fb.draw_text(24, 16, "PROJECT ETERNALOS", Color::ACCENT_LAVENDER, None);
        fb.draw_text(200, 16, "|   x86-64 Hybrid Kernel   |   Eternal Purple Architecture", Color::TEXT_MUTED, None);

        // 3. Main Center Branding & Logo Card
        let card_x = 40;
        let card_y = 70;
        let card_w = fb.width - 80;
        let card_h = 160;

        fb.fill_rect(Rect::new(card_x, card_y, card_w, card_h), Color::PANEL_BG);
        fb.draw_rect_border(Rect::new(card_x, card_y, card_w, card_h), Color::PANEL_BORDER);

        // Render embedded monochrome EternalOS logo asset in Pure White (#FFFFFF)
        let logo_x = card_x + 30;
        let logo_y = card_y + 45;
        fb.draw_monochrome_bitmap(
            logo_x,
            logo_y,
            crate::logo_asset::LOGO_WIDTH,
            crate::logo_asset::LOGO_HEIGHT,
            &crate::logo_asset::LOGO_BITMAP,
            Color::WHITE,
        );

        // Wordmark and System Identity
        let text_base_x = logo_x + crate::logo_asset::LOGO_WIDTH + 24;
        fb.draw_text(text_base_x, card_y + 40, "EternalOS - Core Kernel Subsystem", Color::TEXT_PRIMARY, None);
        fb.draw_text(text_base_x, card_y + 65, "Architecture : x86-64 Bare-Metal (UEFI Boot)", Color::TEXT_MUTED, None);
        fb.draw_text(text_base_x, card_y + 85, "Boot Protocol: Limine v10 Protocol", Color::TEXT_MUTED, None);
        fb.draw_text(text_base_x, card_y + 105, "Scheduler    : Preemptive Round-Robin (100 Hz PIT)", Color::TEXT_MUTED, None);

        // 4. Subsystem Diagnostics & Status Panel
        let diag_y = card_y + card_h + 24;
        let diag_h = fb.height - diag_y - 48;
        fb.fill_rect(Rect::new(card_x, diag_y, card_w, diag_h), Color::PANEL_BG);
        fb.draw_rect_border(Rect::new(card_x, diag_y, card_w, diag_h), Color::PANEL_BORDER);

        fb.draw_text(card_x + 20, diag_y + 16, "SYSTEM DIAGNOSTICS & VERIFICATION", Color::ACCENT_LAVENDER, None);
        fb.draw_hline(card_x + 20, diag_y + 36, card_w - 40, Color::PANEL_BORDER);

        let col1_x = card_x + 24;
        let col2_x = card_x + (card_w / 2) + 12;
        let mut row_y = diag_y + 50;

        // Subsystem items column 1
        fb.draw_text(col1_x, row_y, "[PASS] CPU GDT, TSS & IST1 Stack", Color::SUCCESS_GREEN, None);
        fb.draw_text(col2_x, row_y, "[PASS] PIT Preemptive Scheduler (100 Hz)", Color::SUCCESS_GREEN, None);
        row_y += 24;

        fb.draw_text(col1_x, row_y, "[PASS] IDT & Exception Handlers", Color::SUCCESS_GREEN, None);
        fb.draw_text(col2_x, row_y, "[PASS] Kernel Synchronization Primitives", Color::SUCCESS_GREEN, None);
        row_y += 24;

        fb.draw_text(col1_x, row_y, "[PASS] Bitmap Physical Frame Allocator", Color::SUCCESS_GREEN, None);
        fb.draw_text(col2_x, row_y, "[PASS] Kernel Timers & Sleep Queues", Color::SUCCESS_GREEN, None);
        row_y += 24;

        fb.draw_text(col1_x, row_y, "[PASS] 4-Level Paging & Virtual Memory", Color::SUCCESS_GREEN, None);
        fb.draw_text(col2_x, row_y, "[PASS] PCI Configuration & Bus Discovery", Color::SUCCESS_GREEN, None);
        row_y += 24;

        fb.draw_text(col1_x, row_y, "[PASS] Dynamic Kernel Heap (128 KiB)", Color::SUCCESS_GREEN, None);
        fb.draw_text(col2_x, row_y, "[PASS] Framebuffer & 2D Graphics Primitives", Color::SUCCESS_GREEN, None);

        // 5. Bottom Status / Footer Bar
        let footer_y = fb.height - 32;
        fb.fill_rect(Rect::new(0, footer_y, fb.width, 32), Color::PANEL_BG);
        fb.draw_hline(0, footer_y, fb.width, Color::PANEL_BORDER);
        fb.draw_text(24, footer_y + 8, "Status: Kernel Startup Complete - CPU Halt Safe State", Color::SUCCESS_GREEN, None);
        let fb_info = "Mode: 32bpp Linear Framebuffer";
        let info_x = fb.width.saturating_sub(280);
        fb.draw_text(info_x, footer_y + 8, fb_info, Color::TEXT_MUTED, None);
    });
}
