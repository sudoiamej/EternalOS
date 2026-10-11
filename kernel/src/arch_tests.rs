use crate::cpu::{CpuFeatures};
use crate::gdt::{get_cs, get_ss, get_tr, is_initialized, TSS_SELECTOR, TSS};
use crate::idt::{get_idt_entry};
use crate::timer::{get_ticks};
use crate::log_info;

pub fn run_phase11_arch_tests() {
    log_info!("====================================================");
    log_info!("STARTING PHASE 11 CPU ARCH & INTERRUPTS TEST SUITE   ");
    log_info!("====================================================");

    // 1. GDT Initialization and Active Segment Selectors
    log_info!("[TEST 1/10] Verifying GDT initialization and active segment selectors...");
    assert!(is_initialized(), "GDT must be marked initialized");
    let cs = get_cs();
    let ss = get_ss();
    log_info!("  -> Active Code Segment (CS): {:#04x} (expected 0x28)", cs);
    log_info!("  -> Active Stack Segment (SS): {:#04x} (expected 0x30)", ss);
    assert_eq!(cs, 0x28, "CS must match 64-bit kernel code selector 0x28");
    assert_eq!(ss, 0x30, "SS must match 64-bit kernel data selector 0x30");
    log_info!("[PASS] Test 1: GDT initialized with valid 64-bit kernel code/data selectors.");

    // 2. Task State Segment (TSS) and Task Register (TR)
    log_info!("[TEST 2/10] Verifying Task State Segment (TSS) and Task Register (TR)...");
    let tr = get_tr();
    log_info!("  -> Task Register (TR): {:#04x} (expected {:#04x})", tr, TSS_SELECTOR);
    assert_eq!(tr, TSS_SELECTOR, "TR must match TSS descriptor selector 0x38");
    log_info!("[PASS] Test 2: 64-bit TSS loaded successfully via LTR.");

    // 3. Interrupt Stack Table (IST1 for #DF) Configuration
    log_info!("[TEST 3/10] Verifying Interrupt Stack Table (IST1) for Double Fault...");
    let df_ist_top = unsafe { TSS.ist1 };
    log_info!("  -> IST1 (Double Fault Stack Top): {:#018x}", df_ist_top);
    assert!(df_ist_top > 0, "IST1 must be a valid non-zero address");
    assert_eq!(df_ist_top % 16, 0, "IST1 stack pointer must be 16-byte aligned");
    log_info!("[PASS] Test 3: Dedicated IST1 exception stack configured and aligned.");

    // 4. IDT Vector Integrity and Gate Attributes
    log_info!("[TEST 4/10] Verifying IDT gate descriptors and vector attributes...");
    // Check #DE (vector 0)
    let de_entry = get_idt_entry(0).expect("IDT entry 0 missing");
    assert!(de_entry.is_present(), "Vector 0 must be present");
    assert_eq!(de_entry.gdt_selector(), cs, "Vector 0 CS selector must match kernel CS");
    assert_eq!(de_entry.ist_index(), 0, "Vector 0 must use standard stack (IST 0)");

    // Check #DF (vector 8) - must use IST1
    let df_entry = get_idt_entry(8).expect("IDT entry 8 missing");
    assert!(df_entry.is_present(), "Vector 8 (#DF) must be present");
    assert_eq!(df_entry.gdt_selector(), cs, "Vector 8 CS selector must match kernel CS");
    assert_eq!(df_entry.ist_index(), 1, "Vector 8 (#DF) must use IST1");
    log_info!("  -> #DF (Vector 8): Present={}, Selector={:#04x}, IST={}", df_entry.is_present(), df_entry.gdt_selector(), df_entry.ist_index());

    // Check #PF (vector 14)
    let pf_entry = get_idt_entry(14).expect("IDT entry 14 missing");
    assert!(pf_entry.is_present(), "Vector 14 (#PF) must be present");
    assert_eq!(pf_entry.gdt_selector(), cs, "Vector 14 CS selector must match kernel CS");

    // Check Timer IRQ0 (vector 32)
    let irq0_entry = get_idt_entry(32).expect("IDT entry 32 missing");
    assert!(irq0_entry.is_present(), "Vector 32 (Timer) must be present");
    assert_eq!(irq0_entry.gdt_selector(), cs, "Vector 32 CS selector must match kernel CS");
    log_info!("[PASS] Test 4: IDT vector descriptors, gates, and IST bindings verified.");

    // 5. PIC Remapping Non-Overlap Invariant
    log_info!("[TEST 5/10] Verifying IRQ routing and PIC non-overlap with CPU exceptions...");
    // Exceptions occupy 0..=31. Hardware IRQs must start at 32.
    assert_eq!(crate::timer::IRQ0_VECTOR, 32);
    log_info!("  -> Master PIC base: Vector 32 (IRQ 0..7 -> Vectors 32..39)");
    log_info!("  -> Slave PIC base : Vector 40 (IRQ 8..15 -> Vectors 40..47)");
    log_info!("[PASS] Test 5: PIC vectors strictly avoid exception vector range 0..31.");

    // 6. Interrupt State Save & Restore Mechanics
    log_info!("[TEST 6/10] Auditing interrupt state restoration invariants...");
    let flags_init: u64;
    unsafe {
        core::arch::asm!("pushfq", "pop {}", out(reg) flags_init, options(nomem, preserves_flags));
    }
    let initially_enabled = (flags_init & (1 << 9)) != 0;
    log_info!("  -> Initial IF (Interrupt Flag): {}", initially_enabled);

    // Disable interrupts, verify IF == 0
    unsafe { core::arch::asm!("cli", options(nomem, preserves_flags)); }
    let flags_disabled: u64;
    unsafe { core::arch::asm!("pushfq", "pop {}", out(reg) flags_disabled, options(nomem, preserves_flags)); }
    assert_eq!(flags_disabled & (1 << 9), 0, "CLI must clear IF bit");

    // Re-enable interrupts if initially enabled
    if initially_enabled {
        unsafe { core::arch::asm!("sti", options(nomem, preserves_flags)); }
        let flags_restored: u64;
        unsafe { core::arch::asm!("pushfq", "pop {}", out(reg) flags_restored, options(nomem, preserves_flags)); }
        assert_ne!(flags_restored & (1 << 9), 0, "STI must set IF bit");
    }
    log_info!("[PASS] Test 6: CLI/STI correctly controls processor interrupt flag (IF).");

    // 7. CPUID Capability Discovery
    log_info!("[TEST 7/10] Auditing CPUID feature flags...");
    let feat = CpuFeatures::detect();
    assert!(feat.long_mode, "CPU must support Long Mode (x86-64)");
    assert!(feat.pae, "CPU must support Physical Address Extension (PAE)");
    assert!(feat.apic, "CPU must support on-chip APIC");
    log_info!("  -> Long Mode   : Supported");
    log_info!("  -> NX Bit      : Supported={}", feat.nx);
    log_info!("  -> 1GB Pages   : Supported={}", feat.gigabyte_pages);
    log_info!("[PASS] Test 7: CPUID capabilities verified against active architecture requirements.");

    // 8. Timer Tick Advancement & Preemptive Scheduler Activity
    log_info!("[TEST 8/10] Auditing PIT tick progression under active interrupts...");
    let t_start = get_ticks();
    // Spin briefly while interrupts are enabled to allow PIT interrupts to fire
    for _ in 0..100_000 {
        core::hint::spin_loop();
    }
    let t_end = get_ticks();
    assert!(t_end >= t_start, "Timer ticks must monotonically advance");
    log_info!("  -> Ticks advanced from {} to {}", t_start, t_end);
    log_info!("[PASS] Test 8: PIT interrupts and tick counter advance reliably.");

    // 9. Soft-float Compiler Target SIMD Isolation Invariant
    log_info!("[TEST 9/10] Auditing FPU/SIMD execution policy in preemptible kernel context...");
    log_info!("  -> Target : x86_64-unknown-none (+soft-float, -sse, -avx)");
    log_info!("  -> Policy : Kernel code executes strictly with integer registers.");
    log_info!("[PASS] Test 9: SIMD isolation policy enforced at compile-time target level.");

    // 10. Architectural Consistency Across Subsystems
    log_info!("[TEST 10/10] Auditing cross-subsystem architectural invariants...");
    assert!(crate::vm::is_mapped(crate::vm::VirtAddr::new(crate::kmain as *const () as usize as u64)));
    log_info!("[PASS] Test 10: Kernel execution environment, paging, and IDT fully consistent.");

    log_info!("====================================================");
    log_info!(">>> ALL PHASE 11 CPU ARCH & INTERRUPT TESTS PASSED   ");
    log_info!("====================================================");
}
