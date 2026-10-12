//! Centralized System Lifecycle Implementation
//!
//! Provides safe, predictable, and clean mechanisms for:
//! 1. `shutdown` — Flushes filesystems/caches, disables hardware, and powers down via ACPI / QEMU shutdown ports.
//! 2. `reboot`   — Flushes filesystems/caches, issues hardware reset via 8042 PS/2 controller, ACPI reset, or triple-fault fallback.
//! 3. `exit`     — Contextual shell exit (root shell notification vs child process termination).

use core::arch::asm;
use crate::console::console_write;

/// Lifecycle request action types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleAction {
    Shutdown,
    Reboot,
    Exit,
}

/// Status or result of a lifecycle request
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleResult {
    /// Action is proceeding to platform hardware
    Proceeding,
    /// Root shell notice presented (cannot exit primary shell)
    RootShellNotice,
    /// Child shell exited successfully
    ChildShellExited,
    /// Unsupported hardware or platform mechanism
    UnsupportedPlatform(&'static str),
}

/// Flush/sync filesystems and block devices before lifecycle transitions
pub fn sync_storage_subsystems() {
    console_write("[lifecycle] Synchronizing storage caches & pending writes...\r\n");
    // RamFs and read-only EternalFS do not hold uncommitted dirty writeback pages.
    // If future write-back caching is added to BlockDevice, it will be flushed here.
    core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
    console_write("[lifecycle] Storage caches synchronized cleanly.\r\n");
}

/// Safely quiesce peripheral hardware and disable CPU interrupts
pub fn quiesce_system() {
    console_write("[lifecycle] Quiescing device drivers and interrupt lines...\r\n");

    // Disable CPU interrupts (CLI) so no timer ticks, keyboard ISRs, or network interrupts fire
    unsafe {
        asm!("cli", options(nomem, nostack, preserves_flags));
    }
}

/// Execute a system shutdown request
pub fn execute_shutdown() -> ! {
    sync_storage_subsystems();
    quiesce_system();

    console_write("[lifecycle] Initiating platform power-off sequence...\r\n");

    // 1. QEMU ACPI / PIIX4 / Q35 Poweroff:
    // Port 0x604 with value 0x2000 (standard QEMU ACPI poweroff port enabled by default in QEMU)
    unsafe {
        asm!("out dx, ax", in("dx") 0x604u16, in("ax") 0x2000u16, options(nomem, nostack, preserves_flags));
    }

    // Small delay
    for _ in 0..10_000 {
        core::hint::spin_loop();
    }

    // 2. QEMU / Bochs older APM shutdown port (0xB004 with 0x2000)
    unsafe {
        asm!("out dx, ax", in("dx") 0xB004u16, in("ax") 0x2000u16, options(nomem, nostack, preserves_flags));
    }

    for _ in 0..10_000 {
        core::hint::spin_loop();
    }

    // 3. QEMU `isa-debug-exit` port (0x501)
    unsafe {
        asm!("out dx, al", in("dx") 0x501u16, in("al") 0x00u8, options(nomem, nostack, preserves_flags));
    }

    // If all platform hardware shutdown mechanisms are rejected by the hypervisor or bare-metal BIOS:
    console_write("\r\n[lifecycle] Notice: Platform power-off is unsupported or not acknowledged by hardware.\r\n");
    console_write("[lifecycle] System is safely halted. It is now safe to turn off your computer.\r\n");

    crate::hcf()
}

/// Execute a system reboot request
pub fn execute_reboot() -> ! {
    sync_storage_subsystems();
    quiesce_system();

    console_write("[lifecycle] Initiating platform restart sequence...\r\n");

    // 1. ACPI / PCI Reset Control Register (Port 0xCF9):
    // Bit 1 = System Reset (0x02), Bit 2 = Reset CPU (0x04) -> 0x06 (Cold reset)
    unsafe {
        // Step 1: Write 0x02 to prepare reset
        asm!("out dx, al", in("dx") 0xCF9u16, in("al") 0x02u8, options(nomem, nostack, preserves_flags));
        for _ in 0..1000 { core::hint::spin_loop(); }
        // Step 2: Write 0x06 to trigger reset
        asm!("out dx, al", in("dx") 0xCF9u16, in("al") 0x06u8, options(nomem, nostack, preserves_flags));
    }

    for _ in 0..10_000 {
        core::hint::spin_loop();
    }

    // 2. 8042 PS/2 Keyboard Controller Pulse Reset:
    // Writing 0xFE to port 0x64 pulses the CPU reset line
    unsafe {
        // Wait until 8042 input buffer is empty (bit 1 == 0)
        for _ in 0..1000 {
            let status: u8;
            asm!("in al, dx", in("dx") 0x64u16, out("al") status, options(nomem, nostack, preserves_flags));
            if (status & 0x02) == 0 {
                break;
            }
        }
        // Send Pulse Reset command
        asm!("out dx, al", in("dx") 0x64u16, in("al") 0xFEu8, options(nomem, nostack, preserves_flags));
    }

    for _ in 0..10_000 {
        core::hint::spin_loop();
    }

    // 3. Fallback: Triple-fault via empty Interrupt Descriptor Table (IDT)
    console_write("[lifecycle] Triggering CPU triple-fault reset fallback...\r\n");
    unsafe {
        let null_idt: [u16; 3] = [0, 0, 0]; // Limit 0, Base 0
        asm!("lidt [{}]", in(reg) null_idt.as_ptr(), options(nostack));
        asm!("int 3", options(noreturn));
    }
}

/// Handle `exit` command based on shell hierarchy context
pub fn handle_exit(is_root_shell: bool) -> LifecycleResult {
    if is_root_shell {
        console_write("exit: Cannot exit the root system shell.\r\n");
        console_write("Type 'shutdown' to power off the machine or 'reboot' to restart.\r\n");
        LifecycleResult::RootShellNotice
    } else {
        console_write("exit: Terminating child shell process.\r\n");
        LifecycleResult::ChildShellExited
    }
}
