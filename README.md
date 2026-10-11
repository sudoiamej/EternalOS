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
│       └── mm.rs           # Limine physical memory map parser & range validation
└── tools/
    └── limine.exe          # Limine bootloader host deployment tool
```

---

## 🛠️ Build & Run Instructions (Native Windows)

### Prerequisites
1. **Rust**: Installed via `rustup` with target `x86_64-unknown-none` and component `llvm-tools-preview`.
2. **QEMU**: `qemu-system-x86_64` (installed in `C:\Program Files\qemu`).
3. **PowerShell**: Windows 11 PowerShell 5.1 / 7+.

### 1. Normal Kernel Boot
To build and launch the kernel normally in QEMU:
```powershell
.\run.ps1
```
Or run headless in the terminal to inspect serial output directly:
```powershell
.\run.ps1 -Headless
```

### 2. How to Test Kernel Subsystems & Exceptions
Phase 2 includes isolated, compile-time test features to verify exception handling and memory inspection without affecting normal runs:

#### Test A: Normal Memory-Map Inspection
Run normally with `-Headless`. The kernel enumerates all physical memory entries provided by Limine, calculates totals (Usable, Reserved, Reclaimable), and validates address bounds against arithmetic overflow.

#### Test B: Divide-by-Zero Exception (#DE)
```powershell
.\run.ps1 -Headless -Feature test-div-zero
```
*Expected Result:* The CPU triggers interrupt vector 0 (`#DE`), prints the full saved `InterruptStackFrame` (RIP, CS, RFLAGS, RSP, SS) to serial, and enters safe halt (`hcf`).

#### Test C: Invalid Opcode Exception (#UD)
```powershell
.\run.ps1 -Headless -Feature test-invalid-opcode
```
*Expected Result:* The CPU executes an undefined instruction (`ud2`), triggers interrupt vector 6 (`#UD`), dumps the faulting stack frame, and safely halts.

#### Test D: Controlled Kernel Panic
```powershell
.\run.ps1 -Headless -Feature test-panic
```
*Expected Result:* The panic handler captures the source file, line number, and panic message, outputs formatted diagnostic logs to serial, and prevents recursive panics by halting.
