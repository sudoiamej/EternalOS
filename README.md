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
│       └── heap_tests.rs   # Phase 4 dynamic allocation test suite
└── tools/
    └── limine.exe          # Limine bootloader host deployment tool
```

---

## 🧠 Memory Management Architecture

### 1. Physical Page-Frame Allocator (PMM)
- **Design:** Statically allocated Bitmap Frame Allocator tracking up to 4 GiB of physical memory (`MAX_PHYSICAL_MEM`).
- **Memory Overhead:** 131,072 bytes (128 KiB) of bitmap overhead for 1,048,576 4 KiB frames (~0.003% total overhead).
- **Protected Regions:** Low 1 MiB (`0x0`–`0x100000`) hardware region and non-usable Limine regions (`RESERVED`, `ACPI_*`, `BOOTLOADER_RECLAIMABLE`, `FRAMEBUFFER`).
- **Safety Invariants:** Checked alignment, double-free detection, and checked arithmetic.

### 2. Virtual Memory Subsystem (VM)
- **Paging Mode:** 4-Level x86-64 Paging active (`PML4 -> PDPT -> PD -> PT`).
- **Safe Mapping:** `map_page(vaddr, pframe, flags)` walks the active PML4 (retrieved from `CR3` and translated via Limine HHDM), allocates intermediate page tables dynamically from the PMM, sets entry flags (`PRESENT | WRITABLE | NO_EXECUTE`), and executes `invlpg`.

### 3. Dynamic Kernel Heap (Phase 4)
- **Virtual Base:** Dedicated kernel heap space at `0xffff_9000_0000_0000`.
- **Initial Size:** 128 KiB (32 4 KiB pages), scalable up to 16 MiB.
- **Backing Mechanism:** Every virtual heap page is backed by physical frames allocated from the PMM and mapped with `WRITABLE | NO_EXECUTE` permissions before heap initialization.
- **Allocator Design:** Free-list allocator implementing `core::alloc::GlobalAlloc`.
- **Alignment & Splitting:**
  - Guarantees 8-byte minimum allocation and header alignment.
  - Supports custom alignments (16, 64, 128, etc.).
  - Free block headers are maintained in-band with boundary preservation.
- **Safety & Error Handling:**
  - OOB deallocation rejection.
  - Heap exhaustion deterministic failure (`Err("Kernel heap exhausted")` or returning null pointer).
  - Protected via re-entrancy safe `LockedHeap` spinlock.

---

## 🛠️ Build & Run Instructions (Native Windows)

### 1. Normal Kernel Boot
```powershell
.\run.ps1 -Headless
```

### 2. Run Phase 4 Kernel Heap Tests
```powershell
.\run.ps1 -Headless -Feature test-heap
```

### 3. Run Phase 3 Physical Memory Tests
```powershell
.\run.ps1 -Headless -Feature test-memory
```
