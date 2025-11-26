// src/main.rs
// Main entry point and shared utilities for timeout command

mod args;
mod platform;

use args::Args;
use clap::{CommandFactory, Parser};
use clap_complete::{generate, Shell};
use owo_colors::OwoColorize;
use std::fmt;
use std::io;
use std::process::exit;
use std::time::Duration;
use thiserror::Error;

#[cfg(unix)]
use nix::sys::signal::{kill, killpg, Signal};
#[cfg(unix)]
use nix::unistd::Pid;

/// Custom error types for timeout operations
#[derive(Error, Debug)]
pub enum TimeoutError {
    #[cfg(unix)]
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

    #[cfg(unix)]
    #[error("failed to create process group: {0}")]
    ProcessGroupFailed(nix::Error),

    #[cfg(unix)]
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
    pub const IS_WINDOWS: bool = cfg!(windows);

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
        } else if Self::IS_WINDOWS {
            "Windows"
        } else {
            "Unknown"
        }
    }
}

/// Type-safe signal wrapper (Unix only)
#[cfg(unix)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeoutSignal(pub Signal);

#[cfg(unix)]
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
        // Try killpg first (process group signal)
        match killpg(pgid, self.0) {
            Ok(()) => Ok(()),
            Err(nix::errno::Errno::ESRCH) => {
                // On macOS, killpg may fail with ESRCH even when the process exists
                // Fall back to killing the process directly
                kill(pgid, self.0).map_err(|e| TimeoutError::SignalSendFailed {
                    signal: self.as_str().to_string(),
                    source: e,
                })
            }
            Err(e) => Err(TimeoutError::SignalSendFailed {
                signal: self.as_str().to_string(),
                source: e,
            }),
        }
    }
}

#[cfg(unix)]
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
    #[cfg(unix)]
    pub signal_sent: Option<TimeoutSignal>,
    #[cfg(not(unix))]
    pub signal_sent: Option<String>,
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
            #[cfg(unix)]
            let signal_str = self.signal_sent.map(|s| s.as_str()).unwrap_or("none");
            #[cfg(not(unix))]
            let signal_str = self.signal_sent.as_deref().unwrap_or("none");

            eprintln!(
                r#"{{"command":"{}","duration_ms":{},"timed_out":{},"exit_code":{},"signal":"{}","elapsed_ms":{},"kill_after_used":{},"cpu_limit":{},"memory_limit":{},"stopped_detected":{},"platform":"{}"}}"#,
                self.command.replace('"', "\\\""),
                self.duration.as_millis(),
                self.timed_out,
                self.exit_code,
                signal_str,
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

const EXIT_CANCELED: i32 = 125;

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

    // Handle shell completion generation
    if let Some(shell_name) = &args.generate_completions {
        let shell = match shell_name.to_lowercase().as_str() {
            "bash" => Shell::Bash,
            "zsh" => Shell::Zsh,
            "fish" => Shell::Fish,
            "powershell" => Shell::PowerShell,
            "elvish" => Shell::Elvish,
            _ => {
                eprintln!("{}: unknown shell '{}'", "Error".red(), shell_name);
                eprintln!("Supported shells: bash, zsh, fish, powershell, elvish");
                exit(EXIT_CANCELED);
            }
        };

        let mut cmd = Args::command();
        generate(shell, &mut cmd, "timeout", &mut io::stdout());
        return;
    }

    // Unwrap required fields (they're required when not generating completions)
    let duration_str = args.duration.as_ref().expect("duration is required");
    let command = args.command.as_ref().expect("command is required");

    // Show platform-specific warnings
    if !Platform::IS_LINUX {
        if args.cpu_limit().is_some() || args.mem_limit().is_some() {
            eprintln!(
                "{}: Running on {}. Some features may have limited support.",
                "Warning".yellow(),
                Platform::name()
            );

            #[cfg(not(any(target_os = "linux", target_os = "freebsd", target_os = "dragonfly")))]
            {
                eprintln!(
                    "{}: Resource limits (--cpu-limit, --mem-limit) not supported on this platform",
                    "Warning".yellow()
                );
                if args.cpu_limit().is_some() || args.mem_limit().is_some() {
                    eprintln!(
                        "{}: Resource limits requested but not available on {}",
                        "Error".red(),
                        Platform::name()
                    );
                    exit(EXIT_CANCELED);
                }
            }
        }
    }

    let duration = match parse_duration(duration_str) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{}: {}", "timeout".red(), e);
            exit(EXIT_CANCELED);
        }
    };

    #[cfg(unix)]
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

    #[cfg(not(unix))]
    if args.signal.is_some() {
        eprintln!(
            "Warning: --signal option not supported on {}",
            Platform::name()
        );
    }

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

    let mem_limit = if let Some(mem) = &args.mem_limit() {
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

    #[cfg(unix)]
    let result = platform::run_with_timeout(
        command,
        &args.args,
        duration,
        term_signal,
        kill_after_duration,
        args.foreground(),
        args.preserve_status,
        args.verbose,
        args.detect_stopped(),
        args.no_notify(),
        args.status_on_timeout,
        args.cpu_limit(),
        mem_limit,
    )
    .await;

    #[cfg(windows)]
    let result = platform::run_with_timeout(
        command,
        &args.args,
        duration,
        kill_after_duration,
        args.preserve_status,
        args.verbose,
        args.status_on_timeout,
    )
    .await;

    #[cfg(not(any(unix, windows)))]
    let result = {
        eprintln!("{}: Platform not supported", "Error".red());
        Err(TimeoutError::FeatureNotSupported(format!(
            "Platform {} not supported",
            Platform::name()
        )))
    };

    match result {
        Ok(code) => exit(code),
        Err(e) => {
            eprintln!("{}: {}", "timeout".red(), e);
            exit(EXIT_CANCELED);
        }
    }
}
