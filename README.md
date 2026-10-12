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

## 📋 Prerequisites & System Requirements

To build and run EternalOS locally, ensure the following host tools are installed and available on your system `PATH`:

### 1. Operating System
- **Windows 10 / 11** (PowerShell 5.1+ or PowerShell 7 Core)
  - Also runs on Linux / macOS if adapting `run.ps1` to shell or running cargo and QEMU directly.

### 2. Rust Toolchain (Nightly)
- **Rustup:** [https://rustup.rs](https://rustup.rs)
- **Rust Nightly Channel:**
  ```powershell
  rustup toolchain install nightly
  ```
- **Bare-metal x86-64 Target:**
  ```powershell
  rustup target add x86_64-unknown-none --toolchain nightly
  ```
- **LLVM Tools (optional, for disassembly/symbol inspection):**
  ```powershell
  rustup component add llvm-tools-preview --toolchain nightly
  ```

### 3. Hypervisor / Virtual Machine Emulator
- **QEMU for Windows (x86_64):**
  - Download from [https://www.qemu.org/download/#windows](https://www.qemu.org/download/#windows) or install via `winget`:
    ```powershell
    winget install SoftwareFreedomConservancy.QEMU
    ```
  - Standard installation directory: `C:\Program Files\qemu\`
  - Required binaries: `qemu-system-x86_64.exe` and `qemu-img.exe`
- **OVMF UEFI Firmware:**
  - Provided bundled by default with QEMU for Windows at:
    `C:\Program Files\qemu\share\edk2-x86_64-code.fd`
  - (If installed in a custom location, verify this file path or update `run.ps1`).

### 4. Git
- **Git for Windows:** [https://git-scm.com/download/win](https://git-scm.com/download/win)

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
│       ├── reliability_tests.rs # Phase 9 reliability, diagnostics, and invariant verification suite
│       ├── vmem_tests.rs   # Phase 10 advanced virtual memory & page-table test suite
│       ├── cpu.rs          # Phase 11 CPUID discovery & soft-float execution policy
│       ├── gdt.rs          # Phase 11 GDT, TSS, and IST exception stack configuration
│       ├── arch_tests.rs   # Phase 11 CPU architecture & interrupt hardening test suite
│       ├── pci.rs          # Phase 12 PCI configuration mechanism #1, BAR parser, device registry
│       └── pci_tests.rs    # Phase 12 PCI discovery & hardware enumeration test suite
└── tools/
    └── limine.exe          # Limine bootloader host deployment tool
```

## 💾 Disk Utilities & Eternal Purple Visual Identity (Milestone)

### 1. Architecture & Capabilities
- **Disk Utilities (`epart`, `efsck`, `eblock` in `shell.rs`):**
  - **`epart` (Partition Inspector - Read-Only):**
    - Inspects real block devices and parses MBR / GPT partition tables via `crate::partition::discover_partitions`.
    - Formats device identifier, sector count, logical sector size, partition indices, partition type codes, starting LBAs, sector counts, partition capacity in MiB (with 64-bit overflow prevention), bootable flags, and device capacity bounds validation.
    - `--verbose` mode renders GPT Unique GUIDs and UTF-8 partition label names.
  - **`efsck` (EternalFS Read-Only Consistency Checker):**
    - Validates attached storage filesystems without writing or modifying metadata.
    - Performs structural consistency checks: Superblock magic signature (`ETERNAL1`), IEEE 802.3 CRC32 checksum, filesystem version and block size, partition capacity geometry, root inode range, read-only mount initialization, root directory `readdir` traversal, and inode table direct block pointer bounds.
    - Provides truthful validation summary (Passed, Failed, and Unsupported Repair status).
  - **`eblock` (Block Device Diagnostics & Bounded Read-Test):**
    - Inspects active block storage controller metadata (`name`, logical sector size, total sectors, capacity in MiB, access permissions, PCI bus/device/function location).
    - `--read-test` flag runs a bounded 16-sector read test from LBA 0 through the block-device abstraction with elapsed PIT timer duration reporting.
    - Truthfully reports uninitialized state or unsupported SMART diagnostics on VirtIO virtual disks.
- **Eternal Purple Brand Unification & White Logo (`framebuffer.rs`, `console.rs`, `mouse.rs`):**
  - Centralized palette design tokens:
    - Primary Eternal Purple: `#7C3AED` (`Color::ACCENT_PURPLE`)
    - Secondary Purple: `#6D28D9` (`Color::ACCENT_SECONDARY`)
    - Soft Lavender: `#A78BFA` (`Color::ACCENT_LAVENDER`)
    - Highlight Lavender: `#C4B5FD` (`Color::ACCENT_HIGHLIGHT`)
    - Dark Background: `#111019` (`Color::DARK_BG`)
    - Logo Asset: Pure White `#FFFFFF` (`Color::WHITE`)
  - Replaced legacy cyan/blue branding accents across boot header bars, navigation lines, terminal frame borders, text cursor indicators, and mouse cursor interiors.
  - Semantic status colors preserved: Emerald Green (`#2ECC71`) for passing diagnostics, Amber (`#F39C12`) for scrollback indicators, Red (`#EF4444`) for errors, and Gray (`#948FA8`) for muted hardware info.

---

## 🌐 Modular Networking Stack & Intel E1000 Driver (Milestone)

### 1. Architecture & Capabilities
- **Intel 82540EM (E1000) Gigabit Ethernet Driver (`net/e1000.rs`):**
  - PCI device discovery via Vendor `0x8086`, Device `0x100E`.
  - Dynamic page table mapping for physical MMIO BAR0 (128 KiB memory-mapped control registers with `PAGE_WRITABLE | PAGE_NO_CACHE`).
  - Hardware reset & link status negotiation (`CTRL`, `STATUS`).
  - Hardware MAC address auto-discovery from EEPROM / RAL0/RAH0 registers.
  - Ring buffer circular DMA:
    - 32 Receive descriptors (`E1000RxDesc`) with circular head/tail tracking (`RDBAL`, `RDLEN`, `RDH`, `RDT`).
    - 32 Transmit descriptors (`E1000TxDesc`) with circular head/tail tracking (`TDBAL`, `TDLEN`, `TDH`, `TDT`).
    - End-of-Packet (`EOP`) and Insert-Checksum/Report-Status (`RS | IFCS`) descriptor flags.
    - Zero unbounded memory allocation: statically bounded ring DMA storage pool (`E1000DmaPool`).
- **Zero-Allocation Protocol Stack (`net/wire.rs`, `net/stack.rs`):**
  - **Ethernet II:** 14-byte frame header parser/serializer with big-endian byte-order validation.
  - **ARP:** RFC 826 ARP Request and Reply engine with a bounded 16-entry LRU cache (`ArpCache`).
  - **IPv4:** RFC 791 packet parsing and generation with standard 16-bit One's Complement internet checksum calculation and validation.
  - **ICMP:** RFC 792 Echo Request and Echo Reply engine. Auto-responds to incoming ping requests and generates outbound pings.
  - **UDP:** RFC 768 datagram processing with IPv4 pseudo-header checksum generation.
  - **TCP:** RFC 793 packet framing, checksum calculation, TCP flags (`SYN`, `ACK`, `FIN`, `RST`, `PSH`), and connection state machine foundation (`TcpState`).
  - **DNS:** RFC 1035 UDP client generating DNS queries on port 53, standard label-length encoding, and response parser resolving IPv4 `A` records (including compressed name pointers).
- **Interactive Network Commands in CLI (`shell.rs`):**
  - `enet`: Display active network device status, link state, hardware MAC, RX/TX packet counts, and byte counters.
  - `ip`: Display or inspect current IPv4 configuration (IP, subnet mask, gateway, DNS server).
  - `dhcp`: Acquire dynamic IP address configuration via RFC 2131/2132 DHCPDISCOVER/DHCPREQUEST broadcast and commit leased IP, subnet mask, gateway, and DNS server.
  - `ping <ip>`: Send ICMP Echo Request packets and report round-trip reply status.
  - `dns <domain>`: Send a live DNS lookup query to the configured DNS server and display resolved IPv4 address.
  - `tcp <ip> <port>`: Perform live TCP 3-way handshake (`SYN` -> `SYN-ACK` -> `ACK`), establish socket state, and send clean RST teardown to target host.
- **Deterministic Automated Test Suite (`test-net`):**
  - Run via: `powershell -ExecutionPolicy Bypass -File .\run.ps1 -Headless -Feature test-net`
  - Validates wire framing, ARP cache replacement, IPv4 checksums, ICMP echo serialization, UDP checksum calculation, TCP state progression, DNS A-record response parsing, DHCP packet serialization, and real hardware E1000 DMA packet transmission.

---

## 🔌 USB Subsystem & UHCI Device Enumeration (Milestone)

### 1. Architecture & Capabilities
- **Intel PIIX3 UHCI USB 1.1 Host Controller Driver (`usb/uhci.rs`):**
  - Discovers UHCI host controllers across PCI bus (Class `0x0C`, Subclass `0x03`, Prog IF `0x00`).
  - Reads I/O port base register from PCI BAR4 (`0x20` register, bit 0 I/O flag mask).
  - Configures and initializes 4 KiB page-aligned Frame List (1024 entries) pointing to Queue Heads (QH) and Transfer Descriptors (TD).
  - Starts controller run loop by clearing halt bit and setting run bit in `USBCMD`.
  - Performs root hub port detection, debounce delay, and root hub port reset via `PORTSC1` / `PORTSC2` (asserting `PORT_RESET` for 20ms, then clearing).
  - Detects attached device presence and device speed (`LowSpeed` 1.5 Mbps vs `FullSpeed` 12 Mbps).
  - Constructs and executes standard USB Control Transfers on Endpoint 0 using SETUP, DATA IN, and STATUS OUT Transfer Descriptors with hardware error retry count.
- **USB Descriptor Architecture (`usb/descriptor.rs`):**
  - Provides strict length-bounded parsers for standard USB descriptors:
    - **Device Descriptor:** 18 bytes (length, descriptor type, USB BCD version, device class/subclass/protocol, max packet size 0, vendor ID, product ID, release BCD, string indices, configurations count).
    - **Configuration Descriptor:** 9-byte header + total interface descriptors length.
    - **Interface Descriptor:** 9 bytes (interface number, alternate setting, endpoint count, interface class/subclass/protocol, interface string index).
    - **Endpoint Descriptor:** 7 bytes (endpoint address, transfer attributes, max packet size, polling interval).
    - **String Descriptor:** Decodes UTF-16LE characters to bounded ASCII strings.
- **USB Device Registry & CLI Inspection (`usb/device.rs`, `shell.rs`):**
  - Thread-safe `UsbRegistry` tracking controller ID, port number, assigned address, connection speed, device descriptor, and status.
  - Interactive CLI command `eusb` (alias `lsusb`):
    - Lists all active host controllers and enumerated USB devices.
    - Displays Port, Speed (LowSpeed 1.5Mbps / FullSpeed 12Mbps / HighSpeed 480Mbps / SuperSpeed 5Gbps), Vendor ID, Product ID, and Device Class.
    - `--verbose` flag inspects BCD USB specification version, Endpoint 0 packet size, and total configurations.
- **Deterministic Automated Test Suite (`test-usb`):**
  - Run via: `powershell -ExecutionPolicy Bypass -File .\run.ps1 -Headless -Feature test-usb`
  - Validates setup packet creation, device descriptor parsing, configuration & interface descriptor verification, endpoint descriptor bounds, UTF-16LE string decoding, and device registry operations.

---

## 🐚 Userspace, Process Execution & Command-Line Shell (Phase 15)

### 1. Architecture & Capabilities
- **x86-64 User Mode (Ring 3 / CPL 3):**
  - GDT user segment descriptors configured: `USER_DS` (selector `0x3B`, DPL 3) and `USER_CS` (selector `0x43`, DPL 3).
  - Task State Segment (TSS) maintains privileged stack pointer (`rsp0`), dynamically updated during process switches for secure transition on interrupts and exceptions.
- **Process Management & Address Space Isolation (`process.rs`):**
  - Isolated PML4 per process (`create_user_address_space`): Higher-half kernel page tables (entries 256..512) are mapped identically, while lower-half user space (entries 0..256) is completely isolated and private.
  - Page-table reclamation (`destroy_user_address_space`): Safely recurses through lower-half user PML4, PDPT, PD, and PT structures, unmapping and returning all physical frames to the frame allocator upon process exit.
  - User stack located at `0x0000_7FFF_FFFF_0000` (4 KiB guard/stack frame).
  - Genuine user mode entry via x86-64 `iretq` restoring `SS`, `RSP`, `RFLAGS` (`0x202` IF=1), `CS`, and `RIP`.
- **ELF64 Executable Loader (`elf.rs`):**
  - Validates ELF magic (`0x7F 'E' 'L' 'F'`), 64-bit ELF class, little-endian encoding, executable type (`ET_EXEC`), and machine target (`EM_X86_64`).
  - Verifies entry point canonicality within user virtual address space (`< 0x0000_8000_0000_0000`).
  - Loads and maps `PT_LOAD` segments with required user permissions (`PAGE_USER | PAGE_WRITABLE`), zero-filling BSS differences (`p_memsz > p_filesz`).
- **Controlled System Call ABI (`int 0x80`, `syscall.rs`, `syscall_handler.rs`):**
  - Registered IDT vector `0x80` with DPL 3 (`options = 0xEE00`).
  - Strict user-pointer validation (`validate_user_buffer`, `copy_user_string`): Rejects addresses in kernel memory (`>= 0x0000_8000_0000_0000`), null pointers, or buffers that wrap around 64-bit address space.
  - Implemented core system calls:
    - `SYS_EXIT` (0): Terminate calling process with exit code.
    - `SYS_WRITE` (1): Write bytes to stdout / stderr (renders to both UART serial and framebuffer console).
    - `SYS_READ` (2): Read bytes from stdin (UART COM1 line receiver).
    - `SYS_YIELD` (3): Voluntarily yield CPU time slice.
    - `SYS_SYSINFO` (4): Query kernel version, uptime ticks, total/free memory frames, and active task count.
    - `SYS_OPEN` (5), `SYS_CLOSE` (6), `SYS_READDIR` (7), `SYS_STAT` (8): VFS file and directory access.
    - `SYS_SPAWN` (9), `SYS_WAITPID` (10): Process spawning and lifecycle waiting.
- **Interactive Command-Line Shell (`shell.rs` & `console.rs`):**
  - Dual-output console rendering to serial COM1 and graphical framebuffer.
  - Interactive line editor with backspace handling, character echoing, and command dispatch.
  - Built-in commands: `help`, `clear`, `echo`, `pwd`, `ls`, `cd`, `cat`, `stat`, `uname`, `meminfo`, `exit`, `shutdown`, `reboot`.
- **System Lifecycle Management (`lifecycle.rs`, `shell.rs`):**
  - `exit`: Contextual shell termination. Protects the root supervisor shell from accidental termination by displaying a notification explaining root context and directing the operator to `shutdown` or `reboot`. Terminating child shell processes is dispatched when sub-shells are active.
  - `shutdown`: Initiates synchronized storage cache sync, disables hardware interrupts (`cli`), and issues platform power-off to QEMU ACPI / PIIX4 / Q35 power-off port (`0x604` with `0x2000`), APM port (`0xB004`), and debug exit (`0x501`). Halts CPU cleanly if unsupported by bare-metal platform.
  - `reboot`: Synchronizes storage caches, disables interrupts, and issues platform cold reset via PCI/ACPI reset control register (`0xCF9` with `0x06`), 8042 PS/2 keyboard controller pulse reset (`0x64` with `0xFE`), and triple-fault IDT reset fallback.
- **Deterministic Test Suite (`test-userspace`):**
  - Test 1: User segment descriptors & CPL3 configuration verification.
  - Test 2: User address space creation, higher-half kernel sharing, and lower-half user isolation.
  - Test 3: ELF64 executable loader validation, header parsing, entry bounds, and invalid ELF rejection.
  - Test 4: Syscall pointer protection: kernel pointer rejection, null pointer rejection, and overflow bounds guarding.
  - Test 5: Process lifecycle & simulated user-mode execution via `int 0x80`.
  - Test 6: Interactive shell parser and command execution verification.

---

## 📂 EternalFS Read-Only On-Disk Filesystem (Phase 14C)

### 1. Architecture & Specification (`docs/eternalfs-spec.md`)
- **On-Disk Binary Format:**
  - Standard filesystem block size: `1024` bytes (2 physical 512-byte sectors per block).
  - Superblock at Block 1 (LBA 2 within partition): Magic `ETERNAL1`, version `1`, block size `1024`, total blocks, total inodes, inode table block, data block start, root inode (`1`), feature flags, and IEEE 802.3 CRC32 checksum.
  - Inode Table: 64-byte records, 1-indexed. Contains node type (Regular File / Directory), permission flags, 64-bit file size, 10 direct block pointers (`10 * 1024 = 10,240` bytes capacity), 1 single indirect block pointer (`+ 256 KiB`), and CRC32 checksum.
  - Directory Entries: Contiguous 32-byte records storing referenced inode ID, type, filename length, and up to 26 bytes of ASCII/UTF-8 filename string.
- **Read-Only Driver Implementation (`vfs/eternalfs.rs`):**
  - Implements the Phase 14B `FileSystem` trait for any `BlockDevice + Send + Sync`.
  - Safe interior mutability using `SpinLock<D>` wrapper for block device I/O.
  - Full path lookup (`lookup`), metadata retrieval (`get_metadata`), sequential and offset-based file reading (`read`), cross-block boundary reads, sparse hole zero-filling, and directory enumeration (`readdir`).
  - Read-Only Invariants Enforced: Mutation operations (`write`, `truncate`, `create_file`, `create_dir`) are strictly rejected with `VfsError::PermissionDenied`.
- **Host-Side Fixture Generator (`tools/make_eternalfs.ps1`):**
  - Formats test partitions on disposable raw images (`$env:TEMP\eternalos_test_disk.img`).
  - Generates known fixtures: `/hello.txt`, `/empty.txt` (0 bytes, EOF validation), `/multiblock.txt` (1,500 bytes spanning direct blocks with 'A' and 'B' patterns), and `/docs/kernel.txt` (nested directory).
- **Deterministic Test Suite (`test-eternalfs`):**
  - Test 1: Superblock decoding, version checks, CRC verification, bad magic rejection, and partition boundary checks.
  - Test 2: Inode decoding, CRC32 validation, and illegal node type rejection.
  - Test 3: In-memory synthetic filesystem unit testing: directory traversal, file offset reads, cross-block boundary reading, EOF handling, directory enumeration, and write permission denial.
  - Test 4: Real hardware integration test on VirtIO block controller: partition discovery -> EternalFS mount -> `/hello.txt` lookup & read -> `/empty.txt` EOF check -> `/multiblock.txt` cross-block verification -> `/docs/kernel.txt` nested read -> root directory enumeration.

---

## 📁 Virtual Filesystem (VFS) Layer & RamFs (Phase 14B)

### 1. Architecture & Capabilities
- **Hardware-Independent VFS Architecture (`vfs/mod.rs`):**
  - Abstract filesystem interface (`FileSystem` trait): `root_inode`, `lookup`, `get_metadata`, `create_file`, `create_dir`, `read`, `write`, `truncate`, and `readdir`.
  - Node & Metadata model: `InodeId` (64-bit identifier), `NodeType` (`RegularFile`, `Directory`), `Metadata` (size, type, permissions), and `DirEntry`.
  - Unified error handling (`VfsError`): `NotFound`, `AlreadyExists`, `NotADirectory`, `IsADirectory`, `InvalidArgument`, `DirectoryNotEmpty`, `PermissionDenied`, `TableOverflow`, `NoSpace`, `InvalidPath`, `InvalidHandle`, and `ArithmeticOverflow`.
- **Path Normalization & Canonicalization (`vfs/path.rs`):**
  - Dedicated normalization algorithm (`normalize_path`) and component iterator (`PathComponentIterator`).
  - Resolves `.` (current directory), collapses `..` (parent directory, clamped at `/`), collapses multiple consecutive slashes (`///`), and strips trailing slashes.
  - Rejects relative paths, oversized path components (`MAX_NAME_LEN = 64`), and paths exceeding buffer limits (`MAX_PATH_LEN = 256`).
- **In-Memory Filesystem Backend (`vfs/ramfs.rs`):**
  - Volatile RamFs implementing the `FileSystem` trait.
  - Statically bounded memory model: 32 nodes (`RAMFS_MAX_NODES`), 16 children per directory (`RAMFS_MAX_CHILDREN`), and 512-byte payload capacity per file (`RAMFS_FILE_MAX_SIZE`), completely isolated from hardware storage.
  - Supports file creation, directory creation, hierarchical traversal, bounded read/write with auto-expansion, and file truncation.
- **Global VFS Router & Mount Table (`vfs/router.rs`):**
  - Mount point registry supporting root mounting at `/`.
  - Global `SpinLock<VfsRouter>` with safe handle lifecycle tracking:
    - Bounded file table (up to 32 open files).
    - Monotonically incrementing generation counters (`FileHandle { fd, generation }`) to detect and reject stale file handles.
    - Automatic sequential offset tracking and EOF semantics.
- **Deterministic Test Suite (`test-vfs`):**
  - Test 1: Path normalization, canonicalization, repeated slashes, `.`/`..` resolution, and boundary guards.
  - Test 2: Isolated RamFs file/directory creation, duplicate rejection, payload read/write, truncation, and readdir enumeration.
  - Test 3: High-level VFS lifecycle: file creation, write, stat, sequential read, and EOF.
  - Test 4: Nested directory hierarchies, relative navigation across subdirectories, and non-directory traversal rejection.
  - Test 5: File handle security: closed handle rejection, stale generation detection, and active mount point verification.

---

## 🗂️ Read-Only GPT & MBR Partition Table Parsing (Phase 14A)

### 1. Architecture & Capabilities
- **Normalized Partition Representation (`partition.rs`):**
  - Unified `Partition` struct: start LBA, sector count, exclusive end LBA, table type (`MBR` or `GPT`), partition type (`Mbr(u8)` or `Gpt(Guid)`), unique partition GUID, attributes, UTF-8 name (safely decoded from UTF-16LE), and bootable indicator.
  - Bounded static registry (`PartitionTable`, up to 16 partitions): zero dynamic heap allocation during early initialization.
  - Granular error model (`PartitionError`): captures signature invalidity, CRC32 mismatches, boundary violations, overlapping partitions, duplicate GUIDs, and unsupported geometries.
- **Legacy MBR Parser:**
  - Validates `0x55AA` boot signature at sector 0 bytes 510..511.
  - Validates boot indicators (`0x00` or `0x80`), partition types, and non-empty entries.
  - Detects GPT Protective MBRs (`type == 0xEE`) and transparently routes parsing to GPT.
  - Detects extended partition types (`0x05`, `0x0F`, `0x85`) and reports documented rejection rather than misinterpreting as primary partitions.
  - Enforces boundary limits against device capacity and overlapping partition checks.
- **GUID Partition Table (GPT) Parser:**
  - Verifies `"EFI PART"` signature at LBA 1 and header size boundaries (92..=512 bytes).
  - Validates Header CRC32 (with CRC field cleared) via IEEE 802.3 standard implementation.
  - Verifies disk GUID, current LBA (1), backup LBA, and usable LBA ranges against device capacity.
  - Reads and validates partition entry array CRC32.
  - Decodes mixed-endian GUIDs and UTF-16LE partition names with fallback replacement for unmapped code points.
  - Enforces usable LBA boundaries, duplicate unique GUID prevention, and non-overlapping partitions.
- **Strict Read-Only Execution:**
  - Parser exclusively issues `read_sectors` operations against the underlying block device; no write requests are issued.
- **Deterministic Test Suite (`test-partitions`):**
  - Test 1: IEEE 802.3 standard CRC32 algorithm verification against standard check vectors.
  - Test 2: GUID little-endian parsing, formatting, and zero-checks.
  - Test 3: Synthetic MBR scenarios (valid empty, valid primary, missing signature, invalid boot indicator, out of bounds, overlapping partitions, extended partition rejection, protective MBR detection).
  - Test 4: Synthetic GPT scenarios (valid single partition, header CRC mismatch, entry array CRC mismatch, invalid header size, partition out of usable range).
  - Test 5: Real VirtIO hardware partition discovery and validation against attached QEMU disks.

---

## 💾 Block Storage Foundation & VirtIO Driver (Phase 13B)

### 1. Architecture & Capabilities
- **Hardware-Independent Block Layer (`block.rs`):**
  - Trait `BlockDevice`: provides device capabilities (`BlockDeviceInfo`), bounded sector reads (`read_sectors`), bounded sector writes (`write_sectors`), and cache flush operations (`flush`).
  - Validation engine (`validate_request`): protects against zero-sector requests, end-of-device range overruns, buffer truncation, and arithmetic overflow.
  - Granular error classification (`BlockError`): handles out-of-bounds, buffer misalignment, controller timeouts, hardware errors, read-only policies, and DMA allocation failures.
- **VirtIO Block Hardware Driver (`virtio_blk.rs`):**
  - Legacy PCI transport driver communicating over I/O ports discovered via PCI BAR0 (`0x6000`).
  - Statically allocated, page-aligned DMA ring buffer (`VirtioDmaPool`, 16 KiB BSS storage):
    - Page 0: 256 Split Ring Descriptors (aligned 16 bytes).
    - Page 1: 256 Available Ring entries (`VirtqAvail`).
    - Page 2: 256 Used Ring entries (`VirtqUsed`, strictly aligned to 4096-byte boundary).
    - Page 3: Request headers, status indicators, and 4096-byte bounce buffer.
  - Safe physical address translation via kernel PML4 page tables (`translate_addr`).
  - Synchronous bounded polling with memory fences (`fence(SeqCst)`) for deterministic command completion.
- **Disposable QEMU Test Disk:**
  - Configured and automatically created in `$env:TEMP\eternalos_test_disk.img` (10 MiB raw image).
  - Attached via QEMU CLI arguments `-drive id=testdisk,file=...,format=raw,if=none -device virtio-blk-pci,drive=testdisk`.
- **Deterministic Test Suite (`test-storage`):**
  - Test 1: Bounds validation and arithmetic overflow rejection.
  - Test 2: In-memory RAM disk single sector roundtrip.
  - Test 3: In-memory RAM disk multi-sector (4 sectors / 2048 bytes) integrity.
  - Test 4: Read-only protection policy enforcement.
  - Test 5: Real VirtIO hardware sector write and read-after-write verification on the QEMU test disk.

---

## 🎨 Framebuffer & Early Graphics Subsystem (Phase 13A)

### 1. Architecture & Capabilities
- **Modular Framebuffer Abstraction:** `Framebuffer` encapsulation representing the active linear framebuffer supplied by Limine (`1280x800 @ 32 bpp`, pitch `5120` bytes, BGRX/RGBX pixel formats).
- **Physical Memory Resolution:** Resolved physical framebuffer base address (`0x0000000080001000`) via kernel PML4 virtual-to-physical address translation.
- **2D Rendering Primitives:** Zero-allocation, bounds-checked rendering engine:
  - `clear(color)`: Fast 32-bit row-stride clearing.
  - `put_pixel(x, y, color)`: Individual pixel plotting with pitch and clipping calculation.
  - `fill_rect(rect, color)` / `draw_rect_border(rect, color, thickness)`: Bounded rectangle fill and outline operations.
  - `draw_hline(x, y, length, color)` / `draw_vline(x, y, length, color)`: Fast horizontal and vertical stroke lines.
  - `draw_char(x, y, char, fg, bg)` / `draw_text(x, y, text, fg, bg)`: Monospaced 8x16 font rendering with automatic ASCII conversion and clipping.
  - `draw_monochrome_bitmap(x, y, width, height, data, fg, bg)`: 1-bit per pixel monochrome bitmap renderer.
- **Embedded Visual Assets:**
  - `font.rs`: Embedded 8x16 monospaced ASCII bitmap font covering standard printable ASCII range (`0x20..=0x7E`) with fallback glyph for unmapped code points.
  - `logo_asset.rs`: 64x58 1-bit monochrome EternalOS emblem generated from `assets/logo.png`.
- **Graphical Boot Screen:** Modern dark-mode dashboard featuring top navigation bar, central branding card with logo and wordmark, diagnostics status grid (Memory, CPU, Paging, PCI), and a live status footer.
- **Deterministic Test Suite (`test-graphics`):** 12 comprehensive unit tests validating address calculation, stride padding, color packing, clipping boundaries, integer overflow safeguards, font glyph lookup, and logo geometry.

---

## 🔌 PCI/PCIe Enumeration & Hardware Discovery (Phase 12)

### 1. Architecture & Capabilities
- **Configuration Mechanism #1 Access:** Encapsulated I/O-port access (`0xCF8` address, `0xCFC` data) protected by interrupt-safe `SpinLock`.
- **Bounds & Alignment Validation:** Validates bus (0..255), device (0..31), function (0..7), and register offset alignments (32-bit dword, 16-bit word, 8-bit byte).
- **Safe Hardware Discovery:** Distinguishes absent devices (`0xFFFF`), parses single-function vs multifunction devices (bit 7 of header type), and enumerates secondary buses discovered via Type 1 PCI-to-PCI bridges.
- **Resource & BAR Interpretation:** Pure, non-destructive parsing of Type 0 (6 BARs) and Type 1 (2 BARs) headers, decoding I/O ports vs 32-bit / 64-bit MMIO addresses and prefetchability flags without writing or probing registers.
- **Bounded Registry:** Thread-safe global `PciRegistry` holding discovered functions with deterministic overflow tracking.

---

## ⚙️ CPU Architecture & Interrupt Infrastructure Hardening (Phase 11)

### 1. Architecture & Invariants
- **Dedicated 64-bit GDT:** Standard 64-bit kernel code (`0x28`) and data (`0x30`) descriptors reloaded via `retfq` far jump.
- **Task State Segment (TSS) & IST:** 64-bit TSS descriptor (`0x38`) loaded with `ltr`, configuring dedicated 16 KiB 16-byte aligned exception stack for `IST1` bound to Double Fault (`#DF`, vector 8).
- **CPUID Capability Detection:** Discovers Long Mode, NX bit, APIC, PAE, and SIMD features, enforcing soft-float integer kernel execution policy.

---

---

## 🗺️ Advanced Virtual Memory & x86-64 Page Tables (Phase 10)

### 1. Architecture & Capabilities
- **4-Level Paging Walk:** Full standard x86-64 page-table hierarchy (`PML4` -> `PDPT` -> `PD` -> `PT`) walking and inspection via Higher-Half Direct Map (`HHDM`).
- **Translation:** `translate_addr(vaddr)` converts canonical virtual addresses into physical addresses, supporting 4 KiB leaves as well as 2 MiB and 1 GiB huge page mappings.
- **Mapping Operations:**
  - `map_page(vaddr, pframe, flags)`: Allocates intermediate tables on demand, binds 4 KiB leaf entry, and flushes TLB via `invlpg`.
  - `unmap_page(vaddr)`: Clears leaf entry, flushes TLB, and automatically reclaims empty intermediate tables (`PT`, `PD`, `PDPT`) back to the Physical Memory Manager (PMM).
  - `update_page_flags(vaddr, flags)`: Atomically updates leaf entry permissions and flushes local TLB without altering physical address binding.
- **Decoupled Lifetime:** Unmapping a page strictly severs the virtual-to-physical translation without automatically freeing the underlying physical frame unless the caller explicitly deallocates it.
- **Hardware Permission Enforcement:** Verified hardware enforcement of read-only permissions via isolated fault testing (`test-vmem-fault`).

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

### 2. Run Phase 15 Userspace, Syscalls & Shell Tests
```powershell
.\run.ps1 -Headless -Feature test-userspace
```

### 3. Run Phase 14C EternalFS Read-Only Filesystem Tests
```powershell
.\run.ps1 -Headless -Feature test-eternalfs
```

### 3. Run Phase 14B Virtual Filesystem (VFS) Tests
```powershell
.\run.ps1 -Headless -Feature test-vfs
```

### 4. Run Phase 14A Partition Discovery Tests
```powershell
.\run.ps1 -Headless -Feature test-partitions
```

### 5. Run Phase 13B Block Storage & VirtIO Tests
```powershell
.\run.ps1 -Headless -Feature test-storage
```

### 6. Run Phase 13A Graphics Tests
```powershell
.\run.ps1 -Headless -Feature test-graphics
```

### 7. Run Phase 12 PCI Enumeration & Hardware Discovery Tests
```powershell
.\run.ps1 -Headless -Feature test-pci
```

### 3. Run Phase 11 CPU Architecture & Interrupt Hardening Tests
```powershell
.\run.ps1 -Headless -Feature test-arch
```

### 4. Run Phase 10 Virtual Memory & Page Table Tests
```powershell
.\run.ps1 -Headless -Feature test-vmem
```

### 3. Run Controlled Exception & Fault Tests
```powershell
.\run.ps1 -Headless -Feature test-vmem-fault
.\run.ps1 -Headless -Feature test-pagefault
.\run.ps1 -Headless -Feature test-gpf
.\run.ps1 -Headless -Feature test-invalid-opcode
.\run.ps1 -Headless -Feature test-div-zero
.\run.ps1 -Headless -Feature test-panic
```

### 4. Run Prior Subsystem Test Suites (Regression Verification)
```powershell
.\run.ps1 -Headless -Feature test-reliability
.\run.ps1 -Headless -Feature test-sleep
.\run.ps1 -Headless -Feature test-sync
.\run.ps1 -Headless -Feature test-preempt
.\run.ps1 -Headless -Feature test-task
.\run.ps1 -Headless -Feature test-heap
.\run.ps1 -Headless -Feature test-memory
```
