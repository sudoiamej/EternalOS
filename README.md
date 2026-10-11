# Project EternalOS

EternalOS is an x86-64 operating system built from scratch with a Rust-first hybrid kernel architecture, booting via the modern Limine boot protocol.

---

## 🎯 Architecture & Milestone Goals

- **Architecture:** Hybrid Kernel (x86-64 bare-metal)
- **Boot Protocol:** Limine Boot Protocol v10 (UEFI & BIOS compatible)
- **Toolchain:** Rust Nightly (`x86_64-unknown-none`) + LLD Linker
- **Primary Host Environment:** Native Windows 11
- **Emulation & Verification:** QEMU Virtual Machine (`q35` with OVMF UEFI)

---

## 📂 Repository Structure

```text
EternalOS/
├── .gitignore
├── README.md
├── run.ps1                 # Native 1-click build & boot runner for Windows
├── boot/
│   ├── BOOTX64.EFI         # Limine x86-64 UEFI bootloader binary
│   ├── limine-bios-cd.bin  # Limine BIOS CD stage
│   ├── limine-uefi-cd.bin  # Limine UEFI CD stage
│   └── limine.conf         # Limine boot menu configuration
├── kernel/
│   ├── Cargo.toml          # Kernel package configuration & test features
│   ├── Cargo.lock          # Locked crate versions (limine v0.5.0)
│   ├── rust-toolchain.toml # Locks to nightly & bare-metal target
│   ├── build.rs            # Passes linker script directly to lld
│   ├── linker-x86_64.ld    # High-half x86-64 ELF linker script (0xffffffff80000000)
│   └── src/
│       ├── main.rs         # Kernel entry, subsystem orchestration, test triggers
│       ├── serial.rs       # 16550 UART serial driver, structured log macros (DEBUG, INFO, WARN, ERROR)
│       ├── idt.rs          # Interrupt Descriptor Table & CPU exception handlers + Timer IRQ0 handler
│       ├── timer.rs        # 8259 PIC remapping, 8254 PIT rate generator (100 Hz), tick counter
│       ├── mm.rs           # Limine physical memory map parser & Bitmap Physical Frame Allocator
│       ├── mm_tests.rs     # Phase 3 physical memory management test suite
│       ├── vm.rs           # Virtual memory foundations, 4-level page table walking, mapping & TLB invalidation
│       ├── heap.rs         # Kernel heap allocator (`GlobalAlloc`), free-list management, and page table backing
│       ├── heap_tests.rs   # Phase 4 dynamic allocation test suite
│       ├── task.rs         # Kernel task abstraction, round-robin scheduler, context switch & preemption support
│       ├── task_tests.rs   # Phase 5 cooperative multitasking test suite
│       └── preempt_tests.rs# Phase 6 hardware timer preemption test suite
└── tools/
    └── limine.exe          # Limine bootloader host deployment tool
```

---

## ⏱️ Hardware Timer & Preemptive Scheduling (Phase 6)

### 1. Hardware Timer Subsystem
- **Timer Source:** Intel 8254 Programmable Interval Timer (PIT) configured in Mode 2 (Rate Generator) at 100 Hz (10 ms time-slice quantum).
- **Interrupt Routing:** Dual 8259 PIC remapped to vectors 32..47 so IRQ0 maps directly to IDT Vector 32 without colliding with CPU exceptions 0..31.
- **Tick Tracking:** Tracked using an interrupt-safe `AtomicU64` (`TIMER_TICKS`).
- **End-Of-Interrupt (EOI):** Sent directly to the PIC (`0x20`) on each timer interrupt tick.

### 2. Preemptive Scheduling Mechanism
- **Interrupt Entry:** Timer IRQ0 enters `timer_interrupt_handler` via vector 32.
- **Preemption Safety & Handoff:**
  - Timer handler attempts a non-blocking `compare_exchange` on `SCHEDULER_LOCK`. If the scheduler is not locked, it increments `PREEMPTION_COUNT` and invokes `SCHEDULER.schedule_next()`.
  - Stacks and callee-saved registers (`r15`, `r14`, `r13`, `r12`, `rbx`, `rbp`, `rsp`, `rip`) are switched, preempting the running task.
  - Newly spawned tasks bootstrap via `task_entry_trampoline`, which issues `sti` to guarantee interrupts remain enabled during task execution.
- **Cooperative Compatibility:** `yield_now()` remains supported and cleanly coordinates with timer preemption using `cli`/`sti` boundaries during yield transitions.

---

## 🛠️ Build & Run Instructions (Native Windows)

### 1. Normal Kernel Boot
```powershell
.\run.ps1 -Headless
```

### 2. Run Phase 6 Hardware Timer Preemption Tests
```powershell
.\run.ps1 -Headless -Feature test-preempt
```

### 3. Run Prior Subsystem Test Suites (Regression Verification)
```powershell
.\run.ps1 -Headless -Feature test-task
.\run.ps1 -Headless -Feature test-heap
.\run.ps1 -Headless -Feature test-memory
```
