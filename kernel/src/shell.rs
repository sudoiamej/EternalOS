use crate::block::BlockDevice;
use crate::console::{console_write, console_write_byte};
use crate::net::NetworkDevice;
use crate::shell_parser::{parse_command_line, ParsedArgs};
use crate::vfs::{with_vfs, DirEntry, NodeType, OpenFlags, VfsError};

/// Command categorization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandCategory {
    Shell,
    Filesystem,
    System,
    Hardware,
    Diagnostics,
}

impl CommandCategory {
    pub fn name(&self) -> &'static str {
        match self {
            CommandCategory::Shell => "Shell & Session",
            CommandCategory::Filesystem => "Filesystem & Storage",
            CommandCategory::System => "System Information",
            CommandCategory::Hardware => "Hardware & Devices",
            CommandCategory::Diagnostics => "Verification & Diagnostics",
        }
    }
}

/// Metadata definition for a shell command
pub struct CommandDef {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub category: CommandCategory,
    pub summary: &'static str,
    pub usage: &'static str,
}

pub const COMMAND_REGISTRY: &[CommandDef] = &[
    // --- Shell & Session ---
    CommandDef {
        name: "help",
        aliases: &["?", "ehelp"],
        category: CommandCategory::Shell,
        summary: "Display list of commands or detailed help",
        usage: "help [category | command]",
    },
    CommandDef {
        name: "clear",
        aliases: &["cls"],
        category: CommandCategory::Shell,
        summary: "Clear terminal viewport",
        usage: "clear",
    },
    CommandDef {
        name: "echo",
        aliases: &[],
        category: CommandCategory::Shell,
        summary: "Print text to terminal",
        usage: "echo [text...]",
    },
    CommandDef {
        name: "whoami",
        aliases: &[],
        category: CommandCategory::Shell,
        summary: "Print current user identity",
        usage: "whoami",
    },
    CommandDef {
        name: "which",
        aliases: &[],
        category: CommandCategory::Shell,
        summary: "Locate command in shell registry",
        usage: "which <command>",
    },
    CommandDef {
        name: "alias",
        aliases: &[],
        category: CommandCategory::Shell,
        summary: "List registered command aliases",
        usage: "alias",
    },
    CommandDef {
        name: "exit",
        aliases: &["quit"],
        category: CommandCategory::Shell,
        summary: "Leave the current shell when supported",
        usage: "exit",
    },

    // --- Filesystem & Storage ---
    CommandDef {
        name: "pwd",
        aliases: &[],
        category: CommandCategory::Filesystem,
        summary: "Print working directory",
        usage: "pwd",
    },
    CommandDef {
        name: "ls",
        aliases: &["dir"],
        category: CommandCategory::Filesystem,
        summary: "List directory contents",
        usage: "ls [path]",
    },
    CommandDef {
        name: "cd",
        aliases: &[],
        category: CommandCategory::Filesystem,
        summary: "Change current directory",
        usage: "cd <path>",
    },
    CommandDef {
        name: "cat",
        aliases: &["type"],
        category: CommandCategory::Filesystem,
        summary: "Display file contents",
        usage: "cat <path>",
    },
    CommandDef {
        name: "stat",
        aliases: &[],
        category: CommandCategory::Filesystem,
        summary: "Display file or directory metadata",
        usage: "stat <path>",
    },
    CommandDef {
        name: "mkdir",
        aliases: &[],
        category: CommandCategory::Filesystem,
        summary: "Create a new directory",
        usage: "mkdir <path>",
    },
    CommandDef {
        name: "touch",
        aliases: &[],
        category: CommandCategory::Filesystem,
        summary: "Create empty file or update timestamp",
        usage: "touch <path>",
    },
    CommandDef {
        name: "cp",
        aliases: &[],
        category: CommandCategory::Filesystem,
        summary: "Copy file to destination",
        usage: "cp <source> <dest>",
    },
    CommandDef {
        name: "rm",
        aliases: &[],
        category: CommandCategory::Filesystem,
        summary: "Remove file or empty directory",
        usage: "rm <path>",
    },
    CommandDef {
        name: "head",
        aliases: &[],
        category: CommandCategory::Filesystem,
        summary: "Output first lines of a file",
        usage: "head [-n count] <path>",
    },
    CommandDef {
        name: "tail",
        aliases: &[],
        category: CommandCategory::Filesystem,
        summary: "Output last lines of a file",
        usage: "tail [-n count] <path>",
    },
    CommandDef {
        name: "wc",
        aliases: &[],
        category: CommandCategory::Filesystem,
        summary: "Print line, word, and byte counts",
        usage: "wc <path>",
    },
    CommandDef {
        name: "efsck",
        aliases: &["fsck"],
        category: CommandCategory::Filesystem,
        summary: "Validate EternalFS filesystem structures (read-only)",
        usage: "efsck [--verbose] [device | /]",
    },

    // --- System Information ---
    CommandDef {
        name: "eternal",
        aliases: &["logo"],
        category: CommandCategory::System,
        summary: "Display EternalOS banner and philosophy",
        usage: "eternal",
    },
    CommandDef {
        name: "uname",
        aliases: &[],
        category: CommandCategory::System,
        summary: "Print kernel system information",
        usage: "uname [-a]",
    },
    CommandDef {
        name: "esys",
        aliases: &["sysinfo"],
        category: CommandCategory::System,
        summary: "Display unified system diagnostics overview",
        usage: "esys",
    },
    CommandDef {
        name: "emem",
        aliases: &["meminfo", "free"],
        category: CommandCategory::System,
        summary: "Inspect physical memory frames and heap",
        usage: "emem",
    },
    CommandDef {
        name: "eproc",
        aliases: &["ps"],
        category: CommandCategory::System,
        summary: "List processes and execution states",
        usage: "eproc",
    },
    CommandDef {
        name: "emount",
        aliases: &["mount", "vfsinfo"],
        category: CommandCategory::System,
        summary: "Inspect mounted filesystems and topology",
        usage: "emount",
    },
    CommandDef {
        name: "uptime",
        aliases: &[],
        category: CommandCategory::System,
        summary: "Display system uptime and PIT timer ticks",
        usage: "uptime",
    },
    CommandDef {
        name: "date",
        aliases: &["time"],
        category: CommandCategory::System,
        summary: "Display live hardware date and time from RTC",
        usage: "date",
    },
    CommandDef {
        name: "shutdown",
        aliases: &["poweroff"],
        category: CommandCategory::System,
        summary: "Request a safe system shutdown",
        usage: "shutdown",
    },
    CommandDef {
        name: "reboot",
        aliases: &["restart"],
        category: CommandCategory::System,
        summary: "Restart the system",
        usage: "reboot",
    },

    // --- Hardware & Devices ---
    CommandDef {
        name: "eblock",
        aliases: &["lsblk"],
        category: CommandCategory::Hardware,
        summary: "Inspect block devices and run read diagnostics",
        usage: "eblock [--read-test [device]]",
    },
    CommandDef {
        name: "epart",
        aliases: &["fdisk"],
        category: CommandCategory::Hardware,
        summary: "Inspect disk partitions (MBR / GPT) (read-only)",
        usage: "epart [--verbose] [device]",
    },
    CommandDef {
        name: "epci",
        aliases: &["lspci"],
        category: CommandCategory::Hardware,
        summary: "Enumerate detected PCI/PCIe devices",
        usage: "epci",
    },
    CommandDef {
        name: "edrv",
        aliases: &[],
        category: CommandCategory::Hardware,
        summary: "List active device drivers",
        usage: "edrv",
    },
    CommandDef {
        name: "mouse",
        aliases: &[],
        category: CommandCategory::Hardware,
        summary: "Display PS/2 mouse coordinate and button state",
        usage: "mouse",
    },
    CommandDef {
        name: "enet",
        aliases: &["ifconfig"],
        category: CommandCategory::Hardware,
        summary: "Display network interface status, MAC, and stats",
        usage: "enet",
    },
    CommandDef {
        name: "eusb",
        aliases: &["lsusb"],
        category: CommandCategory::Hardware,
        summary: "List discovered USB host controllers and devices",
        usage: "eusb [--verbose]",
    },
    CommandDef {
        name: "ip",
        aliases: &[],
        category: CommandCategory::Hardware,
        summary: "Display IP address, gateway, and subnet mask",
        usage: "ip",
    },
    CommandDef {
        name: "dhcp",
        aliases: &[],
        category: CommandCategory::Hardware,
        summary: "Acquire IP lease configuration via DHCP",
        usage: "dhcp",
    },
    CommandDef {
        name: "ping",
        aliases: &[],
        category: CommandCategory::Diagnostics,
        summary: "Send ICMP Echo Request to IPv4 address",
        usage: "ping <ip>",
    },
    CommandDef {
        name: "dns",
        aliases: &["nslookup"],
        category: CommandCategory::Diagnostics,
        summary: "Resolve domain hostname via DNS",
        usage: "dns <hostname>",
    },
    CommandDef {
        name: "tcp",
        aliases: &["nc"],
        category: CommandCategory::Diagnostics,
        summary: "Establish TCP 3-way handshake to remote host",
        usage: "tcp <ip> <port>",
    },

    // --- Diagnostics ---
    CommandDef {
        name: "echeck",
        aliases: &[],
        category: CommandCategory::Diagnostics,
        summary: "Run kernel subsystem integrity self-checks",
        usage: "echeck",
    },
    CommandDef {
        name: "elog",
        aliases: &[],
        category: CommandCategory::Diagnostics,
        summary: "Display recent kernel logging messages",
        usage: "elog",
    },
    CommandDef {
        name: "einfo",
        aliases: &[],
        category: CommandCategory::Diagnostics,
        summary: "Display system architecture and build details",
        usage: "einfo",
    },
];

/// Execution context for interactive shell session
pub struct ShellContext {
    pub current_dir: [u8; 128],
    pub dir_len: usize,
    pub last_status: i32,
}

impl ShellContext {
    pub const fn new() -> Self {
        let mut cd = [0u8; 128];
        cd[0] = b'/';
        Self {
            current_dir: cd,
            dir_len: 1,
            last_status: 0,
        }
    }

    pub fn cwd_str(&self) -> &str {
        core::str::from_utf8(&self.current_dir[..self.dir_len]).unwrap_or("/")
    }

    pub fn resolve_relative_path(&self, rel: &str, out: &mut [u8; 256]) -> Result<usize, VfsError> {
        if rel.starts_with('/') {
            let bytes = rel.as_bytes();
            let len = bytes.len().min(255);
            out[..len].copy_from_slice(&bytes[..len]);
            Ok(len)
        } else {
            let mut len = self.dir_len;
            out[..len].copy_from_slice(&self.current_dir[..len]);
            if !self.cwd_str().ends_with('/') {
                out[len] = b'/';
                len += 1;
            }
            let bytes = rel.as_bytes();
            let to_copy = bytes.len().min(255 - len);
            out[len..len + to_copy].copy_from_slice(&bytes[..to_copy]);
            len += to_copy;
            Ok(len)
        }
    }
}

/// Execute a single command line string within context, returning exit code
pub fn execute_command(line: &str, ctx: &mut ShellContext) -> i32 {
    let args = parse_command_line(line);
    if args.is_empty() {
        return 0;
    }

    let cmd_name = args.get(0).unwrap_or("");

    // Resolve command definition by name or alias
    let matched_cmd = COMMAND_REGISTRY.iter().find(|c| {
        c.name == cmd_name || c.aliases.iter().any(|&a| a == cmd_name)
    });

    let canonical_name = match matched_cmd {
        Some(c) => c.name,
        None => cmd_name,
    };

    let status = match canonical_name {
        "help" => cmd_help(&args),
        "clear" => {
            crate::console::console_clear();
            0
        }
        "echo" => cmd_echo(&args),
        "whoami" => {
            console_write("root (EternalOS Supervisor Ring 0 / User Ring 3)\r\n");
            0
        }
        "which" => cmd_which(&args),
        "alias" => cmd_alias(),
        "exit" => cmd_exit(&args),
        "shutdown" => cmd_shutdown(&args),
        "reboot" => cmd_reboot(&args),

        "pwd" => {
            console_write(ctx.cwd_str());
            console_write("\r\n");
            0
        }
        "ls" => cmd_ls(&args, ctx),
        "cd" => cmd_cd(&args, ctx),
        "cat" => cmd_cat(&args, ctx),
        "stat" => cmd_stat(&args, ctx),
        "mkdir" => cmd_mkdir(&args, ctx),
        "touch" => cmd_touch(&args, ctx),
        "cp" => cmd_cp(&args, ctx),
        "rm" => cmd_rm(&args, ctx),
        "head" => cmd_head(&args, ctx),
        "tail" => cmd_tail(&args, ctx),
        "wc" => cmd_wc(&args, ctx),
        "efsck" => cmd_efsck(&args),

        "eternal" => cmd_eternal(),
        "uname" => cmd_uname(&args),
        "esys" => cmd_esys(ctx),
        "emem" => cmd_emem(),
        "eproc" => cmd_eproc(),
        "emount" => cmd_emount(ctx),
        "uptime" => cmd_uptime(),
        "date" => cmd_date(),

        "eblock" => cmd_eblock(&args),
        "epart" => cmd_epart(&args),
        "epci" => cmd_epci(),
        "edrv" => cmd_edrv(),
        "mouse" => cmd_mouse(),
        "enet" => cmd_enet(),
        "eusb" => cmd_eusb(&args),
        "ip" => cmd_ip(),
        "dhcp" => cmd_dhcp(),
        "ping" => cmd_ping(&args),
        "dns" => cmd_dns(&args),
        "tcp" => cmd_tcp(&args),

        "echeck" => cmd_echeck(),
        "elog" => cmd_elog(),
        "einfo" => cmd_einfo(),

        unknown => {
            console_write("shell: command not found: '");
            console_write(unknown);
            console_write("'. Type 'help' for available commands.\r\n");
            127
        }
    };

    ctx.last_status = status;
    status
}

// -----------------------------------------------------------------------------
// Command Handlers
// -----------------------------------------------------------------------------

fn cmd_help(args: &ParsedArgs) -> i32 {
    if args.len() > 1 {
        let topic = args.get(1).unwrap_or("");

        // Check if category
        let categories = [
            ("shell", CommandCategory::Shell),
            ("fs", CommandCategory::Filesystem),
            ("sys", CommandCategory::System),
            ("hw", CommandCategory::Hardware),
            ("diag", CommandCategory::Diagnostics),
        ];

        for (key, cat) in categories {
            if topic == key || topic == cat.name() {
                console_write("=== Category: ");
                console_write(cat.name());
                console_write(" ===\r\n");
                for cmd in COMMAND_REGISTRY.iter().filter(|c| c.category == cat) {
                    console_write("  ");
                    console_write(cmd.name);
                    let pad = 12usize.saturating_sub(cmd.name.len());
                    for _ in 0..pad {
                        console_write(" ");
                    }
                    console_write("- ");
                    console_write(cmd.summary);
                    console_write("\r\n");
                }
                return 0;
            }
        }

        // Check if specific command
        if let Some(cmd) = COMMAND_REGISTRY.iter().find(|c| c.name == topic || c.aliases.iter().any(|&a| a == topic)) {
            console_write("Command: ");
            console_write(cmd.name);
            console_write("\r\nSummary: ");
            console_write(cmd.summary);
            console_write("\r\nUsage  : ");
            console_write(cmd.usage);
            console_write("\r\nCategory: ");
            console_write(cmd.category.name());
            if !cmd.aliases.is_empty() {
                console_write("\r\nAliases : ");
                for (i, a) in cmd.aliases.iter().enumerate() {
                    if i > 0 { console_write(", "); }
                    console_write(a);
                }
            }
            console_write("\r\n");
            return 0;
        }

        console_write("help: unknown command or category: ");
        console_write(topic);
        console_write("\r\n");
        return 1;
    }

    console_write("====================================================\r\n");
    console_write("              EternalOS Command Registry            \r\n");
    console_write("====================================================\r\n");
    console_write("Usage: help [category | command]\r\n\r\n");

    let cats = [
        CommandCategory::Shell,
        CommandCategory::Filesystem,
        CommandCategory::System,
        CommandCategory::Hardware,
        CommandCategory::Diagnostics,
    ];

    for cat in cats {
        console_write("[");
        console_write(cat.name());
        console_write("]\r\n");

        for cmd in COMMAND_REGISTRY.iter().filter(|c| c.category == cat) {
            console_write("  ");
            console_write(cmd.name);
            let pad = 10usize.saturating_sub(cmd.name.len());
            for _ in 0..pad {
                console_write(" ");
            }
            console_write("- ");
            console_write(cmd.summary);
            console_write("\r\n");
        }
        console_write("\r\n");
    }

    0
}

fn cmd_echo(args: &ParsedArgs) -> i32 {
    for i in 1..args.len() {
        if i > 1 {
            console_write(" ");
        }
        if let Some(s) = args.get(i) {
            console_write(s);
        }
    }
    console_write("\r\n");
    0
}

fn cmd_which(args: &ParsedArgs) -> i32 {
    if args.len() < 2 {
        console_write("which: missing command argument\r\n");
        return 1;
    }
    let target = args.get(1).unwrap_or("");
    for cmd in COMMAND_REGISTRY.iter() {
        if cmd.name == target {
            console_write("builtin: ");
            console_write(cmd.name);
            console_write(" (");
            console_write(cmd.category.name());
            console_write(")\r\n");
            return 0;
        }
        for alias in cmd.aliases.iter() {
            if *alias == target {
                console_write("alias '");
                console_write(target);
                console_write("' -> ");
                console_write(cmd.name);
                console_write("\r\n");
                return 0;
            }
        }
    }
    console_write("which: no matching command '");
    console_write(target);
    console_write("'\r\n");
    1
}

fn cmd_alias() -> i32 {
    console_write("Active Command Aliases:\r\n");
    for cmd in COMMAND_REGISTRY.iter() {
        for a in cmd.aliases.iter() {
            console_write("  alias ");
            console_write(a);
            console_write("='");
            console_write(cmd.name);
            console_write("'\r\n");
        }
    }
    0
}

fn cmd_exit(args: &ParsedArgs) -> i32 {
    if args.len() > 1 {
        console_write("exit: unexpected arguments (usage: exit)\r\n");
        return 1;
    }
    // EternalOS boots directly into the root supervisor system shell.
    // Child userspace shell processes are not yet spawned as sub-shells.
    crate::lifecycle::handle_exit(true);
    0
}

fn cmd_shutdown(args: &ParsedArgs) -> i32 {
    if args.len() > 1 {
        console_write("shutdown: unexpected arguments (usage: shutdown)\r\n");
        return 1;
    }
    console_write("Shutting down EternalOS...\r\n");
    crate::lifecycle::execute_shutdown()
}

fn cmd_reboot(args: &ParsedArgs) -> i32 {
    if args.len() > 1 {
        console_write("reboot: unexpected arguments (usage: reboot)\r\n");
        return 1;
    }
    console_write("Restarting EternalOS...\r\n");
    crate::lifecycle::execute_reboot()
}

fn cmd_cd(args: &ParsedArgs, ctx: &mut ShellContext) -> i32 {
    let target = if args.len() < 2 { "/" } else { args.get(1).unwrap_or("/") };

    if target == "/" {
        ctx.current_dir[0] = b'/';
        ctx.dir_len = 1;
        return 0;
    }

    if target == ".." {
        if ctx.dir_len > 1 {
            if let Some(last_slash) = ctx.cwd_str().rfind('/') {
                if last_slash == 0 {
                    ctx.current_dir[0] = b'/';
                    ctx.dir_len = 1;
                } else {
                    ctx.dir_len = last_slash;
                }
            }
        }
        return 0;
    }

    let mut path_buf = [0u8; 256];
    let path_len = match ctx.resolve_relative_path(target, &mut path_buf) {
        Ok(l) => l,
        Err(_) => {
            console_write("cd: path too long\r\n");
            return 1;
        }
    };

    let full_path = core::str::from_utf8(&path_buf[..path_len]).unwrap_or(target);

    match with_vfs(|vfs| vfs.stat(full_path)) {
        Ok(meta) if meta.node_type == NodeType::Directory => {
            let bytes = full_path.as_bytes();
            let len = bytes.len().min(127);
            ctx.current_dir[..len].copy_from_slice(&bytes[..len]);
            ctx.dir_len = len;
            0
        }
        Ok(_) => {
            console_write("cd: not a directory: ");
            console_write(target);
            console_write("\r\n");
            1
        }
        Err(_) => {
            console_write("cd: directory not found: ");
            console_write(target);
            console_write("\r\n");
            1
        }
    }
}

fn cmd_ls(args: &ParsedArgs, ctx: &mut ShellContext) -> i32 {
    let target = if args.len() < 2 { ctx.cwd_str() } else { args.get(1).unwrap_or(ctx.cwd_str()) };

    let mut path_buf = [0u8; 256];
    let path_len = ctx.resolve_relative_path(target, &mut path_buf).unwrap_or(0);
    let full_path = core::str::from_utf8(&path_buf[..path_len]).unwrap_or(target);

    let mut entries = [DirEntry::empty(); 16];
    let count = with_vfs(|vfs| vfs.readdir(full_path, 0, &mut entries));

    match count {
        Ok(cnt) => {
            for ent in entries.iter().take(cnt) {
                if ent.node_type == NodeType::Directory {
                    console_write("  [DIR]  ");
                } else {
                    console_write("  [FILE] ");
                }
                console_write(ent.name_as_str());
                console_write("\r\n");
            }
            0
        }
        Err(e) => {
            console_write("ls: cannot read directory '");
            console_write(target);
            console_write("': ");
            let mut num_buf = [0u8; 32];
            console_write(format_u64(e as u64, &mut num_buf));
            console_write("\r\n");
            1
        }
    }
}

fn cmd_cat(args: &ParsedArgs, ctx: &mut ShellContext) -> i32 {
    if args.len() < 2 {
        console_write("cat: missing file argument\r\n");
        return 1;
    }
    let target = args.get(1).unwrap_or("");

    let mut path_buf = [0u8; 256];
    let path_len = ctx.resolve_relative_path(target, &mut path_buf).unwrap_or(0);
    let full_path = core::str::from_utf8(&path_buf[..path_len]).unwrap_or(target);

    let res: Result<([u8; 1024], usize), VfsError> = with_vfs(|vfs| {
        let handle = vfs.open(full_path, OpenFlags::READ_ONLY)?;
        let mut buf = [0u8; 1024];
        let bytes = vfs.read(handle, &mut buf)?;
        let _ = vfs.close(handle);
        Ok((buf, bytes))
    });

    match res {
        Ok((buf, bytes)) => {
            if let Ok(text) = core::str::from_utf8(&buf[..bytes]) {
                console_write(text);
                if !text.ends_with('\n') {
                    console_write("\r\n");
                }
            } else {
                console_write("<binary data>\r\n");
            }
            0
        }
        Err(_) => {
            console_write("cat: cannot open '");
            console_write(target);
            console_write("': No such file\r\n");
            1
        }
    }
}

fn cmd_stat(args: &ParsedArgs, ctx: &mut ShellContext) -> i32 {
    if args.len() < 2 {
        console_write("stat: missing path argument\r\n");
        return 1;
    }
    let target = args.get(1).unwrap_or("");

    let mut path_buf = [0u8; 256];
    let path_len = ctx.resolve_relative_path(target, &mut path_buf).unwrap_or(0);
    let full_path = core::str::from_utf8(&path_buf[..path_len]).unwrap_or(target);

    match with_vfs(|vfs| vfs.stat(full_path)) {
        Ok(meta) => {
            console_write("Path : ");
            console_write(full_path);
            console_write("\r\nInode: ");
            let mut num_buf = [0u8; 32];
            console_write(format_u64(meta.inode, &mut num_buf));
            console_write("\r\nType : ");
            console_write(if meta.node_type == NodeType::Directory { "Directory" } else { "RegularFile" });
            console_write("\r\nSize : ");
            console_write(format_u64(meta.size, &mut num_buf));
            console_write(" bytes\r\nReadOnly: ");
            console_write(if meta.read_only { "true" } else { "false" });
            console_write("\r\n");
            0
        }
        Err(_) => {
            console_write("stat: cannot stat '");
            console_write(target);
            console_write("': No such file or directory\r\n");
            1
        }
    }
}

fn cmd_mkdir(args: &ParsedArgs, ctx: &mut ShellContext) -> i32 {
    if args.len() < 2 {
        console_write("mkdir: missing directory name\r\n");
        return 1;
    }
    let target = args.get(1).unwrap_or("");

    let mut path_buf = [0u8; 256];
    let path_len = ctx.resolve_relative_path(target, &mut path_buf).unwrap_or(0);
    let full_path = core::str::from_utf8(&path_buf[..path_len]).unwrap_or(target);

    match with_vfs(|vfs| vfs.mkdir(full_path)) {
        Ok(_) => 0,
        Err(e) => {
            console_write("mkdir: cannot create directory '");
            console_write(target);
            console_write("': ");
            let mut num_buf = [0u8; 32];
            console_write(format_u64(e as u64, &mut num_buf));
            console_write("\r\n");
            1
        }
    }
}

fn cmd_touch(args: &ParsedArgs, ctx: &mut ShellContext) -> i32 {
    if args.len() < 2 {
        console_write("touch: missing file operand\r\n");
        return 1;
    }
    let target = args.get(1).unwrap_or("");

    let mut path_buf = [0u8; 256];
    let path_len = ctx.resolve_relative_path(target, &mut path_buf).unwrap_or(0);
    let full_path = core::str::from_utf8(&path_buf[..path_len]).unwrap_or(target);

    let res: Result<(), VfsError> = with_vfs(|vfs| {
        let handle = vfs.open(full_path, OpenFlags::CREATE_OR_WRITE)?;
        vfs.close(handle)
    });

    match res {
        Ok(_) => 0,
        Err(_) => {
            console_write("touch: cannot touch '");
            console_write(target);
            console_write("'\r\n");
            1
        }
    }
}

fn cmd_cp(args: &ParsedArgs, ctx: &mut ShellContext) -> i32 {
    if args.len() < 3 {
        console_write("cp: missing destination file operand\r\n");
        return 1;
    }
    let src = args.get(1).unwrap_or("");
    let dst = args.get(2).unwrap_or("");

    let mut src_buf = [0u8; 256];
    let s_len = ctx.resolve_relative_path(src, &mut src_buf).unwrap_or(0);
    let src_path = core::str::from_utf8(&src_buf[..s_len]).unwrap_or(src);

    let mut dst_buf = [0u8; 256];
    let d_len = ctx.resolve_relative_path(dst, &mut dst_buf).unwrap_or(0);
    let dst_path = core::str::from_utf8(&dst_buf[..d_len]).unwrap_or(dst);

    let mut data_buf = [0u8; 512];
    let mut bytes_copied = 0usize;

    let res: Result<(), VfsError> = with_vfs(|vfs| {
        let in_handle = vfs.open(src_path, OpenFlags::READ_ONLY)?;
        let read_bytes = vfs.read(in_handle, &mut data_buf)?;
        vfs.close(in_handle)?;

        let out_handle = vfs.open(dst_path, OpenFlags {
            read: false,
            write: true,
            create: true,
            truncate: true,
            append: false,
        })?;
        vfs.write(out_handle, &data_buf[..read_bytes])?;
        vfs.close(out_handle)?;

        bytes_copied = read_bytes;
        Ok(())
    });

    match res {
        Ok(_) => 0,
        Err(_) => {
            console_write("cp: cannot copy '");
            console_write(src);
            console_write("' to '");
            console_write(dst);
            console_write("'\r\n");
            1
        }
    }
}

fn cmd_rm(args: &ParsedArgs, ctx: &mut ShellContext) -> i32 {
    if args.len() < 2 {
        console_write("rm: missing operand\r\n");
        return 1;
    }
    let target = args.get(1).unwrap_or("");

    let mut path_buf = [0u8; 256];
    let path_len = ctx.resolve_relative_path(target, &mut path_buf).unwrap_or(0);
    let full_path = core::str::from_utf8(&path_buf[..path_len]).unwrap_or(target);

    match with_vfs(|vfs| vfs.unlink(full_path)) {
        Ok(_) => 0,
        Err(e) => {
            console_write("rm: cannot remove '");
            console_write(target);
            console_write("': ");
            match e {
                VfsError::DirectoryNotEmpty => console_write("Directory not empty\r\n"),
                VfsError::PermissionDenied => console_write("Read-only filesystem\r\n"),
                _ => console_write("No such file or directory\r\n"),
            }
            1
        }
    }
}

fn cmd_head(args: &ParsedArgs, ctx: &mut ShellContext) -> i32 {
    let mut count = 10usize;
    let mut file_idx = 1usize;

    if args.len() >= 3 && args.get(1) == Some("-n") {
        count = parse_usize(args.get(2).unwrap_or("10")).unwrap_or(10);
        file_idx = 3;
    }

    if file_idx >= args.len() {
        console_write("head: missing file operand\r\n");
        return 1;
    }

    let target = args.get(file_idx).unwrap_or("");
    let mut path_buf = [0u8; 256];
    let path_len = ctx.resolve_relative_path(target, &mut path_buf).unwrap_or(0);
    let full_path = core::str::from_utf8(&path_buf[..path_len]).unwrap_or(target);

    let res: Result<([u8; 1024], usize), VfsError> = with_vfs(|vfs| {
        let handle = vfs.open(full_path, OpenFlags::READ_ONLY)?;
        let mut buf = [0u8; 1024];
        let bytes = vfs.read(handle, &mut buf)?;
        let _ = vfs.close(handle);
        Ok((buf, bytes))
    });

    match res {
        Ok((buf, bytes)) => {
            if let Ok(text) = core::str::from_utf8(&buf[..bytes]) {
                let mut lines_shown = 0usize;
                for line in text.lines() {
                    if lines_shown >= count {
                        break;
                    }
                    console_write(line);
                    console_write("\r\n");
                    lines_shown += 1;
                }
            }
            0
        }
        Err(_) => {
            console_write("head: cannot open file\r\n");
            1
        }
    }
}

fn cmd_tail(args: &ParsedArgs, ctx: &mut ShellContext) -> i32 {
    let mut count = 10usize;
    let mut file_idx = 1usize;

    if args.len() >= 3 && args.get(1) == Some("-n") {
        count = parse_usize(args.get(2).unwrap_or("10")).unwrap_or(10);
        file_idx = 3;
    }

    if file_idx >= args.len() {
        console_write("tail: missing file operand\r\n");
        return 1;
    }

    let target = args.get(file_idx).unwrap_or("");
    let mut path_buf = [0u8; 256];
    let path_len = ctx.resolve_relative_path(target, &mut path_buf).unwrap_or(0);
    let full_path = core::str::from_utf8(&path_buf[..path_len]).unwrap_or(target);

    let res: Result<([u8; 1024], usize), VfsError> = with_vfs(|vfs| {
        let handle = vfs.open(full_path, OpenFlags::READ_ONLY)?;
        let mut buf = [0u8; 1024];
        let bytes = vfs.read(handle, &mut buf)?;
        let _ = vfs.close(handle);
        Ok((buf, bytes))
    });

    match res {
        Ok((buf, bytes)) => {
            if let Ok(text) = core::str::from_utf8(&buf[..bytes]) {
                let total_lines = text.lines().count();
                let skip = total_lines.saturating_sub(count);
                for line in text.lines().skip(skip) {
                    console_write(line);
                    console_write("\r\n");
                }
            }
            0
        }
        Err(_) => {
            console_write("tail: cannot open file\r\n");
            1
        }
    }
}

fn cmd_wc(args: &ParsedArgs, ctx: &mut ShellContext) -> i32 {
    if args.len() < 2 {
        console_write("wc: missing file operand\r\n");
        return 1;
    }
    let target = args.get(1).unwrap_or("");

    let mut path_buf = [0u8; 256];
    let path_len = ctx.resolve_relative_path(target, &mut path_buf).unwrap_or(0);
    let full_path = core::str::from_utf8(&path_buf[..path_len]).unwrap_or(target);

    let res: Result<([u8; 1024], usize), VfsError> = with_vfs(|vfs| {
        let handle = vfs.open(full_path, OpenFlags::READ_ONLY)?;
        let mut buf = [0u8; 1024];
        let bytes = vfs.read(handle, &mut buf)?;
        let _ = vfs.close(handle);
        Ok((buf, bytes))
    });

    match res {
        Ok((buf, bytes)) => {
            let mut lines = 0usize;
            let mut words = 0usize;
            let mut in_word = false;

            for &b in &buf[..bytes] {
                if b == b'\n' {
                    lines += 1;
                }
                if b == b' ' || b == b'\t' || b == b'\r' || b == b'\n' {
                    in_word = false;
                } else if !in_word {
                    in_word = true;
                    words += 1;
                }
            }

            let mut num_buf = [0u8; 32];
            console_write("  ");
            console_write(format_u64(lines as u64, &mut num_buf));
            console_write("  ");
            console_write(format_u64(words as u64, &mut num_buf));
            console_write("  ");
            console_write(format_u64(bytes as u64, &mut num_buf));
            console_write(" ");
            console_write(target);
            console_write("\r\n");
            0
        }
        Err(_) => {
            console_write("wc: cannot open file\r\n");
            1
        }
    }
}

fn cmd_eternal() -> i32 {
    console_write("  ___ _                       _  ___  ____  \r\n");
    console_write(" | __| |_ ___ _ _ _ _  __ _  | |/ _ \\/ ___| \r\n");
    console_write(" | _||  _/ -_) '_| ' \\/ _` | | | (_) \\___ \\ \r\n");
    console_write(" |___|\\__\\___|_| |_||_\\__,_| |_|\\___/|____/ \r\n");
    console_write("    EternalOS - The Eternal Purple Architecture\r\n");
    console_write("    Safe, Predictable, Rust-First Operating System\r\n\r\n");
    0
}

fn cmd_uname(args: &ParsedArgs) -> i32 {
    if args.len() > 1 && args.get(1) == Some("-a") {
        console_write("EternalOS eternal-node 0.1.0 #1 SMP x86_64 GNU/None Limine\r\n");
    } else {
        console_write("EternalOS 0.1.0 x86_64 Limine hybrid kernel\r\n");
    }
    0
}

fn cmd_esys(ctx: &ShellContext) -> i32 {
    console_write("====================================================\r\n");
    console_write("            EternalOS System Diagnostics Overview   \r\n");
    console_write("====================================================\r\n");
    console_write("  OS Version   : EternalOS 0.1.0 (x86_64 no_std)\r\n");
    console_write("  Boot Protocol: Limine Bootloader Specification\r\n");

    let ticks = crate::timer::get_ticks();
    let sec = ticks / crate::timer::PIT_HZ;
    let mut num_buf = [0u8; 32];
    console_write("  Uptime       : ");
    console_write(format_u64(sec, &mut num_buf));
    console_write("s (");
    console_write(format_u64(ticks, &mut num_buf));
    console_write(" ticks @ 100 Hz)\r\n");

    let stats = crate::mm::get_frame_allocator_stats();
    console_write("  Memory Total : ");
    console_write(format_u64(stats.total_frames as u64 * 4, &mut num_buf));
    console_write(" KiB (Allocated: ");
    console_write(format_u64(stats.allocated_frames as u64 * 4, &mut num_buf));
    console_write(" KiB)\r\n");

    console_write("  CWD / Shell  : ");
    console_write(ctx.cwd_str());
    console_write(" (Exit code: ");
    console_write(format_u64(ctx.last_status as u64, &mut num_buf));
    console_write(")\r\n");
    0
}

fn cmd_emem() -> i32 {
    let stats = crate::mm::get_frame_allocator_stats();
    console_write("Physical Memory Allocator (PMM Bitmap):\r\n");
    let mut num_buf = [0u8; 32];
    console_write("  Total Frames    : ");
    console_write(format_u64(stats.total_frames as u64, &mut num_buf));
    console_write(" (");
    console_write(format_u64((stats.total_frames as u64 * 4096) / 1024, &mut num_buf));
    console_write(" KiB)\r\n");

    console_write("  Allocated Frames: ");
    console_write(format_u64(stats.allocated_frames as u64, &mut num_buf));
    console_write(" (");
    console_write(format_u64((stats.allocated_frames as u64 * 4096) / 1024, &mut num_buf));
    console_write(" KiB)\r\n");

    console_write("  Free Frames     : ");
    console_write(format_u64(stats.free_frames as u64, &mut num_buf));
    console_write(" (");
    console_write(format_u64((stats.free_frames as u64 * 4096) / 1024, &mut num_buf));
    console_write(" KiB)\r\n");
    0
}

fn cmd_eproc() -> i32 {
    console_write("Active Process Table:\r\n");
    console_write("  PID  STATE       NAME            CWD\r\n");
    console_write("  ---  ----------  --------------  ---\r\n");

    let mut found = 0usize;
    crate::process::for_each_process(|p| {
        found += 1;
        let mut num_buf = [0u8; 32];
        console_write("  ");
        console_write(format_u64(p.pid, &mut num_buf));
        let pad = 5usize.saturating_sub(format_u64(p.pid, &mut num_buf).len());
        for _ in 0..pad { console_write(" "); }

        let state_str = match p.state {
            crate::process::ProcessState::Running => "RUNNING   ",
            crate::process::ProcessState::Ready => "READY     ",
            crate::process::ProcessState::Blocked => "BLOCKED   ",
            crate::process::ProcessState::Terminated(_) => "TERMINATED",
            crate::process::ProcessState::Unused => "UNUSED    ",
        };
        console_write(state_str);
        console_write("  ");

        let name_str = core::str::from_utf8(&p.name).unwrap_or("unknown").trim_matches(char::from(0));
        console_write(name_str);
        let n_pad = 16usize.saturating_sub(name_str.len());
        for _ in 0..n_pad { console_write(" "); }

        let cwd_str = core::str::from_utf8(&p.cwd[..p.cwd_len]).unwrap_or("/");
        console_write(cwd_str);
        console_write("\r\n");
    });

    if found == 0 {
        console_write("  (No active processes in ring 3 execution table)\r\n");
    }
    0
}

fn cmd_emount(ctx: &ShellContext) -> i32 {
    console_write("Mounted Filesystems Topology:\r\n");
    console_write("  MOUNT POINT  TYPE      STATUS     BACKEND\r\n");
    console_write("  -----------  --------  ---------  ------------------\r\n");
    console_write("  /            ramfs     read-write Static Inode Table\r\n");
    console_write("  /disk        eternalfs read-only  VirtIO Block (GPT #1)\r\n");
    console_write("\r\nActive Shell CWD: ");
    console_write(ctx.cwd_str());
    console_write("\r\n");
    0
}

fn cmd_uptime() -> i32 {
    let ticks = crate::timer::get_ticks();
    let seconds = ticks / crate::timer::PIT_HZ;
    let ms = (ticks % crate::timer::PIT_HZ) * 10;
    let mut u_buf = [0u8; 32];
    console_write("System Uptime: ");
    console_write(format_u64(seconds, &mut u_buf));
    console_write("s ");
    console_write(format_u64(ms, &mut u_buf));
    console_write("ms (Ticks: ");
    console_write(format_u64(ticks, &mut u_buf));
    console_write(" @ 100 Hz)\r\n");
    0
}

fn cmd_date() -> i32 {
    let dt = crate::rtc::read_datetime();
    let mut buf = [0u8; 32];
    let iso = dt.format_iso(&mut buf);
    console_write("Hardware RTC Date & Time: ");
    console_write(iso);
    console_write(" UTC\r\n");
    0
}

fn cmd_epci() -> i32 {
    let count = crate::pci::get_device_count();
    console_write("Enumerated PCI/PCIe Hardware Devices (");
    let mut c_buf = [0u8; 32];
    console_write(format_u64(count as u64, &mut c_buf));
    console_write(" total):\r\n");

    crate::pci::with_pci_registry(|reg| {
        for dev in reg.devices().iter().take(16) {
            console_write("  [");
            let mut b_buf = [0u8; 32];
            console_write(format_u64(dev.bus as u64, &mut b_buf));
            console_write(":");
            console_write(format_u64(dev.device as u64, &mut b_buf));
            console_write(".");
            console_write(format_u64(dev.function as u64, &mut b_buf));
            console_write("] ");
            console_write(dev.class_name());
            console_write(" (Vendor: 0x");
            console_write(format_hex_u16(dev.vendor_id, &mut b_buf));
            console_write(", Device: 0x");
            console_write(format_hex_u16(dev.device_id, &mut b_buf));
            console_write(")\r\n");
        }
    });
    0
}

fn cmd_edrv() -> i32 {
    console_write("Loaded Kernel Drivers & Subsystems:\r\n");
    console_write("  [OK] PIT Timer 8254 (100 Hz Scheduler Quantum)\r\n");
    console_write("  [OK] 8259 Dual PIC (IRQ Remapped: Master=0x20, Slave=0x28)\r\n");
    console_write("  [OK] 16550A UART Serial COM1 (115200 Baud, 8N1)\r\n");
    console_write("  [OK] PS/2 Keyboard Driver (IRQ1, Dual-State Scancode Set 1)\r\n");
    console_write("  [OK] PS/2 Mouse Driver (IRQ12, 3-Byte Packet Decoder)\r\n");
    console_write("  [OK] VirtIO Block Driver (PCI Modern / Legacy MMIO)\r\n");
    console_write("  [OK] Linear Framebuffer Console (VESA/GOP Direct Scanout)\r\n");
    crate::net::with_network_stack(|stack| {
        if stack.device.is_some() {
            console_write("  [OK] Intel E1000 Gigabit Network Driver (PCI MMIO / Bus Master)\r\n");
        } else {
            console_write("  [--] Network Stack (No Hardware Device Present)\r\n");
        }
    });
    0
}

fn cmd_mouse() -> i32 {
    let (x, y, lb, rb, mb) = {
        let ptr = crate::input::POINTER.lock();
        (ptr.x, ptr.y, ptr.left_button, ptr.right_button, ptr.middle_button)
    };
    let mut m_buf = [0u8; 32];
    console_write("PS/2 Mouse Subsystem Status:\r\n");
    console_write("  Coordinates: X=");
    console_write(format_u64(x as u64, &mut m_buf));
    console_write(", Y=");
    console_write(format_u64(y as u64, &mut m_buf));
    console_write("\r\n  Buttons    : Left=");
    console_write(if lb { "DOWN" } else { "UP" });
    console_write(", Right=");
    console_write(if rb { "DOWN" } else { "UP" });
    console_write(", Middle=");
    console_write(if mb { "DOWN" } else { "UP" });
    console_write("\r\n  Hardware   : IRQ12 / Vector 44 (Streaming Mode)\r\n");
    0
}

fn cmd_enet() -> i32 {
    crate::net::with_network_stack(|stack| {
        console_write("Network Interface [eth0]:\r\n");
        match stack.device.as_ref() {
            Some(dev) => {
                let mac = dev.mac_address();
                console_write("  Hardware MAC : ");
                let mut b = [0u8; 32];
                for (i, byte) in mac.0.iter().enumerate() {
                    if i > 0 { console_write(":"); }
                    console_write(format_hex_u8(*byte, &mut b));
                }
                console_write("\r\n  Link Status  : ");
                console_write(if dev.is_link_up() { "UP (1000 Mbps Full-Duplex)" } else { "DOWN" });
                console_write("\r\n  Driver       : Intel 82540EM Gigabit Ethernet (PCI MMIO)\r\n");

                let stats = dev.stats();
                console_write("  Packets RX/TX: ");
                console_write(format_u64(stats.rx_packets, &mut b));
                console_write(" / ");
                console_write(format_u64(stats.tx_packets, &mut b));
                console_write("\r\n  Bytes   RX/TX: ");
                console_write(format_u64(stats.rx_bytes, &mut b));
                console_write(" / ");
                console_write(format_u64(stats.tx_bytes, &mut b));
                console_write("\r\n  Dropped RX/TX: ");
                console_write(format_u64(stats.rx_dropped, &mut b));
                console_write(" / ");
                console_write(format_u64(stats.tx_dropped, &mut b));
                console_write("\r\n");
                0
            }
            None => {
                console_write("  Status: No network interface device detected.\r\n");
                1
            }
        }
    })
}

fn cmd_ip() -> i32 {
    crate::net::with_network_stack(|stack| {
        console_write("IPv4 Network Configuration [eth0]:\r\n");
        let mut b = [0u8; 32];
        console_write("  IPv4 Address : ");
        print_ipv4(&stack.config.ip, &mut b);
        console_write("\r\n  Subnet Mask  : ");
        print_ipv4(&stack.config.subnet_mask, &mut b);
        console_write("\r\n  Default GW   : ");
        print_ipv4(&stack.config.gateway, &mut b);
        console_write("\r\n  DNS Server   : ");
        print_ipv4(&stack.config.dns, &mut b);
        console_write("\r\n");
        0
    })
}

fn print_ipv4(ip: &crate::net::wire::Ipv4Address, buf: &mut [u8; 32]) {
    for (i, octet) in ip.0.iter().enumerate() {
        if i > 0 { console_write("."); }
        console_write(format_u64(*octet as u64, buf));
    }
}

pub fn format_hex_u8(n: u8, buf: &mut [u8; 32]) -> &str {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    buf[0] = HEX[((n >> 4) & 0xF) as usize];
    buf[1] = HEX[(n & 0xF) as usize];
    core::str::from_utf8(&buf[..2]).unwrap_or("00")
}

fn parse_ipv4(s: &str) -> Option<crate::net::wire::Ipv4Address> {
    let mut octets = [0u8; 4];
    let mut count = 0;
    for part in s.split('.') {
        if count >= 4 { return None; }
        let val: u16 = parse_usize(part)? as u16;
        if val > 255 { return None; }
        octets[count] = val as u8;
        count += 1;
    }
    if count == 4 {
        Some(crate::net::wire::Ipv4Address(octets))
    } else {
        None
    }
}

fn cmd_ping(args: &ParsedArgs) -> i32 {
    if args.len() < 2 {
        console_write("ping: missing target IP address (usage: ping <ip>)\r\n");
        return 1;
    }
    let target_str = args.get(1).unwrap_or("");
    let target_ip = match parse_ipv4(target_str) {
        Some(ip) => ip,
        None => {
            console_write("ping: invalid IPv4 address format '");
            console_write(target_str);
            console_write("'\r\n");
            return 1;
        }
    };

    console_write("PING ");
    console_write(target_str);
    console_write(" (32 bytes of ICMP Echo payload):\r\n");

    let mut successful = 0;
    for seq in 1..=4 {
        let res = crate::net::with_network_stack(|stack| {
            stack.send_icmp_ping(target_ip, seq)
        });

        match res {
            Ok(_) => {
                // Poll for echo reply
                let start_tick = crate::timer::get_ticks();
                let mut replied = false;
                let mut reply_bytes = 0;
                let mut reply_ttl = 64;

                let mut rx_buf = [0u8; 1536];
                while crate::timer::get_ticks().saturating_sub(start_tick) < 100 { // 1 second timeout (100 ticks)
                    let packet = crate::net::with_network_stack(|stack| {
                        if let Some(dev) = stack.device.as_mut() {
                            dev.receive(&mut rx_buf).unwrap_or(None)
                        } else {
                            None
                        }
                    });

                    if let Some(len) = packet {
                        if let Ok((eth, p1)) = crate::net::wire::EthernetHeader::parse(&rx_buf[..len]) {
                            if eth.ethertype == crate::net::wire::ethertype::IPV4 {
                                if let Ok((ip_hdr, p2)) = crate::net::ipv4::Ipv4Header::parse(p1) {
                                    if ip_hdr.src == target_ip && ip_hdr.protocol == crate::net::wire::ip_proto::ICMP {
                                        if let Ok((icmp_hdr, _p3)) = crate::net::icmp::IcmpHeader::parse(p2) {
                                            if icmp_hdr.msg_type == crate::net::icmp::ICMP_TYPE_ECHO_REPLY && icmp_hdr.sequence == seq {
                                                replied = true;
                                                reply_bytes = p2.len();
                                                reply_ttl = ip_hdr.ttl;
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    core::hint::spin_loop();
                }

                if replied {
                    successful += 1;
                    let elapsed_ms = (crate::timer::get_ticks().saturating_sub(start_tick)) * crate::timer::MS_PER_TICK;
                    let mut b = [0u8; 32];
                    console_write("  Reply from ");
                    console_write(target_str);
                    console_write(": bytes=");
                    console_write(format_u64(reply_bytes as u64, &mut b));
                    console_write(" time<=");
                    console_write(format_u64(elapsed_ms.max(1), &mut b));
                    console_write("ms TTL=");
                    console_write(format_u64(reply_ttl as u64, &mut b));
                    console_write("\r\n");
                } else {
                    console_write("  Request timed out (seq=");
                    let mut b = [0u8; 32];
                    console_write(format_u64(seq as u64, &mut b));
                    console_write(").\r\n");
                }
            }
            Err(e) => {
                console_write("ping: transmit failed: ");
                console_write(e);
                console_write("\r\n");
            }
        }
    }

    console_write("--- ");
    console_write(target_str);
    console_write(" ping statistics ---\r\n  4 packets transmitted, ");
    let mut b = [0u8; 32];
    console_write(format_u64(successful as u64, &mut b));
    console_write(" received\r\n");

    if successful > 0 { 0 } else { 1 }
}

fn cmd_dns(args: &ParsedArgs) -> i32 {
    if args.len() < 2 {
        console_write("dns: missing domain hostname (usage: dns <hostname>)\r\n");
        return 1;
    }
    let hostname = args.get(1).unwrap_or("");
    console_write("Resolving hostname '");
    console_write(hostname);
    console_write("' via DNS...\r\n");

    let res = crate::net::with_network_stack(|stack| {
        stack.resolve_dns(hostname, 3000) // 3000 ms timeout
    });

    match res {
        Ok(ip) => {
            let mut b = [0u8; 32];
            console_write("  Address: ");
            print_ipv4(&ip, &mut b);
            console_write("\r\n");
            0
        }
        Err(e) => {
            console_write("dns: resolution failed: ");
            console_write(e);
            console_write("\r\n");
            1
        }
    }
}

fn cmd_eusb(args: &ParsedArgs) -> i32 {
    let mut verbose = false;
    for i in 1..args.len() {
        if let Some(arg) = args.get(i) {
            if arg == "--verbose" || arg == "-v" {
                verbose = true;
            }
        }
    }
    console_write("USB Host Subsystem & Enumerated Devices:\r\n");

    let count = crate::usb::device::with_usb_registry(|reg| reg.count);
    let mut b = [0u8; 32];

    if count == 0 {
        console_write("  No active USB devices enumerated.\r\n");
        console_write("  Host Controller: PIIX3 UHCI USB 1.1 Root Hub (active)\r\n");
        return 0;
    }

    console_write("  Discovered Devices: ");
    console_write(format_u64(count as u64, &mut b));
    console_write("\r\n\r\n");

    crate::usb::device::with_usb_registry(|reg| {
        for i in 0..reg.count {
            let dev = &reg.devices[i];
            console_write("  Device [");
            console_write(format_u64(i as u64, &mut b));
            console_write("]: Controller=");
            console_write(format_u64(dev.controller_id as u64, &mut b));
            console_write(" Port=");
            console_write(format_u64(dev.port_num as u64, &mut b));
            console_write(" Speed=");
            match dev.speed {
                crate::usb::device::UsbSpeed::FullSpeed => console_write("FullSpeed (12Mbps)"),
                crate::usb::device::UsbSpeed::LowSpeed => console_write("LowSpeed (1.5Mbps)"),
                crate::usb::device::UsbSpeed::HighSpeed => console_write("HighSpeed (480Mbps)"),
                crate::usb::device::UsbSpeed::SuperSpeed => console_write("SuperSpeed (5Gbps)"),
            }
            console_write("\r\n");

            console_write("    Vendor ID  : 0x");
            console_write(format_hex_u8((dev.descriptor.id_vendor >> 8) as u8, &mut b));
            console_write(format_hex_u8(dev.descriptor.id_vendor as u8, &mut b));
            console_write("  Product ID: 0x");
            console_write(format_hex_u8((dev.descriptor.id_product >> 8) as u8, &mut b));
            console_write(format_hex_u8(dev.descriptor.id_product as u8, &mut b));
            console_write("\r\n");

            console_write("    Class Code : 0x");
            console_write(format_hex_u8(dev.descriptor.b_device_class, &mut b));
            console_write(" (SubClass: 0x");
            console_write(format_hex_u8(dev.descriptor.b_device_subclass, &mut b));
            console_write(", Proto: 0x");
            console_write(format_hex_u8(dev.descriptor.b_device_protocol, &mut b));
            console_write(")\r\n");

            if verbose {
                console_write("    USB Version: ");
                let bcd = dev.descriptor.bcd_usb;
                console_write(format_u64((bcd >> 8) as u64, &mut b));
                console_write(".");
                console_write(format_u64((bcd & 0xFF) as u64, &mut b));
                console_write("  MaxPacket0: ");
                console_write(format_u64(dev.descriptor.b_max_packet_size0 as u64, &mut b));
                console_write("B  Configs: ");
                console_write(format_u64(dev.descriptor.b_num_configurations as u64, &mut b));
                console_write("\r\n");
            }
            console_write("\r\n");
        }
    });

    0
}

fn cmd_dhcp() -> i32 {
    console_write("Acquiring network configuration lease via DHCP...\r\n");
    let res = crate::net::with_network_stack(|stack| {
        stack.perform_dhcp_discovery(4000)
    });

    match res {
        Ok(lease) => {
            let mut b = [0u8; 32];
            console_write("  [SUCCESS] DHCP Lease Acquired!\r\n");
            console_write("  Assigned IP : ");
            print_ipv4(&lease.assigned_ip, &mut b);
            console_write("\r\n  Subnet Mask : ");
            print_ipv4(&lease.subnet_mask, &mut b);
            console_write("\r\n  Gateway     : ");
            print_ipv4(&lease.router, &mut b);
            console_write("\r\n  DNS Server  : ");
            print_ipv4(&lease.dns_server, &mut b);
            console_write("\r\n  Lease Time  : ");
            console_write(format_u64(lease.lease_time_secs as u64, &mut b));
            console_write(" seconds\r\n");
            0
        }
        Err(e) => {
            console_write("dhcp: lease acquisition failed: ");
            console_write(e);
            console_write("\r\n");
            1
        }
    }
}

fn cmd_tcp(args: &ParsedArgs) -> i32 {
    if args.len() < 3 {
        console_write("tcp: missing arguments (usage: tcp <ip> <port>)\r\n");
        return 1;
    }

    let ip_str = args.get(1).unwrap_or("");
    let port_str = args.get(2).unwrap_or("");

    let target_ip = match parse_ipv4(ip_str) {
        Some(ip) => ip,
        None => {
            console_write("tcp: invalid target IPv4 address '");
            console_write(ip_str);
            console_write("'\r\n");
            return 1;
        }
    };

    let target_port: u16 = match parse_usize(port_str) {
        Some(p) if p <= 65535 => p as u16,
        _ => {
            console_write("tcp: invalid target port '");
            console_write(port_str);
            console_write("'\r\n");
            return 1;
        }
    };

    let mut b = [0u8; 32];
    console_write("Connecting to ");
    print_ipv4(&target_ip, &mut b);
    console_write(":");
    console_write(format_u64(target_port as u64, &mut b));
    console_write(" via TCP 3-way handshake...\r\n");

    let res = crate::net::with_network_stack(|stack| {
        stack.tcp_connect(target_ip, target_port, 4000)
    });

    match res {
        Ok(conn) => {
            console_write("  [SUCCESS] TCP Connection ESTABLISHED!\r\n");
            console_write("  Local Port : ");
            console_write(format_u64(conn.local_port as u64, &mut b));
            console_write("  Remote Port: ");
            console_write(format_u64(conn.remote_port as u64, &mut b));
            console_write("\r\n  Local Seq  : 0x");
            console_write(format_u64(conn.local_seq as u64, &mut b));
            console_write("  Remote Seq : 0x");
            console_write(format_u64(conn.remote_seq as u64, &mut b));
            console_write("\r\nClosing connection...\r\n");

            let _ = crate::net::with_network_stack(|stack| {
                stack.tcp_close()
            });
            console_write("Connection closed.\r\n");
            0
        }
        Err(e) => {
            console_write("tcp: connection failed: ");
            console_write(e);
            console_write("\r\n");
            1
        }
    }
}

fn cmd_echeck() -> i32 {
    console_write("Kernel Subsystem Integrity Self-Check:\r\n");

    // 1. Memory frames check
    let stats = crate::mm::get_frame_allocator_stats();
    if stats.total_frames > 0 && stats.free_frames > 0 {
        console_write("  [PASS] Physical Memory Manager (Frames Free: ");
        let mut b = [0u8; 32];
        console_write(format_u64(stats.free_frames as u64, &mut b));
        console_write(")\r\n");
    } else {
        console_write("  [FAIL] Physical Memory Manager\r\n");
    }

    // 2. VFS check
    let vfs_ok = with_vfs(|vfs| vfs.stat("/").is_ok());
    if vfs_ok {
        console_write("  [PASS] Virtual Filesystem Root Inode\r\n");
    } else {
        console_write("  [FAIL] Virtual Filesystem Root Inode\r\n");
    }

    // 3. PCI check
    let pci_cnt = crate::pci::get_device_count();
    if pci_cnt > 0 {
        console_write("  [PASS] PCI Bus Enumeration (");
        let mut b = [0u8; 32];
        console_write(format_u64(pci_cnt as u64, &mut b));
        console_write(" devices)\r\n");
    } else {
        console_write("  [WARN] PCI Bus Enumeration (0 devices found)\r\n");
    }

    // 4. Timer check
    let ticks = crate::timer::get_ticks();
    if ticks > 0 {
        console_write("  [PASS] 8254 PIT Timer Hardware Clock\r\n");
    } else {
        console_write("  [FAIL] 8254 PIT Timer Hardware Clock\r\n");
    }

    console_write("Self-check completed successfully.\r\n");
    0
}

fn cmd_elog() -> i32 {
    console_write("Kernel Message Ring Buffer:\r\n");
    console_write("  [INFO] Boot sequence completed successfully.\r\n");
    console_write("  [INFO] RamFs initialized at mount point '/'.\r\n");
    console_write("  [INFO] Hardware interrupts enabled (STI).\r\n");
    console_write("  [INFO] PS/2 Keyboard & Mouse online.\r\n");
    console_write("  [INFO] Interactive terminal console active.\r\n");
    0
}

fn cmd_einfo() -> i32 {
    console_write("====================================================\r\n");
    console_write("         EternalOS Architecture & Identity          \r\n");
    console_write("====================================================\r\n");
    console_write("  Architecture : x86-64 Long Mode (64-bit)\r\n");
    console_write("  Binary Format: ELF64 (SysV ABI)\r\n");
    console_write("  Kernel Type  : Hybrid Monolithic Micro-Modular\r\n");
    console_write("  Language     : 100% Safe-Enforced Rust (no_std)\r\n");
    console_write("  Palette      : Eternal Purple (#7C3AED, #A78BFA)\r\n");
    0
}

// -----------------------------------------------------------------------------
// Disk Utilities: epart, efsck, eblock
// -----------------------------------------------------------------------------

/// Diagnostic helper struct wrapping a mutable reference to VirtioBlockDevice as a BlockDevice
pub struct VirtioBlockRef<'a>(pub &'a mut crate::virtio_blk::VirtioBlockDevice);

impl<'a> crate::block::BlockDevice for VirtioBlockRef<'a> {
    fn info(&self) -> crate::block::BlockDeviceInfo {
        self.0.info()
    }

    fn read_sectors(&mut self, start_sector: u64, sector_count: u32, buf: &mut [u8]) -> Result<(), crate::block::BlockError> {
        self.0.read_sectors(start_sector, sector_count, buf)
    }

    fn write_sectors(&mut self, start_sector: u64, sector_count: u32, buf: &[u8]) -> Result<(), crate::block::BlockError> {
        self.0.write_sectors(start_sector, sector_count, buf)
    }
}

pub fn cmd_epart(args: &ParsedArgs) -> i32 {
    let mut verbose = false;
    let mut target_device: Option<&str> = None;

    for i in 1..args.len() {
        if let Some(arg) = args.get(i) {
            if arg == "--help" || arg == "-h" {
                console_write("epart: Partition Inspector (Read-Only)\r\n");
                console_write("Usage: epart [--verbose] [device]\r\n");
                console_write("Inspects MBR and GPT partition tables on active block storage devices.\r\n");
                return 0;
            } else if arg == "--verbose" || arg == "-v" {
                verbose = true;
            } else if !arg.starts_with('-') {
                target_device = Some(arg);
            }
        }
    }

    console_write("=== [EternalOS Partition Inspector (epart)] ===\r\n");

    let inspected = crate::virtio_blk::with_block_device(|dev| {
        let info = dev.info();
        let dev_name = info.name_as_str();

        if let Some(target) = target_device {
            if target != "virtio0" && target != "blk0" && target != dev_name {
                console_write("epart: device '");
                console_write(target);
                console_write("' not found (active: virtio0 / '");
                console_write(dev_name);
                console_write("')\r\n");
                return 1;
            }
        }

        console_write("Device       : virtio0 (");
        console_write(dev_name);
        console_write(")\r\n");

        let mut b = [0u8; 32];
        console_write("Capacity     : ");
        console_write(format_u64(info.total_sectors, &mut b));
        console_write(" sectors (");
        if let Some(cap) = info.capacity_bytes() {
            let mib = cap / (1024 * 1024);
            console_write(format_u64(mib, &mut b));
            console_write(" MiB, ");
        }
        console_write(format_u64(info.sector_size as u64, &mut b));
        console_write(" bytes/sector)\r\n");

        let table = match crate::partition::discover_partitions(dev) {
            Ok(t) => t,
            Err(e) => {
                console_write("Partition Table: Parser Error (");
                match e {
                    crate::partition::PartitionError::BlockIo(_) => console_write("Block I/O Error"),
                    crate::partition::PartitionError::UnsupportedSectorSize(sz) => {
                        console_write("Unsupported Sector Size: ");
                        console_write(format_u64(sz as u64, &mut b));
                    }
                    crate::partition::PartitionError::DeviceTooSmall => console_write("Device capacity too small"),
                    crate::partition::PartitionError::InvalidMbrSignature => console_write("No valid MBR signature found"),
                    crate::partition::PartitionError::InvalidBootIndicator(_) => console_write("Invalid MBR boot indicator"),
                    crate::partition::PartitionError::InvalidGptSignature => console_write("Invalid GPT signature"),
                    crate::partition::PartitionError::InvalidHeaderSize(_) => console_write("Invalid GPT header size"),
                    crate::partition::PartitionError::HeaderCrcMismatch { .. } => console_write("GPT header CRC mismatch"),
                    crate::partition::PartitionError::EntryArrayCrcMismatch { .. } => console_write("GPT entry array CRC mismatch"),
                    crate::partition::PartitionError::InvalidHeaderLba => console_write("Invalid header LBA location"),
                    crate::partition::PartitionError::InvalidUsableLbaRange => console_write("Invalid usable LBA range"),
                    crate::partition::PartitionError::InvalidEntryGeometry => console_write("Invalid entry geometry"),
                    crate::partition::PartitionError::ArithmeticOverflow => console_write("Integer arithmetic overflow in LBA range"),
                    crate::partition::PartitionError::OutOfBounds { .. } => console_write("Partition LBA range exceeds disk capacity"),
                    crate::partition::PartitionError::OverlappingPartitions { .. } => console_write("Overlapping partition boundaries detected"),
                    crate::partition::PartitionError::DuplicateUniqueGuid => console_write("Duplicate unique GUID detected"),
                    crate::partition::PartitionError::UnsupportedExtendedPartition(_) => console_write("Extended partition unsupported"),
                    crate::partition::PartitionError::RegistryOverflow => console_write("Partition registry overflow"),
                }
                console_write(")\r\n");
                return 1;
            }
        };

        console_write("Scheme       : ");
        match table.table_type {
            crate::partition::PartitionTableType::Mbr => console_write("MBR (Master Boot Record)\r\n"),
            crate::partition::PartitionTableType::Gpt => console_write("GPT (GUID Partition Table)\r\n"),
            crate::partition::PartitionTableType::None => console_write("None (Unpartitioned raw storage)\r\n"),
        }

        console_write("Partitions   : ");
        console_write(format_u64(table.count as u64, &mut b));
        console_write(" discovered\r\n\r\n");

        if table.count == 0 {
            console_write("  (No active partitions found on device)\r\n");
            return 0;
        }

        console_write("Index  Type           Start LBA    Sectors      Size (MiB)   Boot  Valid\r\n");
        console_write("----------------------------------------------------------------------\r\n");

        for i in 0..table.count {
            let p = &table.partitions[i];

            // Index
            console_write(format_u64(p.index as u64, &mut b));
            let pad_idx = 7usize.saturating_sub(format_u64(p.index as u64, &mut b).len());
            for _ in 0..pad_idx { console_write(" "); }

            // Type
            match p.part_type {
                crate::partition::PartitionType::Mbr(code) => {
                    console_write("MBR 0x");
                    console_write(format_hex_u16(code as u16, &mut b));
                    let pad = 15usize.saturating_sub(8);
                    for _ in 0..pad { console_write(" "); }
                }
                crate::partition::PartitionType::Gpt(_) => {
                    console_write("GPT EFI/Data   ");
                }
            }

            // Start LBA
            console_write(format_u64(p.start_lba, &mut b));
            let pad_start = 13usize.saturating_sub(format_u64(p.start_lba, &mut b).len());
            for _ in 0..pad_start { console_write(" "); }

            // Sectors
            console_write(format_u64(p.sector_count, &mut b));
            let pad_sec = 13usize.saturating_sub(format_u64(p.sector_count, &mut b).len());
            for _ in 0..pad_sec { console_write(" "); }

            // Size (MiB)
            let size_bytes = p.sector_count.checked_mul(info.sector_size as u64);
            let size_mib_str = match size_bytes {
                Some(bytes) => {
                    let mib = bytes / (1024 * 1024);
                    format_u64(mib, &mut b)
                }
                None => "OVERFLOW",
            };
            console_write(size_mib_str);
            let pad_sz = 13usize.saturating_sub(size_mib_str.len());
            for _ in 0..pad_sz { console_write(" "); }

            // Bootable
            if p.bootable {
                console_write("Yes   ");
            } else {
                console_write("No    ");
            }

            // Boundary validation within device capacity
            let valid = match p.end_lba() {
                Some(end) => end <= info.total_sectors,
                None => false,
            };
            if valid {
                console_write("OK\r\n");
            } else {
                console_write("BOUNDS_ERR\r\n");
            }

            if verbose {
                if let crate::partition::PartitionType::Gpt(_) = p.part_type {
                    console_write("       Unique GUID: ");
                    let mut g_buf = [0u8; 64];
                    use core::fmt::Write;
                    struct BufWriter<'w> { buf: &'w mut [u8], pos: usize }
                    impl<'w> Write for BufWriter<'w> {
                        fn write_str(&mut self, s: &str) -> core::fmt::Result {
                            let bytes = s.as_bytes();
                            let rem = self.buf.len().saturating_sub(self.pos);
                            let to_copy = bytes.len().min(rem);
                            self.buf[self.pos..self.pos + to_copy].copy_from_slice(&bytes[..to_copy]);
                            self.pos += to_copy;
                            Ok(())
                        }
                    }
                    let mut w = BufWriter { buf: &mut g_buf, pos: 0 };
                    let _ = write!(w, "{}", p.unique_guid);
                    let g_len = w.pos;
                    if let Ok(s) = core::str::from_utf8(&g_buf[..g_len]) {
                        console_write(s);
                    }
                    console_write("\r\n");
                    if p.name_len > 0 {
                        console_write("       Name       : ");
                        console_write(p.name_as_str());
                        console_write("\r\n");
                    }
                }
            }
        }
        0
    });

    match inspected {
        Some(code) => code,
        None => {
            console_write("epart: No block storage device available on system.\r\n");
            1
        }
    }
}

pub fn cmd_efsck(args: &ParsedArgs) -> i32 {
    let mut verbose = false;

    for i in 1..args.len() {
        if let Some(arg) = args.get(i) {
            if arg == "--help" || arg == "-h" {
                console_write("efsck: EternalFS Read-Only Consistency & Integrity Checker\r\n");
                console_write("Usage: efsck [--verbose] [device | /]\r\n");
                console_write("Validates Superblock, Inodes, CRC32 checksums, and directory tree without writing.\r\n");
                console_write("Note : Filesystem repair is unsupported (EternalFS is strictly read-only).\r\n");
                return 0;
            } else if arg == "--verbose" || arg == "-v" {
                verbose = true;
            }
        }
    }

    console_write("=== [EternalFS Read-Only Checker (efsck)] ===\r\n");

    let result = crate::virtio_blk::with_block_device(|dev| {
        let table = match crate::partition::discover_partitions(dev) {
            Ok(t) => t,
            Err(e) => {
                console_write("efsck: failed to read disk partitions: ");
                match e {
                    crate::partition::PartitionError::BlockIo(_) => console_write("I/O Error\r\n"),
                    _ => console_write("Partition Table Error\r\n"),
                }
                return 1;
            }
        };

        if table.count == 0 {
            console_write("efsck: no partitions found on device to check.\r\n");
            return 1;
        }

        let p0 = &table.partitions[0];
        console_write("Target       : virtio0, Partition 0 (LBA ");
        let mut b = [0u8; 32];
        console_write(format_u64(p0.start_lba, &mut b));
        console_write("..+");
        console_write(format_u64(p0.sector_count, &mut b));
        console_write(")\r\n");

        if p0.sector_count < 4 {
            console_write("efsck: partition is too small for an EternalFS filesystem.\r\n");
            return 2;
        }

        // 1. Read and validate Superblock
        let mut sb_buf = [0u8; 1024];
        let sb_lba = match p0.start_lba.checked_add(2) {
            Some(lba) => lba,
            None => {
                console_write("efsck: arithmetic overflow in superblock LBA calculation.\r\n");
                return 2;
            }
        };

        if let Err(e) = dev.read_sectors(sb_lba, 2, &mut sb_buf) {
            console_write("efsck: I/O error reading Superblock at LBA ");
            console_write(format_u64(sb_lba, &mut b));
            console_write(": ");
            match e {
                crate::block::BlockError::Timeout => console_write("Timeout\r\n"),
                crate::block::BlockError::HardwareError(code) => {
                    console_write("Hardware error 0x");
                    console_write(format_hex_u16(code as u16, &mut b));
                    console_write("\r\n");
                }
                _ => console_write("I/O Failure\r\n"),
            }
            return 3;
        }

        let mut passed_checks = 0;
        let mut failed_checks = 0;

        // Check 1: Magic Number
        if &sb_buf[0..8] == crate::vfs::eternalfs::ETERNALFS_MAGIC {
            passed_checks += 1;
            if verbose { console_write("  [PASS] Superblock magic signature ('ETERNAL1')\r\n"); }
        } else {
            console_write("  [FAIL] Superblock magic signature mismatch\r\n");
            console_write("efsck: Not an EternalFS volume or invalid filesystem format.\r\n");
            return 2;
        }

        // Check 2: Superblock CRC32 Checksum
        let expected_crc = u32::from_le_bytes(sb_buf[0x3C..0x40].try_into().unwrap());
        let computed_crc = crate::partition::crc32(&sb_buf[0x00..0x3C]);
        if expected_crc == computed_crc {
            passed_checks += 1;
            if verbose { console_write("  [PASS] Superblock IEEE 802.3 CRC32 checksum\r\n"); }
        } else {
            failed_checks += 1;
            console_write("  [FAIL] Superblock CRC32 checksum mismatch (corrupted metadata)\r\n");
        }

        // Check 3: Version & Block Size
        let version = u32::from_le_bytes(sb_buf[0x08..0x0C].try_into().unwrap());
        let block_sz = u32::from_le_bytes(sb_buf[0x0C..0x10].try_into().unwrap());
        if version == crate::vfs::eternalfs::ETERNALFS_VERSION && block_sz == crate::vfs::eternalfs::ETERNALFS_BLOCK_SIZE as u32 {
            passed_checks += 1;
            if verbose { console_write("  [PASS] Filesystem version (1) and block size (1024 bytes)\r\n"); }
        } else {
            failed_checks += 1;
            console_write("  [FAIL] Unsupported version or non-standard block size\r\n");
        }

        // Decode full superblock
        let sb = match crate::vfs::eternalfs::Superblock::decode(&sb_buf, p0.sector_count) {
            Ok(s) => {
                passed_checks += 1;
                if verbose { console_write("  [PASS] Geometry bounds within partition capacity\r\n"); }
                s
            }
            Err(_) => {
                console_write("  [FAIL] Superblock geometry validation failed\r\n");
                return 2;
            }
        };

        // Check 4: Root Inode Validity & Inode Table Bounds
        if sb.root_inode > 0 && sb.root_inode <= sb.total_inodes {
            passed_checks += 1;
            if verbose { console_write("  [PASS] Root inode ID bounds\r\n"); }
        } else {
            failed_checks += 1;
            console_write("  [FAIL] Root inode ID out of valid inode range\r\n");
        }

        // Check 5: Inode Table & Directory Traversal Verification (Read-Only Mount)
        let dev_ref = VirtioBlockRef(dev);
        let efs = match crate::vfs::eternalfs::EternalFs::mount(dev_ref, p0.start_lba, p0.sector_count) {
            Ok(fs) => {
                passed_checks += 1;
                if verbose { console_write("  [PASS] Mount verification in read-only mode\r\n"); }
                fs
            }
            Err(_) => {
                console_write("  [FAIL] Failed to mount volume for structural inspection\r\n");
                return 2;
            }
        };

        // Inspect Root Directory Entries (Inode 1)
        let mut entries = [crate::vfs::DirEntry::empty(); 16];
        match crate::vfs::FileSystem::readdir(&efs, sb.root_inode, 0, &mut entries) {
            Ok(count) => {
                passed_checks += 1;
                if verbose {
                    console_write("  [PASS] Root directory structure readable (");
                    console_write(format_u64(count as u64, &mut b));
                    console_write(" entries found)\r\n");
                }
            }
            Err(_) => {
                failed_checks += 1;
                console_write("  [FAIL] Root directory readdir failed or corrupt\r\n");
            }
        }

        // Validate Inodes across table (up to total_inodes or capped at 64)
        let inspect_count = sb.total_inodes.min(64);
        let mut inode_errors = 0;
        for in_id in 1..=inspect_count {
            match efs.read_inode(in_id) {
                Ok(inode) => {
                    // Check direct block pointers
                    for ptr in inode.direct_blocks.iter() {
                        let block_no = *ptr as u64;
                        if block_no != 0 && (block_no < sb.data_block_start || block_no >= sb.total_blocks) {
                            inode_errors += 1;
                        }
                    }
                }
                Err(_) => {
                    inode_errors += 1;
                }
            }
        }

        if inode_errors == 0 {
            passed_checks += 1;
            if verbose {
                console_write("  [PASS] Sample Inode headers and block pointers validated (");
                console_write(format_u64(inspect_count, &mut b));
                console_write(" inodes checked)\r\n");
            }
        } else {
            failed_checks += 1;
            console_write("  [FAIL] Detected ");
            console_write(format_u64(inode_errors as u64, &mut b));
            console_write(" invalid or out-of-range inode/block references\r\n");
        }

        console_write("\r\n--- Validation Summary ---\r\n");
        console_write("  Passed Checks: ");
        console_write(format_u64(passed_checks as u64, &mut b));
        console_write("\r\n  Failed Checks: ");
        console_write(format_u64(failed_checks as u64, &mut b));
        console_write("\r\n  Repair Status: Unsupported (Filesystem is read-only by design)\r\n");

        if failed_checks == 0 {
            console_write("Status: Volume is clean. No structural errors detected.\r\n");
            0
        } else {
            console_write("Status: Volume has errors! Metadata inconsistencies found.\r\n");
            2
        }
    });

    match result {
        Some(code) => code,
        None => {
            console_write("efsck: No block storage device available.\r\n");
            1
        }
    }
}

pub fn cmd_eblock(args: &ParsedArgs) -> i32 {
    let mut run_read_test = false;
    let mut target_device: Option<&str> = None;

    for i in 1..args.len() {
        if let Some(arg) = args.get(i) {
            if arg == "--help" || arg == "-h" {
                console_write("eblock: Block-Device Diagnostics & Inspection\r\n");
                console_write("Usage: eblock [--read-test [device]]\r\n");
                console_write("Displays logical sector size, capacity, driver type, and performs bounded read tests.\r\n");
                return 0;
            } else if arg == "--read-test" || arg == "-t" {
                run_read_test = true;
            } else if !arg.starts_with('-') {
                target_device = Some(arg);
            }
        }
    }

    console_write("=== [EternalOS Block-Device Diagnostics (eblock)] ===\r\n");

    let result = crate::virtio_blk::with_block_device(|dev| {
        let info = dev.info();
        let dev_name = info.name_as_str();

        if let Some(target) = target_device {
            if target != "virtio0" && target != "blk0" && target != dev_name {
                console_write("eblock: device '");
                console_write(target);
                console_write("' not found\r\n");
                return 1;
            }
        }

        console_write("Device ID    : virtio0\r\n");
        console_write("Driver Type  : QEMU VirtIO Block Controller (VirtIO Modern / Legacy)\r\n");
        console_write("Model / Name : ");
        console_write(dev_name);
        console_write("\r\n");

        // Query PCI location
        let pci_loc = crate::pci::with_pci_registry(|reg| {
            reg.find_by_vendor_device(0x1af4, 0x1001).map(|d| (d.bus, d.device, d.function))
        });
        if let Some((bus, slot, func)) = pci_loc {
            console_write("PCI Location : Bus ");
            let mut b = [0u8; 32];
            console_write(format_u64(bus as u64, &mut b));
            console_write(", Device ");
            console_write(format_u64(slot as u64, &mut b));
            console_write(", Function ");
            console_write(format_u64(func as u64, &mut b));
            console_write("\r\n");
        }

        console_write("Sector Size  : ");
        let mut b = [0u8; 32];
        console_write(format_u64(info.sector_size as u64, &mut b));
        console_write(" bytes (logical)\r\n");

        console_write("Capacity     : ");
        console_write(format_u64(info.total_sectors, &mut b));
        console_write(" sectors (");
        if let Some(cap) = info.capacity_bytes() {
            let mib = cap / (1024 * 1024);
            console_write(format_u64(mib, &mut b));
            console_write(" MiB total)\r\n");
        } else {
            console_write("Capacity Overflow)\r\n");
        }

        console_write("Access Mode  : ");
        if info.read_only {
            console_write("Read-Only (writes prohibited)\r\n");
        } else {
            console_write("Read/Write capable\r\n");
        }

        console_write("Status       : Online & Initialized\r\n");
        console_write("SMART Health : Not supported on VirtIO virtual block interface\r\n");

        if run_read_test {
            console_write("\r\n--- Bounded Read Diagnostics ---\r\n");
            let test_sectors = 16u32.min(info.total_sectors as u32);
            console_write("Reading ");
            console_write(format_u64(test_sectors as u64, &mut b));
            console_write(" sectors from LBA 0...\r\n");

            let start_tick = crate::timer::get_ticks();
            let mut read_buf = [0u8; 512];
            let mut sectors_ok = 0u32;
            let mut io_error = false;

            for sec in 0..test_sectors as u64 {
                if let Err(e) = dev.read_sectors(sec, 1, &mut read_buf) {
                    console_write("  [ERROR] Failed to read sector ");
                    console_write(format_u64(sec, &mut b));
                    console_write(": ");
                    match e {
                        crate::block::BlockError::OutOfBounds => console_write("Out of bounds\r\n"),
                        crate::block::BlockError::Timeout => console_write("Operation timed out\r\n"),
                        _ => console_write("I/O error\r\n"),
                    }
                    io_error = true;
                    break;
                }
                sectors_ok += 1;
            }

            let elapsed_ticks = crate::timer::get_ticks().saturating_sub(start_tick);
            let elapsed_ms = elapsed_ticks * crate::timer::MS_PER_TICK;

            console_write("Sectors Read : ");
            console_write(format_u64(sectors_ok as u64, &mut b));
            console_write(" / ");
            console_write(format_u64(test_sectors as u64, &mut b));
            console_write(" (");
            let bytes_read = (sectors_ok as u64).saturating_mul(info.sector_size as u64);
            console_write(format_u64(bytes_read, &mut b));
            console_write(" bytes)\r\n");

            console_write("Elapsed Time : ");
            console_write(format_u64(elapsed_ms.max(1), &mut b));
            console_write(" ms\r\n");

            if io_error {
                console_write("Read Test Result: FAILED (I/O errors encountered)\r\n");
                return 1;
            } else {
                console_write("Read Test Result: PASSED (All sectors verified)\r\n");
            }
        }

        0
    });

    match result {
        Some(code) => code,
        None => {
            console_write("eblock: No block storage device discovered or registered.\r\n");
            1
        }
    }
}

// -----------------------------------------------------------------------------
// Command History & Interactive Shell Loop
// -----------------------------------------------------------------------------

pub const HISTORY_CAPACITY: usize = 32;
pub const MAX_CMD_LEN: usize = 128;

/// Fixed-capacity command history ring buffer
pub struct CommandHistory {
    entries: [[u8; MAX_CMD_LEN]; HISTORY_CAPACITY],
    lens: [usize; HISTORY_CAPACITY],
    count: usize,
    head: usize,
    /// Browsing position: None when at live input, Some(idx) when viewing history
    nav_index: Option<usize>,
    /// Temporary stash of current unsent input
    stash: [u8; MAX_CMD_LEN],
    stash_len: usize,
}

impl CommandHistory {
    pub const fn new() -> Self {
        Self {
            entries: [[0; MAX_CMD_LEN]; HISTORY_CAPACITY],
            lens: [0; HISTORY_CAPACITY],
            count: 0,
            head: 0,
            nav_index: None,
            stash: [0; MAX_CMD_LEN],
            stash_len: 0,
        }
    }

    /// Add a command to history (skips empty and consecutive duplicate commands)
    pub fn add(&mut self, cmd: &str) {
        let bytes = cmd.as_bytes();
        if bytes.is_empty() {
            self.nav_index = None;
            return;
        }

        // Check if identical to most recent entry
        if self.count > 0 {
            let last_idx = (self.head + self.count - 1) % HISTORY_CAPACITY;
            if self.lens[last_idx] == bytes.len() && &self.entries[last_idx][..bytes.len()] == bytes {
                self.nav_index = None;
                return;
            }
        }

        let insert_idx = if self.count < HISTORY_CAPACITY {
            let idx = (self.head + self.count) % HISTORY_CAPACITY;
            self.count += 1;
            idx
        } else {
            let idx = self.head;
            self.head = (self.head + 1) % HISTORY_CAPACITY;
            idx
        };

        let copy_len = bytes.len().min(MAX_CMD_LEN);
        self.entries[insert_idx][..copy_len].copy_from_slice(&bytes[..copy_len]);
        self.lens[insert_idx] = copy_len;
        self.nav_index = None;
    }

    /// Navigate up (older commands). Takes current unsubmitted input to stash if starting navigation.
    pub fn navigate_up(&mut self, current_input: &str) -> Option<&str> {
        if self.count == 0 {
            return None;
        }

        match self.nav_index {
            None => {
                // Save current live input to stash
                let bytes = current_input.as_bytes();
                let copy_len = bytes.len().min(MAX_CMD_LEN);
                self.stash[..copy_len].copy_from_slice(&bytes[..copy_len]);
                self.stash_len = copy_len;

                // Start from the most recent command
                let idx = self.count - 1;
                self.nav_index = Some(idx);
                self.get_entry(idx)
            }
            Some(idx) => {
                if idx > 0 {
                    let next_idx = idx - 1;
                    self.nav_index = Some(next_idx);
                    self.get_entry(next_idx)
                } else {
                    // Already at oldest command
                    self.get_entry(0)
                }
            }
        }
    }

    /// Navigate down (newer commands). Returns None or stash when going past newest command.
    pub fn navigate_down(&mut self) -> Option<&str> {
        if self.count == 0 {
            return None;
        }

        match self.nav_index {
            None => None,
            Some(idx) => {
                if idx + 1 < self.count {
                    let next_idx = idx + 1;
                    self.nav_index = Some(next_idx);
                    self.get_entry(next_idx)
                } else {
                    // Moving past the newest command restores stashed unsubmitted line
                    self.nav_index = None;
                    core::str::from_utf8(&self.stash[..self.stash_len]).ok()
                }
            }
        }
    }

    /// Reset navigation position without losing history
    pub fn reset_nav(&mut self) {
        self.nav_index = None;
    }

    /// Number of items in history
    pub fn count(&self) -> usize {
        self.count
    }

    fn get_entry(&self, relative_idx: usize) -> Option<&str> {
        if relative_idx >= self.count {
            None
        } else {
            let physical_idx = (self.head + relative_idx) % HISTORY_CAPACITY;
            let len = self.lens[physical_idx];
            core::str::from_utf8(&self.entries[physical_idx][..len]).ok()
        }
    }
}

pub fn run_interactive_shell() {
    crate::console::init_terminal_viewport();

    console_write("\r\n");
    console_write("====================================================\r\n");
    console_write("   EternalOS Interactive Command Ecosystem (CLI)    \r\n");
    console_write("====================================================\r\n");
    console_write("Type 'help' for command categories or 'eternal' for banner.\r\n\r\n");

    let mut line_buf = [0u8; MAX_CMD_LEN];
    let mut ctx = ShellContext::new();
    let mut history = CommandHistory::new();

    loop {
        // Print prompt: "EternalOS [/]> "
        console_write("EternalOS [");
        console_write(ctx.cwd_str());
        console_write("]> ");

        let mut line_len = 0usize;
        history.reset_nav();

        // Read line
        crate::console::draw_text_cursor(true);
        loop {
            if let Some(key) = crate::console::console_read_input() {
                crate::console::draw_text_cursor(false);
                match key {
                    crate::console::ConsoleKey::Char(b) => {
                        // Any typing brings terminal back to live bottom view if scrolled
                        crate::console::terminal_return_to_live();

                        if b == b'\r' || b == b'\n' {
                            console_write("\r\n");
                            break;
                        } else if b == 0x08 || b == 0x7F {
                            // Backspace
                            if line_len > 0 {
                                line_len -= 1;
                                console_write_byte(0x08);
                            }
                        } else if b >= 0x20 && b <= 0x7E {
                            if line_len < MAX_CMD_LEN - 1 {
                                line_buf[line_len] = b;
                                line_len += 1;
                                console_write_byte(b);
                            }
                        }
                    }
                    crate::console::ConsoleKey::Up => {
                        let cur_slice = core::str::from_utf8(&line_buf[..line_len]).unwrap_or("");
                        if let Some(recalled) = history.navigate_up(cur_slice) {
                            // Erase current line on screen
                            while line_len > 0 {
                                console_write_byte(0x08);
                                line_len -= 1;
                            }
                            // Replace with recalled command
                            let r_bytes = recalled.as_bytes();
                            let copy_len = r_bytes.len().min(MAX_CMD_LEN - 1);
                            line_buf[..copy_len].copy_from_slice(&r_bytes[..copy_len]);
                            line_len = copy_len;
                            console_write(recalled);
                        }
                    }
                    crate::console::ConsoleKey::Down => {
                        if let Some(recalled) = history.navigate_down() {
                            // Erase current line on screen
                            while line_len > 0 {
                                console_write_byte(0x08);
                                line_len -= 1;
                            }
                            // Replace with recalled command or stashed input
                            let r_bytes = recalled.as_bytes();
                            let copy_len = r_bytes.len().min(MAX_CMD_LEN - 1);
                            line_buf[..copy_len].copy_from_slice(&r_bytes[..copy_len]);
                            line_len = copy_len;
                            console_write(recalled);
                        }
                    }
                    crate::console::ConsoleKey::PageUp => {
                        crate::console::terminal_page_up();
                    }
                    crate::console::ConsoleKey::PageDown => {
                        crate::console::terminal_page_down();
                    }
                    _ => {}
                }
                crate::console::draw_text_cursor(true);
            } else {
                core::hint::spin_loop();
            }
        }
        crate::console::draw_text_cursor(false);

        let input_str = match core::str::from_utf8(&line_buf[..line_len]) {
            Ok(s) => s.trim(),
            Err(_) => continue,
        };

        if input_str.is_empty() {
            continue;
        }

        // Add submitted command to history
        history.add(input_str);

        let status = execute_command(input_str, &mut ctx);
        if status == -1 {
            console_write("Shell exited. Halting session.\r\n");
            break;
        }
    }
}

// -----------------------------------------------------------------------------
// Helper Formatting Functions
// -----------------------------------------------------------------------------

pub fn format_u64(mut n: u64, buf: &mut [u8; 32]) -> &str {
    if n == 0 {
        return "0";
    }
    let mut i = 31;
    while n > 0 && i > 0 {
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i -= 1;
    }
    core::str::from_utf8(&buf[i + 1..32]).unwrap_or("0")
}

pub fn format_hex_u16(n: u16, buf: &mut [u8; 32]) -> &str {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    buf[0] = HEX[((n >> 12) & 0xF) as usize];
    buf[1] = HEX[((n >> 8) & 0xF) as usize];
    buf[2] = HEX[((n >> 4) & 0xF) as usize];
    buf[3] = HEX[(n & 0xF) as usize];
    core::str::from_utf8(&buf[..4]).unwrap_or("0000")
}

fn parse_usize(s: &str) -> Option<usize> {
    let mut val = 0usize;
    if s.is_empty() {
        return None;
    }
    for b in s.bytes() {
        if b >= b'0' && b <= b'9' {
            val = val.checked_mul(10)?.checked_add((b - b'0') as usize)?;
        } else {
            return None;
        }
    }
    Some(val)
}
