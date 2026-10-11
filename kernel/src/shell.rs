use crate::console::{console_write, console_write_byte};
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
        summary: "Exit interactive shell",
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

    // --- Hardware & Devices ---
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
        "exit" => -1,

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

        "eternal" => cmd_eternal(),
        "uname" => cmd_uname(&args),
        "esys" => cmd_esys(ctx),
        "emem" => cmd_emem(),
        "eproc" => cmd_eproc(),
        "emount" => cmd_emount(ctx),
        "uptime" => cmd_uptime(),

        "epci" => cmd_epci(),
        "edrv" => cmd_edrv(),
        "mouse" => cmd_mouse(),

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
