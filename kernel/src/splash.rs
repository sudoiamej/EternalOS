//! EternalOS Graphical Boot Splash Subsystem
//!
//! Provides a boot splash screen featuring:
//! - Deep slate/purple backdrop (#111019)
//! - Centered white logo (#FFFFFF) with a soft radial purple glow (#7C3AED / #6D28D9)
//! - System title and version wordmark
//! - Animated/stepwise progress bar with smooth lavender/purple gradient
//! - Truthful subsystem initialization stage and status message
//! - Clean transition into the interactive terminal panel

use crate::framebuffer::{with_framebuffer, Color, Rect};
use crate::logo_asset::{LOGO_BITMAP, LOGO_HEIGHT, LOGO_WIDTH};

/// Subsystem initialization milestones during kernel boot
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootStage {
    CoreMemory,      // GDT, IDT, PMM, Heap, Paging (15%)
    TaskingAndPci,   // Tasking, Scheduler, PCI Bus Enumeration (35%)
    StorageAndVfs,   // VirtIO Block Storage, VFS, RamFs, EternalFS (55%)
    TimerAndInput,   // PIT Timer, PS/2 Keyboard & Mouse, Interrupts (75%)
    NetworkAndUsb,   // E1000 Gigabit NIC, UHCI USB Host Controller (90%)
    Ready,           // System Ready, Starting Interactive Shell (100%)
}

impl BootStage {
    pub fn percentage(&self) -> usize {
        match self {
            BootStage::CoreMemory => 15,
            BootStage::TaskingAndPci => 35,
            BootStage::StorageAndVfs => 55,
            BootStage::TimerAndInput => 75,
            BootStage::NetworkAndUsb => 90,
            BootStage::Ready => 100,
        }
    }

    pub fn default_label(&self) -> &'static str {
        match self {
            BootStage::CoreMemory => "Initializing CPU, Memory & Framebuffer...",
            BootStage::TaskingAndPci => "Enumerating PCI Bus & Tasking Subsystem...",
            BootStage::StorageAndVfs => "Mounting Virtual Filesystem & Storage...",
            BootStage::TimerAndInput => "Configuring Hardware Timers & Input...",
            BootStage::NetworkAndUsb => "Initializing Network & USB Controllers...",
            BootStage::Ready => "Kernel Startup Complete. Launching Shell...",
        }
    }
}

/// Initialize the boot splash screen
pub fn init() {
    with_framebuffer(|fb| {
        if fb.width < 640 || fb.height < 480 {
            // Small screen fallback
            fb.clear(Color::DARK_BG);
            fb.draw_text(16, 16, "Project EternalOS", Color::ACCENT_LAVENDER, None);
            fb.draw_text(16, 40, "Booting...", Color::TEXT_PRIMARY, None);
            return;
        }

        // Fill background
        fb.clear(Color::DARK_BG);

        // Center coordinates
        let center_x = fb.width / 2;
        let logo_y = fb.height / 2 - 120;
        let logo_x = center_x - (LOGO_WIDTH / 2);

        // 1. Draw glowing purple halo layers behind the logo
        draw_logo_glow(fb, center_x, logo_y + (LOGO_HEIGHT / 2));

        // 2. Draw centered pure white logo bitmap
        fb.draw_monochrome_bitmap(logo_x, logo_y, LOGO_WIDTH, LOGO_HEIGHT, &LOGO_BITMAP, Color::WHITE);

        // 3. Draw Wordmark and subtitle
        let title = "ETERNALOS";
        let title_w = title.len() * 8;
        let title_x = center_x - (title_w / 2);
        let title_y = logo_y + LOGO_HEIGHT + 18;
        fb.draw_text(title_x, title_y, title, Color::ACCENT_LAVENDER, None);

        let subtitle = "x86-64 Bare-Metal Operating System";
        let sub_w = subtitle.len() * 8;
        let sub_x = center_x - (sub_w / 2);
        let sub_y = title_y + 20;
        fb.draw_text(sub_x, sub_y, subtitle, Color::TEXT_MUTED, None);

        // 4. Initial progress bar frame
        draw_progress(fb, 5, "Initializing Kernel Environment...");
    });

    pause_stage(250);
}

#[cfg(any(
    feature = "test-ecs",
    feature = "test-net",
    feature = "test-usb",
    feature = "test-shell",
    feature = "test-userspace",
    feature = "test-eternalfs",
    feature = "test-vfs",
    feature = "test-partitions",
    feature = "test-storage",
    feature = "test-graphics",
    feature = "test-pci"
))]
pub fn pause_stage(_ms: u64) {
    // Zero delay during automated test suites
}

#[cfg(not(any(
    feature = "test-ecs",
    feature = "test-net",
    feature = "test-usb",
    feature = "test-shell",
    feature = "test-userspace",
    feature = "test-eternalfs",
    feature = "test-vfs",
    feature = "test-partitions",
    feature = "test-storage",
    feature = "test-graphics",
    feature = "test-pci"
)))]
pub fn pause_stage(ms: u64) {
    let start_tick = crate::timer::get_ticks();
    if start_tick > 0 {
        let needed_ticks = (ms / 10).max(1);
        while crate::timer::get_ticks().saturating_sub(start_tick) < needed_ticks {
            core::hint::spin_loop();
        }
    } else {
        // Fallback before timer IRQ is enabled: calibrated I/O port cycle delay (~1us per io_wait)
        let loops = ms * 1200;
        for _ in 0..loops {
            unsafe {
                core::arch::asm!("out dx, al", in("dx") 0x80u16, in("al") 0u8, options(nomem, nostack, preserves_flags));
            }
        }
    }
}

/// Update boot splash status and progress bar
pub fn update_stage(stage: BootStage, detail: Option<&str>) {
    let percent = stage.percentage();
    let message = detail.unwrap_or_else(|| stage.default_label());

    with_framebuffer(|fb| {
        if fb.width < 640 || fb.height < 480 {
            fb.draw_text(16, 40, message, Color::TEXT_PRIMARY, Some(Color::DARK_BG));
            return;
        }

        draw_progress(fb, percent, message);
    });

    pause_stage(220);
}

/// Finish splash display and transition into terminal console
pub fn finish() {
    update_stage(BootStage::Ready, Some("Kernel Startup Complete. Starting Shell..."));
    pause_stage(350);

    // Switch cleanly to interactive terminal panel layout
    crate::console::init_terminal_viewport();
}

/// Helper to render the soft purple glow behind the logo
fn draw_logo_glow(fb: &crate::framebuffer::Framebuffer, cx: usize, cy: usize) {
    // Outer soft glow (Color: #362D4F / DARK SLATE PURPLE)
    let r_outer_x = 58usize;
    let r_outer_y = 48usize;
    let glow_outer = Color { r: 0x2A, g: 0x22, b: 0x40 };
    draw_ellipse_filled(fb, cx, cy, r_outer_x, r_outer_y, glow_outer);

    // Mid glow (Color: #6D28D9 / ACCENT_SECONDARY)
    let r_mid_x = 44usize;
    let r_mid_y = 36usize;
    let glow_mid = Color { r: 0x48, g: 0x24, b: 0x7A };
    draw_ellipse_filled(fb, cx, cy, r_mid_x, r_mid_y, glow_mid);

    // Inner bright glow (Color: #7C3AED / ACCENT_PURPLE)
    let r_in_x = 32usize;
    let r_in_y = 26usize;
    let glow_in = Color { r: 0x60, g: 0x2C, b: 0xA6 };
    draw_ellipse_filled(fb, cx, cy, r_in_x, r_in_y, glow_in);
}

/// Approximate filled ellipse for glowing effect
fn draw_ellipse_filled(fb: &crate::framebuffer::Framebuffer, cx: usize, cy: usize, rx: usize, ry: usize, color: Color) {
    let ry_i = ry as isize;
    let rx_i = rx as isize;
    let rx_sq = rx_i * rx_i;
    let ry_sq = ry_i * ry_i;

    for dy in -ry_i..=ry_i {
        let py = (cy as isize + dy) as usize;
        let dy_sq = dy * dy;
        let rem = rx_sq - (dy_sq * rx_sq) / ry_sq;
        if rem >= 0 {
            let dx = int_sqrt(rem as usize) as usize;
            let x_start = cx.saturating_sub(dx);
            let x_end = (cx + dx).min(fb.width);
            if x_start < x_end {
                fb.fill_rect(Rect::new(x_start, py, x_end - x_start, 1), color);
            }
        }
    }
}

fn int_sqrt(n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

/// Render the progress bar and status text
fn draw_progress(fb: &crate::framebuffer::Framebuffer, percent: usize, status_text: &str) {
    let center_x = fb.width / 2;
    let bar_width = 380usize;
    let bar_height = 8usize;
    let bar_x = center_x - (bar_width / 2);
    let bar_y = fb.height / 2 + 30;

    // Outer track background & border
    let track_rect = Rect::new(bar_x, bar_y, bar_width, bar_height);
    fb.fill_rect(track_rect, Color::PANEL_BG);
    fb.draw_rect_border(
        Rect::new(bar_x.saturating_sub(2), bar_y.saturating_sub(2), bar_width + 4, bar_height + 4),
        Color::PANEL_BORDER,
    );

    // Active fill width
    let clamped_percent = percent.min(100);
    let fill_w = (bar_width * clamped_percent) / 100;
    if fill_w > 0 {
        // Gradient fill: left half purple, right half soft lavender
        let half = fill_w / 2;
        if half > 0 {
            fb.fill_rect(Rect::new(bar_x, bar_y, half, bar_height), Color::ACCENT_SECONDARY);
            fb.fill_rect(Rect::new(bar_x + half, bar_y, fill_w - half, bar_height), Color::ACCENT_LAVENDER);
        } else {
            fb.fill_rect(Rect::new(bar_x, bar_y, fill_w, bar_height), Color::ACCENT_PURPLE);
        }
    }

    // Status message line (clear background rect first to avoid text overlap)
    let text_y = bar_y + bar_height + 16;
    let text_rect = Rect::new(center_x.saturating_sub(250), text_y, 500, 18);
    fb.fill_rect(text_rect, Color::DARK_BG);

    let text_w = status_text.len() * 8;
    let text_x = center_x.saturating_sub(text_w / 2);
    fb.draw_text(text_x, text_y, status_text, Color::TEXT_MUTED, Some(Color::DARK_BG));
}
