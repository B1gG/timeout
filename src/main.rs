use clap::Parser;
use nix::sys::signal::{kill, killpg, Signal};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::{fork, setpgid, ForkResult, Pid};
use std::fmt;
use std::os::unix::process::CommandExt;
use std::process::{exit, Command};
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::signal::unix::{signal, SignalKind};

// Platform-specific imports
#[cfg(target_os = "linux")]
use nix::libc::{prctl, PR_SET_DUMPABLE, PR_SET_PDEATHSIG};

#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "dragonfly"))]
use nix::libc::RLIM_INFINITY;

#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "dragonfly"))]
use nix::sys::resource::{setrlimit, Resource};

/// Custom error types for timeout operations
#[derive(Error, Debug)]
pub enum TimeoutError {
    #[error("failed to fork process: {0}")]
    ForkFailed(#[from] nix::Error),

    #[error("failed to execute command '{cmd}': {source}")]
    ExecFailed {
        cmd: String,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid duration '{input}': {reason}")]
    InvalidDuration { input: String, reason: String },

    #[error("invalid memory limit '{input}': {reason}")]
    InvalidMemoryLimit { input: String, reason: String },

    #[error("invalid CPU limit '{input}': {reason}")]
    InvalidCpuLimit { input: String, reason: String },

    #[error("unknown signal: {0}")]
    UnknownSignal(String),

    #[error("failed to setup signal handler for {signal}: {source}")]
    SignalSetupFailed {
        signal: String,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to create process group: {0}")]
    ProcessGroupFailed(nix::Error),

    #[error("failed to send signal {signal} to process: {source}")]
    SignalSendFailed {
        signal: String,
        #[source]
        source: nix::Error,
    },

    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "dragonfly"))]
    #[error("failed to set resource limit: {0}")]
    ResourceLimitFailed(nix::Error),

    #[error("command not found: {0}")]
    CommandNotFound(String),

    #[error("permission denied: {0}")]
    PermissionDenied(String),

    #[cfg(not(target_os = "linux"))]
    #[error("feature not supported on this platform: {0}")]
    FeatureNotSupported(String),
}

/// Platform detection helper
pub struct Platform;

impl Platform {
    pub const IS_LINUX: bool = cfg!(target_os = "linux");
    pub const IS_MACOS: bool = cfg!(target_os = "macos");
    pub const IS_FREEBSD: bool = cfg!(target_os = "freebsd");
    pub const IS_OPENBSD: bool = cfg!(target_os = "openbsd");
    pub const IS_NETBSD: bool = cfg!(target_os = "netbsd");
    pub const IS_DRAGONFLY: bool = cfg!(target_os = "dragonfly");

    pub const HAS_PRCTL: bool = cfg!(target_os = "linux");
    pub const HAS_RLIMIT_AS: bool = cfg!(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "dragonfly"
    ));

    pub fn name() -> &'static str {
        if Self::IS_LINUX {
            "Linux"
        } else if Self::IS_MACOS {
            "macOS"
        } else if Self::IS_FREEBSD {
            "FreeBSD"
        } else if Self::IS_OPENBSD {
            "OpenBSD"
        } else if Self::IS_NETBSD {
            "NetBSD"
        } else if Self::IS_DRAGONFLY {
            "DragonFly BSD"
        } else {
            "Unknown Unix"
        }
    }
}

/// Type-safe signal wrapper
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeoutSignal(Signal);

impl TimeoutSignal {
    pub fn from_str_or_num(s: &str) -> Result<Self, TimeoutError> {
        let sig = match s.to_uppercase().as_str() {
            "HUP" | "SIGHUP" | "1" => Signal::SIGHUP,
            "INT" | "SIGINT" | "2" => Signal::SIGINT,
            "QUIT" | "SIGQUIT" | "3" => Signal::SIGQUIT,
            "KILL" | "SIGKILL" | "9" => Signal::SIGKILL,
            "TERM" | "SIGTERM" | "15" => Signal::SIGTERM,
            "USR1" | "SIGUSR1" | "10" => Signal::SIGUSR1,
            "USR2" | "SIGUSR2" | "12" => Signal::SIGUSR2,
            "ALRM" | "SIGALRM" | "14" => Signal::SIGALRM,
            "CONT" | "SIGCONT" | "18" => Signal::SIGCONT,
            _ => return Err(TimeoutError::UnknownSignal(s.to_string())),
        };
        Ok(TimeoutSignal(sig))
    }

    pub fn as_signal(&self) -> Signal {
        self.0
    }

    pub fn as_str(&self) -> &'static str {
        match self.0 {
            Signal::SIGHUP => "SIGHUP",
            Signal::SIGINT => "SIGINT",
            Signal::SIGQUIT => "SIGQUIT",
            Signal::SIGKILL => "SIGKILL",
            Signal::SIGTERM => "SIGTERM",
            Signal::SIGUSR1 => "SIGUSR1",
            Signal::SIGUSR2 => "SIGUSR2",
            Signal::SIGALRM => "SIGALRM",
            Signal::SIGCONT => "SIGCONT",
            _ => "UNKNOWN",
        }
    }

    pub fn send_to_process(&self, pid: Pid) -> Result<(), TimeoutError> {
        kill(pid, self.0).map_err(|e| TimeoutError::SignalSendFailed {
            signal: self.as_str().to_string(),
            source: e,
        })
    }

    pub fn send_to_group(&self, pgid: Pid) -> Result<(), TimeoutError> {
        killpg(pgid, self.0).map_err(|e| TimeoutError::SignalSendFailed {
            signal: self.as_str().to_string(),
            source: e,
        })
    }
}

impl fmt::Display for TimeoutSignal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Timeout metrics for observability
#[derive(Debug, Clone)]
pub struct TimeoutMetrics {
    pub command: String,
    pub duration: Duration,
    pub timed_out: bool,
    pub exit_code: i32,
    pub signal_sent: Option<TimeoutSignal>,
    pub elapsed: Duration,
    pub kill_after_used: bool,
    pub cpu_limit: Option<u64>,
    pub memory_limit: Option<u64>,
    pub stopped_detected: bool,
    pub platform: &'static str,
}

impl TimeoutMetrics {
    pub fn log(&self) {
        if std::env::var("TIMEOUT_METRICS").is_ok() {
            eprintln!(
                r#"{{"command":"{}","duration_ms":{},"timed_out":{},"exit_code":{},"signal":"{}","elapsed_ms":{},"kill_after_used":{},"cpu_limit":{},"memory_limit":{},"stopped_detected":{},"platform":"{}"}}"#,
                self.command.replace('"', "\\\""),
                self.duration.as_millis(),
                self.timed_out,
                self.exit_code,
                self.signal_sent.map(|s| s.as_str()).unwrap_or("none"),
                self.elapsed.as_millis(),
                self.kill_after_used,
                self.cpu_limit
                    .map(|l| l.to_string())
                    .unwrap_or_else(|| "null".to_string()),
                self.memory_limit
                    .map(|l| l.to_string())
                    .unwrap_or_else(|| "null".to_string()),
                self.stopped_detected,
                self.platform
            );
        }
    }
}

/// Run a command with a time limit
#[derive(Parser, Debug)]
#[command(name = "timeout")]
#[command(version = "1.0")]
#[command(about = "Start COMMAND, and kill it if still running after DURATION", long_about = None)]
struct Args {
    /// Send this signal to COMMAND on timeout, rather than SIGTERM
    #[arg(short = 's', long = "signal", value_name = "SIGNAL")]
    signal: Option<String>,

    /// Also send SIGKILL if COMMAND is still running after DURATION
    #[arg(short = 'k', long = "kill-after", value_name = "DURATION")]
    kill_after: Option<String>,

    /// When not running timeout directly from a shell prompt,
    /// allow COMMAND to read from the TTY and get TTY signals
    #[arg(short = 'f', long = "foreground")]
    foreground: bool,

    /// Exit with the same status as COMMAND, even when the command times out
    #[arg(long = "preserve-status")]
    preserve_status: bool,

    /// Diagnose to stderr any signal sent upon timeout
    #[arg(short = 'v', long = "verbose")]
    verbose: bool,

    /// Detect and report when process is stopped (SIGSTOP, SIGTSTP, etc.)
    #[arg(long = "detect-stopped")]
    detect_stopped: bool,

    /// Limit CPU time in seconds (Linux/FreeBSD/DragonFly only)
    #[arg(long = "cpu-limit", value_name = "SECONDS")]
    cpu_limit: Option<u64>,

    /// Limit memory usage (Linux/FreeBSD/DragonFly only)
    /// Accepts values like "100M", "1G", "512K", or raw bytes
    #[arg(long = "mem-limit", value_name = "SIZE")]
    mem_limit: Option<String>,

    /// Duration before timeout (e.g., 10s, 5m, 2h, 1d)
    #[arg(value_name = "DURATION")]
    duration: String,

    /// Command to execute
    #[arg(value_name = "COMMAND")]
    command: String,

    /// Arguments for the command
    #[arg(value_name = "ARG", trailing_var_arg = true)]
    args: Vec<String>,
}

const EXIT_TIMEDOUT: i32 = 124;
const EXIT_CANCELED: i32 = 125;
const EXIT_CANNOT_INVOKE: i32 = 126;
const EXIT_ENOENT: i32 = 127;

fn parse_duration(input: &str) -> Result<Duration, TimeoutError> {
    let input = input.trim();

    if input == "0" {
        return Ok(Duration::from_secs(0));
    }

    let (value_str, multiplier) = if input
        .chars()
        .last()
        .map(|c| c.is_alphabetic())
        .unwrap_or(false)
    {
        let (val, suffix) = input.split_at(input.len() - 1);
        let mult = match suffix {
            "s" => 1,
            "m" => 60,
            "h" => 3600,
            "d" => 86400,
            _ => {
                return Err(TimeoutError::InvalidDuration {
                    input: input.to_string(),
                    reason: format!("invalid time suffix '{}'", suffix),
                })
            }
        };
        (val, mult)
    } else {
        (input, 1)
    };

    let value: f64 = value_str
        .parse()
        .map_err(|_| TimeoutError::InvalidDuration {
            input: input.to_string(),
            reason: format!("invalid numeric value '{}'", value_str),
        })?;

    if value < 0.0 {
        return Err(TimeoutError::InvalidDuration {
            input: input.to_string(),
            reason: "duration cannot be negative".to_string(),
        });
    }

    Ok(Duration::from_secs_f64(value * multiplier as f64))
}

fn parse_memory_limit(input: &str) -> Result<u64, TimeoutError> {
    let input = input.trim();

    let (value_str, multiplier) = if input
        .chars()
        .last()
        .map(|c| c.is_alphabetic())
        .unwrap_or(false)
    {
        let (val, suffix) = input.split_at(input.len() - 1);
        let mult = match suffix.to_uppercase().as_str() {
            "K" => 1024u64,
            "M" => 1024 * 1024,
            "G" => 1024 * 1024 * 1024,
            _ => {
                return Err(TimeoutError::InvalidMemoryLimit {
                    input: input.to_string(),
                    reason: format!("invalid size suffix '{}' (use K, M, or G)", suffix),
                })
            }
        };
        (val, mult)
    } else {
        (input, 1)
    };

    let value: u64 = value_str
        .parse()
        .map_err(|_| TimeoutError::InvalidMemoryLimit {
            input: input.to_string(),
            reason: format!("invalid numeric value '{}'", value_str),
        })?;

    Ok(value * multiplier)
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    // Show platform-specific warnings
    if !Platform::IS_LINUX {
        if args.cpu_limit.is_some() || args.mem_limit.is_some() {
            eprintln!(
                "Warning: Running on {}. Some features may have limited support.",
                Platform::name()
            );

            #[cfg(not(any(target_os = "linux", target_os = "freebsd", target_os = "dragonfly")))]
            {
                eprintln!("Warning: Resource limits (--cpu-limit, --mem-limit) not supported on this platform");
                if args.cpu_limit.is_some() || args.mem_limit.is_some() {
                    eprintln!(
                        "Error: Resource limits requested but not available on {}",
                        Platform::name()
                    );
                    exit(EXIT_CANCELED);
                }
            }
        }
    }

    let duration = match parse_duration(&args.duration) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("timeout: {}", e);
            exit(EXIT_CANCELED);
        }
    };

    let term_signal = if let Some(sig_str) = &args.signal {
        match TimeoutSignal::from_str_or_num(sig_str) {
            Ok(sig) => sig,
            Err(e) => {
                eprintln!("timeout: {}", e);
                exit(EXIT_CANCELED);
            }
        }
    } else {
        TimeoutSignal(Signal::SIGTERM)
    };

    let kill_after_duration = if let Some(ka) = &args.kill_after {
        match parse_duration(ka) {
            Ok(d) => Some(d),
            Err(e) => {
                eprintln!("timeout: {}", e);
                exit(EXIT_CANCELED);
            }
        }
    } else {
        None
    };

    let mem_limit = if let Some(mem) = &args.mem_limit {
        match parse_memory_limit(mem) {
            Ok(limit) => Some(limit),
            Err(e) => {
                eprintln!("timeout: {}", e);
                exit(EXIT_CANCELED);
            }
        }
    } else {
        None
    };

    match run_with_timeout(
        &args.command,
        &args.args,
        duration,
        term_signal,
        kill_after_duration,
        args.foreground,
        args.preserve_status,
        args.verbose,
        args.detect_stopped,
        args.cpu_limit,
        mem_limit,
    )
    .await
    {
        Ok(code) => exit(code),
        Err(e) => {
            eprintln!("timeout: {}", e);
            exit(EXIT_CANCELED);
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_with_timeout(
    command: &str,
    args: &[String],
    duration: Duration,
    term_signal: TimeoutSignal,
    kill_after: Option<Duration>,
    foreground: bool,
    preserve_status: bool,
    verbose: bool,
    detect_stopped: bool,
    cpu_limit: Option<u64>,
    mem_limit: Option<u64>,
) -> Result<i32, TimeoutError> {
    let start_time = Instant::now();
    let mut metrics = TimeoutMetrics {
        command: command.to_string(),
        duration,
        timed_out: false,
        exit_code: 0,
        signal_sent: None,
        elapsed: Duration::ZERO,
        kill_after_used: false,
        cpu_limit,
        memory_limit: mem_limit,
        stopped_detected: false,
        platform: Platform::name(),
    };

    // Linux-specific: Disable core dumps
    #[cfg(target_os = "linux")]
    unsafe {
        prctl(PR_SET_DUMPABLE, 0);
    }

    if !foreground {
        setpgid(Pid::from_raw(0), Pid::from_raw(0)).map_err(TimeoutError::ProcessGroupFailed)?;
    }

    let mut sigchld = signal(SignalKind::child()).map_err(|e| TimeoutError::SignalSetupFailed {
        signal: "SIGCHLD".to_string(),
        source: e,
    })?;

    let child_pid = match unsafe { fork() }? {
        ForkResult::Parent { child } => child,
        ForkResult::Child => {
            // === Child process setup ===

            // Linux-specific: Setup PR_SET_PDEATHSIG
            #[cfg(target_os = "linux")]
            {
                unsafe {
                    if prctl(PR_SET_PDEATHSIG, Signal::SIGKILL as i32) == -1 {
                        eprintln!("timeout: warning: failed to set parent death signal");
                    }
                }

                if getppid() != parent_pid_before_fork {
                    exit(1);
                }
            }

            // BSD/macOS: Warning about missing orphan prevention
            #[cfg(not(target_os = "linux"))]
            if verbose {
                eprintln!(
                    "timeout: note: orphan prevention (PR_SET_PDEATHSIG) not available on {}",
                    Platform::name()
                );
            }

            // Set resource limits (Linux/FreeBSD/DragonFly)
            #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "dragonfly"))]
            {
                if let Some(cpu_secs) = cpu_limit {
                    if let Err(e) = setrlimit(Resource::RLIMIT_CPU, cpu_secs, cpu_secs) {
                        eprintln!("timeout: warning: failed to set CPU limit: {}", e);
                    }
                }

                if let Some(mem_bytes) = mem_limit {
                    // On Linux, use RLIMIT_AS (virtual memory)
                    #[cfg(target_os = "linux")]
                    let resource = Resource::RLIMIT_AS;

                    // On BSD, RLIMIT_AS might not exist, use RLIMIT_DATA or RLIMIT_RSS
                    #[cfg(any(target_os = "freebsd", target_os = "dragonfly"))]
                    let resource = Resource::RLIMIT_DATA;

                    if let Err(e) = setrlimit(resource, mem_bytes, mem_bytes) {
                        eprintln!("timeout: warning: failed to set memory limit: {}", e);
                    }
                }
            }

            // macOS/OpenBSD/NetBSD: Warning about resource limits
            #[cfg(not(any(target_os = "linux", target_os = "freebsd", target_os = "dragonfly")))]
            {
                if cpu_limit.is_some() || mem_limit.is_some() {
                    eprintln!(
                        "timeout: warning: resource limits not fully supported on {}",
                        Platform::name()
                    );
                }
            }

            let _ = unsafe {
                nix::sys::signal::signal(Signal::SIGTTIN, nix::sys::signal::SigHandler::SigDfl)
            };
            let _ = unsafe {
                nix::sys::signal::signal(Signal::SIGTTOU, nix::sys::signal::SigHandler::SigDfl)
            };

            // Linux-specific: Re-enable core dumps
            #[cfg(target_os = "linux")]
            unsafe {
                prctl(PR_SET_DUMPABLE, 1);
            }

            let error = Command::new(command).args(args).exec();

            let exit_code = match error.kind() {
                std::io::ErrorKind::NotFound => EXIT_ENOENT,
                std::io::ErrorKind::PermissionDenied => EXIT_CANNOT_INVOKE,
                _ => EXIT_CANNOT_INVOKE,
            };

            eprintln!("timeout: failed to run command '{}': {}", command, error);
            exit(exit_code);
        }
    };

    // === Parent process ===

    let mut sigint =
        signal(SignalKind::interrupt()).map_err(|e| TimeoutError::SignalSetupFailed {
            signal: "SIGINT".to_string(),
            source: e,
        })?;

    let mut sigterm =
        signal(SignalKind::terminate()).map_err(|e| TimeoutError::SignalSetupFailed {
            signal: "SIGTERM".to_string(),
            source: e,
        })?;

    let mut wait_flags = WaitPidFlag::WNOHANG;
    if detect_stopped {
        wait_flags |= WaitPidFlag::WUNTRACED;
    }

    let exit_code = tokio::select! {
        _ = sigchld.recv() => {
            metrics.elapsed = start_time.elapsed();

            match waitpid(child_pid, Some(wait_flags)) {
                Ok(WaitStatus::Stopped(_, sig)) if detect_stopped => {
                    metrics.stopped_detected = true;
                    if verbose {
                        eprintln!("timeout: process stopped by signal {}", sig);
                    }

                    if !foreground {
                        let _ = TimeoutSignal(Signal::SIGCONT).send_to_group(child_pid);
                    } else {
                        let _ = TimeoutSignal(Signal::SIGCONT).send_to_process(child_pid);
                    }

                    match waitpid(child_pid, None) {
                        Ok(WaitStatus::Exited(_, code)) => {
                            metrics.exit_code = code;
                            metrics.log();
                            code
                        }
                        Ok(WaitStatus::Signaled(_, sig, _)) => {
                            let code = 128 + sig as i32;
                            metrics.exit_code = code;
                            metrics.log();
                            code
                        }
                        _ => EXIT_CANCELED,
                    }
                }
                Ok(WaitStatus::Exited(_, code)) => {
                    metrics.exit_code = code;
                    metrics.log();
                    code
                }
                Ok(WaitStatus::Signaled(_, sig, _)) => {
                    let code = 128 + sig as i32;
                    metrics.exit_code = code;
                    metrics.log();
                    code
                }
                Ok(WaitStatus::StillAlive) => {
                    match waitpid(child_pid, None) {
                        Ok(WaitStatus::Exited(_, code)) => {
                            metrics.exit_code = code;
                            metrics.log();
                            code
                        }
                        Ok(WaitStatus::Signaled(_, sig, _)) => {
                            let code = 128 + sig as i32;
                            metrics.exit_code = code;
                            metrics.log();
                            code
                        }
                        _ => EXIT_CANCELED,
                    }
                }
                _ => EXIT_CANCELED,
            }
        }

        _ = tokio::time::sleep(duration) => {
            metrics.timed_out = true;
            metrics.signal_sent = Some(term_signal);

            if verbose {
                eprintln!("timeout: sending signal {} to command '{}'", term_signal, command);
            }

            if foreground {
                term_signal.send_to_process(child_pid)?;
            } else {
                term_signal.send_to_group(child_pid)?;
            }

            if !foreground {
                let _ = TimeoutSignal(Signal::SIGCONT).send_to_group(child_pid);
            }

            if let Some(ka_duration) = kill_after {
                metrics.kill_after_used = true;

                tokio::select! {
                    _ = sigchld.recv() => {
                        metrics.elapsed = start_time.elapsed();

                        let code = match waitpid(child_pid, Some(WaitPidFlag::WNOHANG)) {
                            Ok(WaitStatus::Exited(_, c)) => {
                                if preserve_status { c } else { EXIT_TIMEDOUT }
                            }
                            Ok(WaitStatus::Signaled(_, sig, _)) => {
                                if preserve_status { 128 + sig as i32 } else { EXIT_TIMEDOUT }
                            }
                            _ => EXIT_TIMEDOUT,
                        };

                        metrics.exit_code = code;
                        metrics.log();
                        code
                    }

                    _ = tokio::time::sleep(ka_duration) => {
                        if verbose {
                            eprintln!("timeout: sending signal SIGKILL to command '{}'", command);
                        }

                        let kill_sig = TimeoutSignal(Signal::SIGKILL);
                        if foreground {
                            kill_sig.send_to_process(child_pid)?;
                        } else {
                            kill_sig.send_to_group(child_pid)?;
                        }

                        let _ = sigchld.recv().await;
                        metrics.elapsed = start_time.elapsed();
                        metrics.exit_code = 128 + 9;
                        metrics.log();

                        128 + 9
                    }
                }
            } else {
                let _ = sigchld.recv().await;
                metrics.elapsed = start_time.elapsed();

                let code = match waitpid(child_pid, None) {
                    Ok(WaitStatus::Exited(_, c)) => {
                        if preserve_status { c } else { EXIT_TIMEDOUT }
                    }
                    Ok(WaitStatus::Signaled(_, sig, _)) => {
                        if preserve_status { 128 + sig as i32 } else { EXIT_TIMEDOUT }
                    }
                    _ => EXIT_TIMEDOUT,
                };

                metrics.exit_code = code;
                metrics.log();
                code
            }
        }

        _ = sigint.recv() => {
            metrics.elapsed = start_time.elapsed();

            let sig = TimeoutSignal(Signal::SIGINT);
            if foreground {
                sig.send_to_process(child_pid)?;
            } else {
                sig.send_to_group(child_pid)?;
            }

            let _ = sigchld.recv().await;
            let code = match waitpid(child_pid, None) {
                Ok(WaitStatus::Exited(_, c)) => c,
                Ok(WaitStatus::Signaled(_, _, _)) => 128 + 2,
                _ => 128 + 2,
            };

            metrics.exit_code = code;
            metrics.signal_sent = Some(sig);
            metrics.log();
            code
        }

        _ = sigterm.recv() => {
            metrics.elapsed = start_time.elapsed();

            let sig = TimeoutSignal(Signal::SIGTERM);
            if foreground {
                sig.send_to_process(child_pid)?;
            } else {
                sig.send_to_group(child_pid)?;
            }

            let _ = sigchld.recv().await;
            let code = match waitpid(child_pid, None) {
                Ok(WaitStatus::Exited(_, c)) => c,
                Ok(WaitStatus::Signaled(_, _, _)) => 128 + 15,
                _ => 128 + 15,
            };

            metrics.exit_code = code;
            metrics.signal_sent = Some(sig);
            metrics.log();
            code
        }
    };

    Ok(exit_code)
}
