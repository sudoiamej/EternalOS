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
│   ├── Cargo.toml
│   ├── Cargo.lock
│   ├── rust-toolchain.toml # Locks to nightly & bare-metal target
│   ├── build.rs            # Passes linker script directly to lld
│   ├── linker-x86_64.ld    # High-half x86-64 ELF linker script (0xffffffff80000000)
│   └── src/
│       └── main.rs         # Kernel entry, COM1 16550 UART serial & framebuffer driver
└── tools/
    └── limine.exe          # Limine bootloader host deployment tool
```

---

## 🛠️ Build & Run Instructions (Native Windows)

### Prerequisites
1. **Rust**: Installed via `rustup` with target `x86_64-unknown-none` and component `llvm-tools-preview`.
2. **QEMU**: `qemu-system-x86_64` (installed in `C:\Program Files\qemu`).
3. **PowerShell**: Windows 11 PowerShell 5.1 / 7+.

### Quick Run
Execute the automated PowerShell runner from the project root:

```powershell
.\run.ps1
```

Or run headless in your terminal capturing serial logs:

```powershell
.\run.ps1 -Headless
```
