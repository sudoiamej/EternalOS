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
│       ├── mm_tests.rs     # Phase 3 memory management test suite
│       └── vm.rs           # Virtual memory foundations, canonical addresses, CR3/PML4 inspection
└── tools/
    └── limine.exe          # Limine bootloader host deployment tool
```

---

## 🧠 Memory Management Architecture (Phase 3)

### 1. Physical Page-Frame Allocator (PMM)
- **Design:** Statically allocated Bitmap Frame Allocator tracking up to 4 GiB of physical memory (`MAX_PHYSICAL_MEM`).
- **Memory Overhead:** 131,072 bytes (128 KiB) of bitmap overhead to manage up to 1,048,576 4 KiB frames (~0.003% overhead).
- **Protection & Exclusions:**
  - Excludes low 1 MiB (`0x0000000000000000`–`0x0000000000100000`) protecting legacy BIOS, IVT, BDA, and VGA hardware mappings.
  - Automatically reserves non-usable ranges: `RESERVED`, `ACPI_NVS`, `ACPI_RECLAIMABLE`, `BOOTLOADER_RECLAIMABLE`, `EXECUTABLE_AND_MODULES`, and `FRAMEBUFFER`.
  - Only memory explicitly tagged as `EntryType::USABLE` is made available to the allocator.
- **Safety & Validation:**
  - Checked 4 KiB alignment on allocation and deallocation.
  - Checked arithmetic against arithmetic overflow on address boundary computations.
  - Strict double-free and out-of-bounds deallocation rejection.
- **Synchronization Assumptions:**
  - Protected via `Mutex<FrameAllocator>` utilizing an atomic compare-exchange spinlock (`AtomicBool`). Single-core BSP execution is currently assumed; SMP cores are parked.

### 2. Virtual Memory Foundations (VM)
- **Paging Mode:** Standard 4-Level x86-64 Paging active (PML4 -> PDPT -> PD -> PT).
- **Active Address Space:** Inspected directly from `CR3` register (`pml4_phys`).
- **Address Validation:** Canonical virtual address checking (bits 47..63 sign-extension validation), 4 KiB alignment checks, and page table index extraction (`p4_index`, `p3_index`, `p2_index`, `p1_index`).
- **Higher-Half Mapping:** Preserves Limine's active higher-half direct map (HHDM) and higher-half kernel mapping (`0xffffffff80000000`). Page table modification is deferred until Phase 4 to guarantee zero unmapping of active bootloader memory.

---

## 🛠️ Build & Run Instructions (Native Windows)

### 1. Normal Kernel Boot
To build and launch the kernel normally in QEMU:
```powershell
.\run.ps1
```
Or run headless in the terminal to inspect serial output directly:
```powershell
.\run.ps1 -Headless
```

### 2. How to Run Phase 3 Memory Tests
```powershell
.\run.ps1 -Headless -Feature test-memory
```
**Verification Checks Executed by Test Suite:**
1. **Alignment & Uniqueness:** Allocates multiple frames, asserting distinct 4 KiB-aligned addresses.
2. **Allocation/Deallocation Cycle:** Frees and re-allocates a frame to verify reuse of freed slots.
3. **Double-Free Rejection:** Rejects duplicate frees with an explicit `Err` diagnostic.
4. **Invalid Deallocation Rejection:** Rejects unaligned and out-of-bounds addresses.
5. **Canonical Address Validation:** Verifies canonical form detection and higher-half P4 indices.
6. **Clean Recovery:** Deallocates all test frames cleanly without resource leakage.
