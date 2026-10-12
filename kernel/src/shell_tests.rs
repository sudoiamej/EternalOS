use crate::shell::{execute_command, ShellContext, COMMAND_REGISTRY};
use crate::shell_parser::parse_command_line;
use crate::{log_info, println};

/// Automated test suite for the Command Suite & Native CLI Ecosystem
pub fn run_phase_shell_tests() {
    log_info!("====================================================");
    log_info!("  RUNNING AUTOMATED TEST SUITE: COMMAND SUITE & CLI ");
    log_info!("====================================================");

    let mut ctx = ShellContext::new();

    // 1. Test Command Parser & Quoted Arguments
    test_argument_parsing();

    // 2. Test Command Registry & Aliases
    test_command_registry();

    // 3. Test Shell Built-in Commands (echo, which, pwd, etc.)
    test_builtin_commands(&mut ctx);

    // 4. Test Filesystem Operations (mkdir, touch, cp, stat, cat, rm)
    test_filesystem_commands(&mut ctx);

    // 5. Test System and Hardware Diagnostic Commands (esys, emem, epci, echeck)
    test_system_commands(&mut ctx);

    // 6. Test Terminal Scrollback Buffer
    test_scrollback_buffer();

    // 7. Test Command History Navigation & Stash
    test_command_history();

    log_info!("====================================================");
    log_info!("  ALL COMMAND SUITE TESTS COMPLETED SUCCESSFULLY!   ");
    log_info!("====================================================");
    println!(">>> SHELL TEST PASS: All CLI commands, scrollback, and history verified.");
}

fn test_argument_parsing() {
    log_info!("[TEST 1/5] Testing Quoted Argument Parser...");

    // Basic space separation
    let p1 = parse_command_line("echo hello world");
    assert_eq!(p1.len(), 3);
    assert_eq!(p1.get(0), Some("echo"));
    assert_eq!(p1.get(1), Some("hello"));
    assert_eq!(p1.get(2), Some("world"));

    // Double quotes with spaces
    let p2 = parse_command_line("echo \"hello world from eternal\"");
    assert_eq!(p2.len(), 2);
    assert_eq!(p2.get(0), Some("echo"));
    assert_eq!(p2.get(1), Some("hello world from eternal"));

    // Single quotes
    let p3 = parse_command_line("mkdir 'test dir'");
    assert_eq!(p3.len(), 2);
    assert_eq!(p3.get(0), Some("mkdir"));
    assert_eq!(p3.get(1), Some("test dir"));

    // Leading and trailing spaces
    let p4 = parse_command_line("   ls   /   ");
    assert_eq!(p4.len(), 2);
    assert_eq!(p4.get(0), Some("ls"));
    assert_eq!(p4.get(1), Some("/"));

    log_info!("  [OK] Argument parsing with quotes and whitespace verified.");
}

fn test_command_registry() {
    log_info!("[TEST 2/5] Testing Command Registry & Alias Resolution...");

    assert!(COMMAND_REGISTRY.len() >= 25, "Registry must contain at least 25 commands");

    // Verify key commands exist
    let commands = ["help", "ls", "cd", "cat", "mkdir", "touch", "cp", "rm", "eternal", "esys", "emem", "epci", "echeck"];
    for cmd in commands {
        assert!(COMMAND_REGISTRY.iter().any(|c| c.name == cmd), "Command '{}' must be registered", cmd);
    }

    // Verify aliases
    let alias_cases = [("cls", "clear"), ("dir", "ls"), ("type", "cat"), ("meminfo", "emem"), ("sysinfo", "esys"), ("lspci", "epci")];
    for (alias, canonical) in alias_cases {
        let entry = COMMAND_REGISTRY.iter().find(|c| c.name == canonical).expect("Canonical command not found");
        assert!(entry.aliases.iter().any(|&a| a == alias), "Alias '{}' must map to '{}'", alias, canonical);
    }

    log_info!("  [OK] Registry coverage and alias lookup verified.");
}

fn test_builtin_commands(ctx: &mut ShellContext) {
    log_info!("[TEST 3/5] Testing Shell Built-ins & Navigation...");

    // Echo
    let s_echo = execute_command("echo test", ctx);
    assert_eq!(s_echo, 0);

    // Which
    let s_which = execute_command("which ls", ctx);
    assert_eq!(s_which, 0);

    // Alias
    let s_alias = execute_command("alias", ctx);
    assert_eq!(s_alias, 0);

    // Pwd
    let s_pwd = execute_command("pwd", ctx);
    assert_eq!(s_pwd, 0);
    assert_eq!(ctx.cwd_str(), "/");

    // Invalid command returns 127
    let s_invalid = execute_command("nonexistent_command_xyz", ctx);
    assert_eq!(s_invalid, 127);

    log_info!("  [OK] Shell built-ins and exit codes verified.");
}

fn test_filesystem_commands(ctx: &mut ShellContext) {
    log_info!("[TEST 4/5] Testing Filesystem Commands (mkdir, touch, cp, stat, rm)...");

    // 1. mkdir /cli_test
    let s_mkdir = execute_command("mkdir /cli_test", ctx);
    assert_eq!(s_mkdir, 0, "mkdir /cli_test failed");

    // 2. cd /cli_test
    let s_cd = execute_command("cd /cli_test", ctx);
    assert_eq!(s_cd, 0, "cd /cli_test failed");
    assert_eq!(ctx.cwd_str(), "/cli_test");

    // 3. touch file1.txt
    let s_touch = execute_command("touch file1.txt", ctx);
    assert_eq!(s_touch, 0, "touch file1.txt failed");

    // 4. stat file1.txt
    let s_stat = execute_command("stat file1.txt", ctx);
    assert_eq!(s_stat, 0, "stat file1.txt failed");

    // 5. cp file1.txt file2.txt
    let s_cp = execute_command("cp file1.txt file2.txt", ctx);
    assert_eq!(s_cp, 0, "cp file1.txt file2.txt failed");

    // 6. ls
    let s_ls = execute_command("ls", ctx);
    assert_eq!(s_ls, 0, "ls failed");

    // 7. rm file1.txt
    let s_rm = execute_command("rm file1.txt", ctx);
    assert_eq!(s_rm, 0, "rm file1.txt failed");

    // Verify file1.txt is gone
    let s_stat_gone = execute_command("stat file1.txt", ctx);
    assert_eq!(s_stat_gone, 1, "file1.txt should no longer exist");

    // 8. rm file2.txt
    let s_rm2 = execute_command("rm file2.txt", ctx);
    assert_eq!(s_rm2, 0, "rm file2.txt failed");

    // 9. cd / and rm /cli_test
    let s_cd_root = execute_command("cd /", ctx);
    assert_eq!(s_cd_root, 0);
    let s_rmdir = execute_command("rm /cli_test", ctx);
    assert_eq!(s_rmdir, 0, "rm empty dir failed");

    log_info!("  [OK] Filesystem manipulation lifecycle (CRUD) verified.");
}

fn test_system_commands(ctx: &mut ShellContext) {
    log_info!("[TEST 5/5] Testing System, Hardware & Integrity Commands...");

    let s_eternal = execute_command("eternal", ctx);
    assert_eq!(s_eternal, 0);

    let s_esys = execute_command("esys", ctx);
    assert_eq!(s_esys, 0);

    let s_emem = execute_command("emem", ctx);
    assert_eq!(s_emem, 0);

    let s_eproc = execute_command("eproc", ctx);
    assert_eq!(s_eproc, 0);

    let s_emount = execute_command("emount", ctx);
    assert_eq!(s_emount, 0);

    let s_uptime = execute_command("uptime", ctx);
    assert_eq!(s_uptime, 0);

    let s_epci = execute_command("epci", ctx);
    assert_eq!(s_epci, 0);

    let s_edrv = execute_command("edrv", ctx);
    assert_eq!(s_edrv, 0);

    // Test new disk utilities: epart, efsck, eblock
    let s_epart_help = execute_command("epart --help", ctx);
    assert_eq!(s_epart_help, 0);

    let s_efsck_help = execute_command("efsck --help", ctx);
    assert_eq!(s_efsck_help, 0);

    let s_eblock_help = execute_command("eblock --help", ctx);
    assert_eq!(s_eblock_help, 0);

    // Run active inspection on real attached VirtIO disk
    let s_eblock = execute_command("eblock", ctx);
    assert_eq!(s_eblock, 0);

    let s_eblock_test = execute_command("eblock --read-test", ctx);
    assert_eq!(s_eblock_test, 0);

    let s_epart = execute_command("epart", ctx);
    assert_eq!(s_epart, 0);

    let s_epart_verb = execute_command("epart --verbose", ctx);
    assert_eq!(s_epart_verb, 0);

    let s_echeck = execute_command("echeck", ctx);
    assert_eq!(s_echeck, 0);

    let s_einfo = execute_command("einfo", ctx);
    assert_eq!(s_einfo, 0);

    log_info!("  [OK] System and diagnostics commands verified.");
}

fn test_scrollback_buffer() {
    log_info!("[TEST 6/7] Testing Terminal Scrollback Buffer...");

    use crate::console::ScrollbackBuffer;
    let mut sb = ScrollbackBuffer::new();

    // Push characters
    for b in b"Hello Scrollback" {
        sb.push_char(*b);
    }
    assert_eq!(sb.count, 1);
    let line = sb.get_line_from_bottom(0).expect("Should have bottom line");
    assert_eq!(&line.chars[..line.len], b"Hello Scrollback");

    // Add multiple lines exceeding TERM_ROWS (24)
    for i in 0..30 {
        sb.newline();
        sb.push_char(b'0' + (i % 10) as u8);
    }
    assert_eq!(sb.count, 31);

    // Initial scroll offset is 0
    assert_eq!(sb.scroll_offset, 0);
    assert!(!sb.is_scrolled());

    // Scroll up by 5 lines
    sb.scroll_up(5);
    assert_eq!(sb.scroll_offset, 5);
    assert!(sb.is_scrolled());

    // Scroll up beyond capacity (clamped to count - TERM_ROWS = 31 - 24 = 7)
    sb.scroll_up(20);
    assert_eq!(sb.scroll_offset, 7);

    // Scroll down by 3 lines
    sb.scroll_down(3);
    assert_eq!(sb.scroll_offset, 4);

    // Reset scroll back to live bottom view
    sb.reset_scroll();
    assert_eq!(sb.scroll_offset, 0);
    assert!(!sb.is_scrolled());

    log_info!("  [OK] Terminal scrollback buffer ring and scrolling clamp verified.");
}

fn test_command_history() {
    log_info!("[TEST 7/7] Testing Interactive Command History & Navigation...");

    use crate::shell::CommandHistory;
    let mut hist = CommandHistory::new();

    assert_eq!(hist.count(), 0);
    assert_eq!(hist.navigate_up(""), None);
    assert_eq!(hist.navigate_down(), None);

    // Empty strings are ignored
    hist.add("");
    assert_eq!(hist.count(), 0);

    // Add first command
    hist.add("ls");
    assert_eq!(hist.count(), 1);

    // Consecutive duplicate is ignored
    hist.add("ls");
    assert_eq!(hist.count(), 1);

    // Add more commands
    hist.add("cd /");
    hist.add("cat file.txt");
    assert_eq!(hist.count(), 3);

    // Navigate Up from empty input: recalls "cat file.txt"
    let c1 = hist.navigate_up("unsubmitted typing");
    assert_eq!(c1, Some("cat file.txt"));

    // Navigate Up again: recalls "cd /"
    let c2 = hist.navigate_up("cat file.txt");
    assert_eq!(c2, Some("cd /"));

    // Navigate Up again: recalls "ls"
    let c3 = hist.navigate_up("cd /");
    assert_eq!(c3, Some("ls"));

    // Navigate Up at oldest: stays at "ls"
    let c4 = hist.navigate_up("ls");
    assert_eq!(c4, Some("ls"));

    // Navigate Down: recalls "cd /"
    let d1 = hist.navigate_down();
    assert_eq!(d1, Some("cd /"));

    // Navigate Down: recalls "cat file.txt"
    let d2 = hist.navigate_down();
    assert_eq!(d2, Some("cat file.txt"));

    // Navigate Down past newest: restores unsubmitted typing!
    let d3 = hist.navigate_down();
    assert_eq!(d3, Some("unsubmitted typing"));

    // Navigate Down again: None
    let d4 = hist.navigate_down();
    assert_eq!(d4, None);

    log_info!("  [OK] Command history ring, deduplication, and stash restore verified.");
}
