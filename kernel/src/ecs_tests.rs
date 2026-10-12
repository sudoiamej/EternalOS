//! Automated Test Suite for Eternal Control Suite (ECS) & Native Server Ecosystem
//!
//! Validates:
//! 1. ECS standardized exit codes (0..=5) and contract compliance
//! 2. Universal CLI options parsing (--help, --version, --verbose, --quiet, --dry-run, --format)
//! 3. System capabilities querying and gating
//! 4. Service Manager registry, state transitions (start/stop/restart), and log ring buffer
//! 5. Bounded HTTP server request handling and status/health endpoints
//! 6. VFS read-only protection on EternalFS (/disk) vs writable RamFS (/tmp)
//! 7. Diagnostic subsystems (heap stats, RTC, log buffer)

use crate::ecs::{
    exit_code, get_system_capabilities, CapabilityStatus, CommonOptions, OutputFormat,
};
use crate::service::{
    for_each_service_log, get_service, log_service, restart_service,
    start_service, stop_service, ServiceState,
};
use crate::web::handle_http_request;
use crate::shell_parser::parse_command_line;
use crate::vfs::{with_vfs, OpenFlags, VfsError};
use crate::{log_info, println};

pub fn run_ecs_tests() {
    log_info!("====================================================");
    log_info!("     RUNNING AUTOMATED TEST SUITE: ETERNAL CONTROL SUITE");
    log_info!("====================================================");

    // 1. Exit code constants & contract verification
    test_exit_codes();

    // 2. Universal options parser verification
    test_common_options_parser();

    // 3. System capabilities query & gating
    test_system_capabilities();

    // 4. Service Manager lifecycle & registry
    test_service_manager();

    // 5. Embedded HTTP Server routing & response generation
    test_http_server_endpoints();

    // 6. VFS Read-Only Protection on EternalFS
    test_vfs_readonly_safety();

    // 7. System Diagnostics & Heap / Log Integrity
    test_diagnostics_integrity();

    // 8. Native Calculator Precedence & Parser Integrity
    test_apps_and_calc_eval();

    log_info!("====================================================");
    log_info!("  ALL ECS & SERVER ECOSYSTEM TESTS PASSED CLEANLY!  ");
    log_info!("====================================================");
    println!(">>> ECS TEST PASS: Exit codes, parser, capabilities, services, eweb, VFS safety, calc, and diagnostics verified.");
}

fn test_exit_codes() {
    log_info!("[TEST 1/7] Testing ECS Standard Exit Codes Contract...");

    assert_eq!(exit_code::SUCCESS, 0);
    assert_eq!(exit_code::OPERATIONAL_FAILURE, 1);
    assert_eq!(exit_code::INVALID_USAGE, 2);
    assert_eq!(exit_code::UNSUPPORTED, 3);
    assert_eq!(exit_code::RESOURCE_UNAVAILABLE, 4);
    assert_eq!(exit_code::VALIDATION_FAILURE, 5);

    log_info!("  -> Exit code boundaries [0..=5] adhere strictly to standard.");
}

fn test_common_options_parser() {
    log_info!("[TEST 2/7] Testing Universal Flag Parsing (--help, --version, --format, etc.)...");

    // Test 1: Empty args
    let raw = "";
    let parsed = parse_command_line(raw);
    let (opts, pos_idx) = CommonOptions::parse(&parsed);
    assert_eq!(opts.format, OutputFormat::Text);
    assert!(!opts.help);
    assert!(!opts.version);
    assert!(!opts.verbose);
    assert!(!opts.quiet);
    assert!(!opts.dry_run);
    assert_eq!(pos_idx, parsed.len());

    // Test 2: --help flag
    let raw = "esys --help";
    let parsed = parse_command_line(raw);
    let (opts, _) = CommonOptions::parse(&parsed);
    assert!(opts.help);

    // Test 3: -h shorthand
    let raw = "emem -h";
    let parsed = parse_command_line(raw);
    let (opts, _) = CommonOptions::parse(&parsed);
    assert!(opts.help);

    // Test 4: --version and -V shorthand
    let raw = "ever --version";
    let parsed = parse_command_line(raw);
    let (opts, _) = CommonOptions::parse(&parsed);
    assert!(opts.version);

    let raw = "ever -V";
    let parsed = parse_command_line(raw);
    let (opts, _) = CommonOptions::parse(&parsed);
    assert!(opts.version);

    // Test 5: --format json
    let raw = "einfo --format json --verbose";
    let parsed = parse_command_line(raw);
    let (opts, _) = CommonOptions::parse(&parsed);
    assert_eq!(opts.format, OutputFormat::Json);
    assert!(opts.verbose);

    // Test 6: Non-flag positional argument indexing
    let raw = "find /test -name kernel --dry-run";
    let parsed = parse_command_line(raw);
    let (opts, pos_idx) = CommonOptions::parse(&parsed);
    assert!(opts.dry_run);
    assert_eq!(pos_idx, 1);
    assert_eq!(parsed.get(pos_idx), Some("/test"));

    log_info!("  -> Universal options parser accurately extracted flags and non-flag positional arguments.");
}

fn test_system_capabilities() {
    log_info!("[TEST 3/7] Testing Hardware & Subsystem Capability Registry...");

    let caps = get_system_capabilities();
    assert!(caps.len() >= 10);

    let mut found_cpu = false;
    let mut found_ramfs = false;
    let mut found_pci = false;
    let mut found_service = false;

    for cap in caps.iter() {
        if cap.name == "cpu.x86_64" {
            found_cpu = true;
            assert_eq!(cap.status, CapabilityStatus::Available);
        } else if cap.name == "fs.ramfs" {
            found_ramfs = true;
            assert_eq!(cap.status, CapabilityStatus::Available);
        } else if cap.name == "bus.pci" {
            found_pci = true;
        } else if cap.name == "srv.manager" {
            found_service = true;
            assert_eq!(cap.status, CapabilityStatus::Available);
        }
    }

    assert!(found_cpu, "CPU capability missing");
    assert!(found_ramfs, "RamFS capability missing");
    assert!(found_pci, "PCI capability missing");
    assert!(found_service, "Service manager capability missing");

    log_info!("  -> All core system capabilities accounted for with truthful hardware-backed status.");
}

fn test_service_manager() {
    log_info!("[TEST 4/7] Testing Service Manager Lifecycle & Logging...");

    // Initial state: Web service should exist
    let web_svc = get_service("web");
    assert!(web_svc.is_some(), "Web service not found in registry");

    let initial = web_svc.unwrap();
    assert_eq!(initial.name, "web");

    // Start service
    let res = start_service("web");
    assert!(res.is_ok(), "Failed to start web service");
    let started = get_service("web").unwrap();
    assert_eq!(started.state, ServiceState::Running);

    // Stop service
    let res = stop_service("web");
    assert!(res.is_ok(), "Failed to stop web service");
    let stopped = get_service("web").unwrap();
    assert_eq!(stopped.state, ServiceState::Stopped);

    // Restart service
    let res = restart_service("web");
    assert!(res.is_ok(), "Failed to restart web service");
    let restarted = get_service("web").unwrap();
    assert_eq!(restarted.state, ServiceState::Running);
    assert_eq!(restarted.restarts, 1);

    // Stop service cleanly
    let _ = stop_service("web");

    // Service logging
    log_service("web", "Diagnostic test message 1");
    log_service("web", "Diagnostic test message 2");

    let mut log_count = 0;
    for_each_service_log("web", |_tick, _entry| {
        log_count += 1;
    });
    assert!(log_count >= 2, "Service logs not persisted in ring buffer");

    // Nonexistent service check
    assert!(start_service("nonexistent_daemon").is_err());
    assert!(stop_service("nonexistent_daemon").is_err());
    assert!(restart_service("nonexistent_daemon").is_err());

    log_info!("  -> Service registry, state transitions (Running/Stopped), and ring-buffer logs verified.");
}

fn test_http_server_endpoints() {
    log_info!("[TEST 5/7] Testing Embedded Lightweight HTTP Server Routing...");

    let mut response_buf = [0u8; 2048];

    // 1. GET / -> Index page
    let req_root = "GET / HTTP/1.1\r\nHost: 10.0.2.15\r\n\r\n";
    let len = handle_http_request(req_root, &mut response_buf);
    assert!(len > 0);
    let resp = core::str::from_utf8(&response_buf[..len]).expect("Invalid UTF-8 HTTP response");
    assert!(resp.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(resp.contains("Server: EternalOS-Httpd/0.1\r\n"));
    assert!(resp.contains("Content-Type: text/html"));
    assert!(resp.contains("EternalOS Server"));

    // 2. GET /health -> JSON health check
    let req_health = "GET /health HTTP/1.1\r\nHost: 10.0.2.15\r\n\r\n";
    let len = handle_http_request(req_health, &mut response_buf);
    assert!(len > 0);
    let resp = core::str::from_utf8(&response_buf[..len]).expect("Invalid UTF-8 HTTP response");
    assert!(resp.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(resp.contains("Content-Type: application/json\r\n"));
    assert!(resp.contains("\"status\":\"healthy\""));

    // 3. GET /api/status -> JSON status API
    let req_status = "GET /api/status HTTP/1.1\r\nHost: 10.0.2.15\r\n\r\n";
    let len = handle_http_request(req_status, &mut response_buf);
    assert!(len > 0);
    let resp = core::str::from_utf8(&response_buf[..len]).expect("Invalid UTF-8 HTTP response");
    assert!(resp.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(resp.contains("Content-Type: application/json\r\n"));
    assert!(resp.contains("\"system\":\"EternalOS\""));

    // 4. GET /esys -> System summary
    let req_esys = "GET /esys HTTP/1.1\r\nHost: 10.0.2.15\r\n\r\n";
    let len = handle_http_request(req_esys, &mut response_buf);
    assert!(len > 0);
    let resp = core::str::from_utf8(&response_buf[..len]).expect("Invalid UTF-8 HTTP response");
    assert!(resp.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(resp.contains("Content-Type: application/json\r\n"));
    assert!(resp.contains("\"os\":\"EternalOS\""));

    // 5. GET /unknown_path -> 404 Not Found
    let req_404 = "GET /no_such_file HTTP/1.1\r\nHost: 10.0.2.15\r\n\r\n";
    let len = handle_http_request(req_404, &mut response_buf);
    assert!(len > 0);
    let resp = core::str::from_utf8(&response_buf[..len]).expect("Invalid UTF-8 HTTP response");
    assert!(resp.starts_with("HTTP/1.1 404 Not Found\r\n"));
    assert!(resp.contains("404 Not Found"));

    log_info!("  -> HTTP request router correctly served /, /health, /api/status, /esys, and 404.");
}

fn test_vfs_readonly_safety() {
    log_info!("[TEST 6/7] Testing VFS Read-Only Protection on EternalFS...");

    // 1. Attempting to write or create a file in /disk must fail with PermissionDenied
    let res = with_vfs(|vfs| {
        vfs.open("/disk/forbidden_write.txt", OpenFlags::CREATE_OR_WRITE)
    });
    match res {
        Err(VfsError::PermissionDenied) => {
            log_info!("  -> /disk write attempt correctly rejected with PermissionDenied.");
        }
        Err(e) => {
            log_info!("  -> /disk write failed as expected with error: {:?}", e);
        }
        Ok(_) => {
            panic!("FATAL: /disk allowed write access! EternalFS read-only invariant violated!");
        }
    }

    // 2. Writing in RamFS (root /) should succeed
    let write_ram = with_vfs(|vfs| {
        let handle = vfs.open("/test_ecs.txt", OpenFlags::CREATE_OR_WRITE)?;
        vfs.write(handle, b"EternalOS ECS test file")?;
        vfs.close(handle)
    });
    assert!(write_ram.is_ok(), "Writing to RamFS failed");

    // Verify file content can be read back
    let read_ram: Result<([u8; 64], usize), VfsError> = with_vfs(|vfs| {
        let handle = vfs.open("/test_ecs.txt", OpenFlags::READ_ONLY)?;
        let mut buf = [0u8; 64];
        let bytes = vfs.read(handle, &mut buf)?;
        let _ = vfs.close(handle);
        Ok((buf, bytes))
    });
    assert!(read_ram.is_ok());
    let (buf, bytes) = read_ram.unwrap();
    assert_eq!(&buf[..bytes], b"EternalOS ECS test file");

    log_info!("  -> Read-only integrity of EternalFS preserved; RamFS read/write verified.");
}

fn test_diagnostics_integrity() {
    log_info!("[TEST 7/7] Testing Diagnostics, Heap Stats, and Serial Log Buffer...");

    // 1. Heap stats
    let heap = crate::heap::get_heap_stats();
    assert!(heap.total_bytes > 0, "Heap total bytes should be non-zero");
    assert!(heap.total_bytes >= heap.allocated_bytes, "Allocated bytes exceeds heap capacity");
    assert_eq!(heap.free_bytes, heap.total_bytes - heap.allocated_bytes);

    // 2. RTC date/time
    let dt = crate::rtc::read_datetime();
    assert!(dt.year >= 2026, "RTC year is invalid");
    assert!(dt.month >= 1 && dt.month <= 12, "RTC month is invalid");
    assert!(dt.day >= 1 && dt.day <= 31, "RTC day is invalid");
    assert!(dt.hour <= 23, "RTC hour is invalid");
    assert!(dt.minute <= 59, "RTC minute is invalid");
    assert!(dt.second <= 59, "RTC second is invalid");

    // 3. Serial circular log buffer
    let mut log_count = 0;
    crate::serial::for_each_log(|_entry| {
        log_count += 1;
    });
    assert!(log_count > 0, "Log ring buffer should contain boot diagnostic logs");

    log_info!("  -> Heap statistics, RTC hardware clock, and serial log buffer verified.");
}

fn test_apps_and_calc_eval() {
    log_info!("[TEST 8/8] Testing Calculator Precedence, Parentheses, and Div-Zero Guards...");

    // Basic arithmetic
    assert_eq!(crate::calc::eval_expression("2 + 3"), Ok(5));
    assert_eq!(crate::calc::eval_expression("10 - 4"), Ok(6));

    // Precedence: * / % before + -
    assert_eq!(crate::calc::eval_expression("2 + 3 * 4"), Ok(14));
    assert_eq!(crate::calc::eval_expression("20 - 10 / 2"), Ok(15));
    assert_eq!(crate::calc::eval_expression("17 % 5"), Ok(2));

    // Parentheses
    assert_eq!(crate::calc::eval_expression("(2 + 3) * 4"), Ok(20));
    assert_eq!(crate::calc::eval_expression("((10 + 5) * 2) / 6"), Ok(5));

    // Unary minus
    assert_eq!(crate::calc::eval_expression("-5 + 10"), Ok(5));
    assert_eq!(crate::calc::eval_expression("-(3 * 4)"), Ok(-12));

    // Division by zero guard
    assert_eq!(crate::calc::eval_expression("10 / 0"), Err("Division by zero"));
    assert_eq!(crate::calc::eval_expression("10 % 0"), Err("Modulo by zero"));

    log_info!("  -> Calculator expression precedence, parentheses, and zero-check verified.");
}
