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
│       ├── task.rs         # Kernel task abstraction, scheduler, context switch, blocking/wakeup
│       ├── task_tests.rs   # Phase 5 cooperative multitasking test suite
│       ├── preempt_tests.rs# Phase 6 hardware timer preemption test suite
│       ├── sync.rs         # Phase 7 synchronization primitives: Mutex, Semaphore, WaitQueue, SpinLock
│       ├── sync_tests.rs   # Phase 7 synchronization verification suite
│       ├── sleep_tests.rs  # Phase 8 kernel timers, sleep queues, and lifecycle test suite
│       └── reliability_tests.rs # Phase 9 reliability, diagnostics, and invariant verification suite
└── tools/
    └── limine.exe          # Limine bootloader host deployment tool
```

---

## 🛡️ Kernel Reliability, Diagnostics & Invariant Verification (Phase 9)

### 1. Diagnostics & Failure Handling Architecture
- **Structured Logging:** Severity levels (`TRACE`, `DEBUG`, `INFO`, `WARN`, `ERROR`, `FATAL`) with single-core spinlock re-entrancy and bounded deadlock protection (lock breaking after 1,000,000 spin attempts).
- **Hardened Panic Path:** `rust_panic` freezes context via immediate `cli`, features atomic latching to detect double-panics, and provides an unformatted emergency output path (`emergency_write_str`) without heap allocations or locks.
- **CPU Exception Handlers:**
  - Expanded IDT exception handlers for `#DE` (0), `#UD` (6), `#DF` (8), `#TS` (10), `#SS` (12), `#GP` (13), and `#PF` (14).
  - Page faults read CR2 and decode hardware error bits (`Present`, `Write`, `User`, `ReservedBit`, `InstructionFetch`).
  - GPF reports hardware selector error codes and stack frame details (`RIP`, `CS`, `RFLAGS`, `RSP`, `SS`).
- **SIMD/FPU Safety Policy:** Target configuration `-mmx,-sse,-sse2,-sse3,-ssse3,-sse4.1,-sse4.2,-avx,-avx2,+soft-float` strictly restricts compiler-generated SIMD/floating-point instructions. Preemptible kernel tasks run with pure integer soft-float semantics, avoiding register clobber without requiring XSAVE/FXSAVE overhead on the hybrid kernel baseline.
- **Invariant Verifiers:** Audited invariant validators in `Scheduler::verify_invariants`, `FrameAllocator::verify_invariants`, `FreeListAllocator::verify_invariants`, and `WaitQueue::verify_invariants`.

---

## ⏱️ Kernel Timers, Sleep Queues & Task Lifecycle (Phase 8)

### 1. Features & Invariants
- **Kernel-Managed Sleep:** `sleep_ticks(ticks)` and `sleep_ms(ms)` put the running task into `TaskState::Sleeping(deadline)` without busy-waiting.
- **PIT Integration:** Timer interrupts at 100 Hz advance ticks and atomically inspect sleeping tasks in `Scheduler::check_sleeping_tasks(now)`, transitioning expired tasks back to `TaskState::Ready`.
- **Ceiling Conversion:** `ms_to_ticks(ms)` applies ceiling division (`(ms + MS_PER_TICK - 1) / MS_PER_TICK`) to guarantee tasks sleep for at least the requested duration.
- **Resource Reclamation:** Terminated tasks transition from `Terminated` to `Dead` upon `reclaim_tasks()` / `reclaim_terminated_tasks()`, returning their 4-page allocated stack frames back to the Physical Frame Allocator via `unmap_page()` and `deallocate_frame()`.
- **Slot Reuse & Monotonic IDs:** Completed/reclaimed tasks yield their task slots for new task allocation while maintaining strictly increasing task IDs (`next_task_id`), preventing ABA issues.

---

## 🔒 Task Synchronization Primitives (Phase 7)

### 1. Primitives & Blocking Mechanics
- **WaitQueue:** Fixed-capacity circular FIFO queue (`WaitQueue`) managing blocked task IDs.
- **SpinLock:** Non-blocking raw spinlock (`SpinLock<T>`) used strictly to protect synchronization metadata under interrupt-disabled (`cli`/`sti`) execution.
- **Mutex:** Blocking kernel mutex (`Mutex<T>`) with strict task ownership semantics (`owner: AtomicU64`), RAII guard (`MutexGuard`), and sleep/wakeup integration with `SCHEDULER`. Contending tasks are placed on the `WaitQueue` and transitioned to `TaskState::Blocked`.
- **Counting Semaphore:** Permit-tracking semaphore (`Semaphore`) with atomic permit accounting and scheduler-integrated sleeping when permits reach 0.
- **Scheduler Integration:**
  - Introduced `TaskState::Blocked` distinct from `Ready`, `Running`, and `Terminated`.
  - Added `block_current_task()` and `wake_task(id)` ensuring atomic state transitions with respect to timer interrupts and scheduler passes.

---

## 🛠️ Build & Run Instructions (Native Windows)

### 1. Normal Kernel Boot
```powershell
.\run.ps1 -Headless
```

### 2. Run Phase 9 Kernel Reliability & Invariant Tests
```powershell
.\run.ps1 -Headless -Feature test-reliability
```

### 3. Run Controlled Exception & Panic Fault Tests
```powershell
.\run.ps1 -Headless -Feature test-pagefault
.\run.ps1 -Headless -Feature test-gpf
.\run.ps1 -Headless -Feature test-invalid-opcode
.\run.ps1 -Headless -Feature test-div-zero
.\run.ps1 -Headless -Feature test-panic
```

### 4. Run Prior Subsystem Test Suites (Regression Verification)
```powershell
.\run.ps1 -Headless -Feature test-sleep
.\run.ps1 -Headless -Feature test-sync
.\run.ps1 -Headless -Feature test-preempt
.\run.ps1 -Headless -Feature test-task
.\run.ps1 -Headless -Feature test-heap
.\run.ps1 -Headless -Feature test-memory
```
