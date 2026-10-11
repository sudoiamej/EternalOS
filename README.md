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
│       ├── idt.rs          # Interrupt Descriptor Table & CPU exception handlers (#DE, #UD, #DF, #GP)
│       ├── mm.rs           # Limine physical memory map parser & Bitmap Physical Frame Allocator
│       ├── mm_tests.rs     # Phase 3 physical memory management test suite
│       ├── vm.rs           # Virtual memory foundations, 4-level page table walking, mapping & TLB invalidation
│       ├── heap.rs         # Kernel heap allocator (`GlobalAlloc`), free-list management, and page table backing
│       ├── heap_tests.rs   # Phase 4 dynamic allocation test suite
│       ├── task.rs         # Kernel task abstraction, cooperative round-robin scheduler, and x86-64 context switch
│       └── task_tests.rs   # Phase 5 cooperative multitasking test suite
└── tools/
    └── limine.exe          # Limine bootloader host deployment tool
```

---

## ⚙️ Kernel Tasking & Cooperative Scheduler (Phase 5)

### 1. Task Abstraction & Stack Management
- **Task Structure (`Task`):** Holds unique `id`, human-readable `name`, `TaskState` (`Ready`, `Running`, `Terminated`), saved `Context`, and bounds `stack_bottom` / `stack_top`.
- **Dedicated Stacks:** Each task is allocated 16 KiB (4 aligned 4 KiB pages) backed by physical frames from the PMM and mapped with `WRITABLE | NO_EXECUTE` permissions in dedicated virtual memory space (`0xffff_a000_0000_0000`). Each stack is separated by unmapped guard pages.
- **Initial Alignment:** Enforces standard x86-64 System V 16-byte stack alignment.

### 2. Context Switching
- **ABI Boundary:** Preserves callee-saved registers (`r15`, `r14`, `r13`, `r12`, `rbx`, `rbp`, `rsp`) and instruction pointer (`rip`).
- **Naked Assembly:** Implemented via `#[unsafe(naked)]` function `x86_64_switch_context(prev, next)` without compiler prologue/epilogue interference.
- **Bootstrap & Trampoline:** Tasks bootstrap through `task_entry_trampoline`. When a task function returns, the trampoline cleanly invokes `terminate_task()`.
- **FPU/SIMD State Policy:** The current execution model does not save/restore SSE/AVX state across cooperative yields; kernel tasks must not execute floating-point/vector instructions across yield points.

### 3. Cooperative Scheduler Policy
- **Algorithm:** Deterministic Round-Robin over an internal task table (`MAX_TASKS = 16`).
- **State Flow:** `Ready -> Running -> (yield_now) -> Ready -> (return) -> Terminated`. Terminated tasks are marked and skipped in future scheduling rounds.
- **Synchronization:** Mutex protection with spinlock handoff (`SCHEDULER_LOCK` released prior to switching execution and re-acquired upon resumption).

---

## 🛠️ Build & Run Instructions (Native Windows)

### 1. Normal Kernel Boot
```powershell
.\run.ps1 -Headless
```

### 2. Run Phase 5 Cooperative Scheduler Tests
```powershell
.\run.ps1 -Headless -Feature test-task
```

### 3. Run Prior Phase Tests (Regression Verification)
```powershell
.\run.ps1 -Headless -Feature test-heap
.\run.ps1 -Headless -Feature test-memory
```
