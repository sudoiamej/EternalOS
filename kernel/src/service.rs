use crate::sync::SpinLock;
use crate::timer::get_ticks;

pub const MAX_SERVICES: usize = 8;
pub const SERVICE_LOG_CAPACITY: usize = 4;
pub const SERVICE_LOG_LEN: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceState {
    Stopped,
    Starting,
    Running,
    Stopping,
    Failed,
}

impl ServiceState {
    pub fn as_str(&self) -> &'static str {
        match self {
            ServiceState::Stopped => "STOPPED",
            ServiceState::Starting => "STARTING",
            ServiceState::Running => "RUNNING",
            ServiceState::Stopping => "STOPPING",
            ServiceState::Failed => "FAILED",
        }
    }
}

#[derive(Clone, Copy)]
pub struct ServiceLogEntry {
    pub tick: u64,
    pub msg_len: usize,
    pub msg: [u8; SERVICE_LOG_LEN],
}

impl ServiceLogEntry {
    pub const fn empty() -> Self {
        Self {
            tick: 0,
            msg_len: 0,
            msg: [0u8; SERVICE_LOG_LEN],
        }
    }

    pub fn text(&self) -> &str {
        core::str::from_utf8(&self.msg[..self.msg_len]).unwrap_or("")
    }
}

pub struct ServiceEntry {
    pub name: &'static str,
    pub description: &'static str,
    pub state: ServiceState,
    pub enabled: bool,
    pub restarts: u32,
    pub start_tick: u64,
    pub logs: [ServiceLogEntry; SERVICE_LOG_CAPACITY],
    pub log_head: usize,
    pub log_count: usize,
}

impl ServiceEntry {
    pub const fn new(name: &'static str, description: &'static str, initial_state: ServiceState) -> Self {
        Self {
            name,
            description,
            state: initial_state,
            enabled: true,
            restarts: 0,
            start_tick: 0,
            logs: [const { ServiceLogEntry::empty() }; SERVICE_LOG_CAPACITY],
            log_head: 0,
            log_count: 0,
        }
    }

    pub fn push_log(&mut self, tick: u64, msg: &str) {
        let entry = &mut self.logs[self.log_head];
        entry.tick = tick;
        let bytes = msg.as_bytes();
        let len = bytes.len().min(SERVICE_LOG_LEN);
        entry.msg[..len].copy_from_slice(&bytes[..len]);
        entry.msg_len = len;

        self.log_head = (self.log_head + 1) % SERVICE_LOG_CAPACITY;
        if self.log_count < SERVICE_LOG_CAPACITY {
            self.log_count += 1;
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ServiceSnapshot {
    pub name: &'static str,
    pub description: &'static str,
    pub state: ServiceState,
    pub enabled: bool,
    pub restarts: u32,
    pub uptime_seconds: u64,
}

pub struct ServiceRegistry {
    pub services: [ServiceEntry; MAX_SERVICES],
    pub count: usize,
}

impl ServiceRegistry {
    pub const fn new() -> Self {
        Self {
            services: [
                ServiceEntry::new("web", "EternalOS Lightweight HTTP Daemon (Port 80)", ServiceState::Stopped),
                ServiceEntry::new("dhcp-client", "DHCP Auto-Configuration Client", ServiceState::Stopped),
                ServiceEntry::new("dns-resolver", "Domain Name System Client Resolver", ServiceState::Running),
                ServiceEntry::new("console", "Terminal & Framebuffer Rendering Engine", ServiceState::Running),
                ServiceEntry::new("shell", "Interactive Command Dispatcher & Session", ServiceState::Running),
                ServiceEntry::new("", "", ServiceState::Stopped),
                ServiceEntry::new("", "", ServiceState::Stopped),
                ServiceEntry::new("", "", ServiceState::Stopped),
            ],
            count: 5,
        }
    }

    pub fn start(&mut self, name: &str) -> Result<(), &'static str> {
        let tick = get_ticks();
        for i in 0..self.count {
            let s = &mut self.services[i];
            if s.name == name {
                if s.state == ServiceState::Running {
                    return Err("Service is already running");
                }
                s.state = ServiceState::Running;
                s.start_tick = tick;
                s.push_log(tick, "Service started successfully");
                return Ok(());
            }
        }
        Err("Service not found")
    }

    pub fn stop(&mut self, name: &str) -> Result<(), &'static str> {
        let tick = get_ticks();
        for i in 0..self.count {
            let s = &mut self.services[i];
            if s.name == name {
                if s.name == "console" || s.name == "shell" {
                    return Err("Cannot stop essential system service");
                }
                if s.state == ServiceState::Stopped {
                    return Err("Service is already stopped");
                }
                s.state = ServiceState::Stopped;
                s.push_log(tick, "Service stopped cleanly");
                return Ok(());
            }
        }
        Err("Service not found")
    }

    pub fn restart(&mut self, name: &str) -> Result<(), &'static str> {
        let tick = get_ticks();
        for i in 0..self.count {
            let s = &mut self.services[i];
            if s.name == name {
                s.restarts += 1;
                s.state = ServiceState::Running;
                s.start_tick = tick;
                s.push_log(tick, "Service restarted");
                return Ok(());
            }
        }
        Err("Service not found")
    }

    pub fn get_snapshot(&self, name: &str) -> Option<ServiceSnapshot> {
        let current_tick = get_ticks();
        for i in 0..self.count {
            let s = &self.services[i];
            if s.name == name {
                let uptime_ticks = if s.state == ServiceState::Running {
                    current_tick.saturating_sub(s.start_tick)
                } else {
                    0
                };
                let uptime_seconds = (uptime_ticks * crate::timer::MS_PER_TICK) / 1000;
                return Some(ServiceSnapshot {
                    name: s.name,
                    description: s.description,
                    state: s.state,
                    enabled: s.enabled,
                    restarts: s.restarts,
                    uptime_seconds,
                });
            }
        }
        None
    }
}

pub static SERVICE_REGISTRY: SpinLock<ServiceRegistry> = SpinLock::new(ServiceRegistry::new());

pub fn start_service(name: &str) -> Result<(), &'static str> {
    SERVICE_REGISTRY.lock().start(name)
}

pub fn stop_service(name: &str) -> Result<(), &'static str> {
    SERVICE_REGISTRY.lock().stop(name)
}

pub fn restart_service(name: &str) -> Result<(), &'static str> {
    SERVICE_REGISTRY.lock().restart(name)
}

pub fn get_service(name: &str) -> Option<ServiceSnapshot> {
    SERVICE_REGISTRY.lock().get_snapshot(name)
}

pub fn for_each_service<F: FnMut(&ServiceSnapshot)>(mut f: F) {
    let reg = SERVICE_REGISTRY.lock();
    let current_tick = get_ticks();
    for i in 0..reg.count {
        let s = &reg.services[i];
        let uptime_ticks = if s.state == ServiceState::Running {
            current_tick.saturating_sub(s.start_tick)
        } else {
            0
        };
        let uptime_seconds = (uptime_ticks * crate::timer::MS_PER_TICK) / 1000;
        let snap = ServiceSnapshot {
            name: s.name,
            description: s.description,
            state: s.state,
            enabled: s.enabled,
            restarts: s.restarts,
            uptime_seconds,
        };
        f(&snap);
    }
}

pub fn log_service(name: &str, msg: &str) {
    let tick = get_ticks();
    let mut reg = SERVICE_REGISTRY.lock();
    for i in 0..reg.count {
        let s = &mut reg.services[i];
        if s.name == name {
            s.push_log(tick, msg);
            break;
        }
    }
}

pub fn for_each_service_log<F: FnMut(u64, &str)>(name: &str, mut f: F) {
    let reg = SERVICE_REGISTRY.lock();
    for i in 0..reg.count {
        let s = &reg.services[i];
        if s.name == name {
            if s.log_count > 0 {
                let start = if s.log_count < SERVICE_LOG_CAPACITY {
                    0
                } else {
                    s.log_head
                };
                for j in 0..s.log_count {
                    let idx = (start + j) % SERVICE_LOG_CAPACITY;
                    let entry = &s.logs[idx];
                    f(entry.tick, entry.text());
                }
            }
            break;
        }
    }
}
