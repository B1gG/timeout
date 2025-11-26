// src/args.rs
// Command-line argument parsing

use clap::Parser;

/// Run a command with a time limit
#[derive(Parser, Debug)]
#[command(name = "timeout")]
#[command(version = "1.0")]
#[command(about = "Start COMMAND, and kill it if still running after DURATION", long_about = None)]
pub struct Args {
    /// Generate shell completions (bash, zsh, fish, powershell, elvish)
    #[arg(long = "generate-completions", value_name = "SHELL", hide = true)]
    pub generate_completions: Option<String>,

    /// Send this signal to COMMAND on timeout, rather than SIGTERM
    #[arg(short = 's', long = "signal", value_name = "SIGNAL")]
    pub signal: Option<String>,

    /// Also send SIGKILL if COMMAND is still running after DURATION (default unit: seconds)
    #[arg(short = 'k', long = "kill-after", value_name = "DURATION")]
    pub kill_after: Option<String>,

    /// When not running timeout directly from a shell prompt,
    /// allow COMMAND to read from the TTY and get TTY signals
    #[cfg(unix)]
    #[arg(short = 'f', long = "foreground")]
    pub foreground: bool,

    /// Exit with the same status as COMMAND, even when the command times out
    #[arg(long = "preserve-status")]
    pub preserve_status: bool,

    /// Diagnose to stderr any signal sent upon timeout
    #[arg(short = 'v', long = "verbose")]
    pub verbose: bool,

    /// Detect and report when process is stopped (SIGSTOP, SIGTSTP, etc.)
    #[cfg(unix)]
    #[arg(long = "detect-stopped")]
    pub detect_stopped: bool,

    /// Do not send the initial signal when timeout expires (send only kill signal)
    #[cfg(unix)]
    #[arg(long = "no-notify")]
    pub no_notify: bool,

    /// Exit with this status code on timeout instead of 124
    #[arg(long = "status", value_name = "STATUS")]
    pub status_on_timeout: Option<i32>,

    /// Limit CPU time in seconds (Linux/FreeBSD/DragonFly only)
    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "dragonfly"))]
    #[arg(long = "cpu-limit", value_name = "SECONDS")]
    pub cpu_limit: Option<u64>,

    /// Limit memory usage (Linux/FreeBSD/DragonFly only)
    /// Accepts values like "100M", "1G", "512K", or raw bytes
    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "dragonfly"))]
    #[arg(long = "mem-limit", value_name = "SIZE")]
    pub mem_limit: Option<String>,

    /// Duration before timeout (e.g., 10, 10s, 5m, 2h, 1d). If no unit, seconds are assumed.
    #[arg(
        value_name = "DURATION",
        required_unless_present = "generate_completions"
    )]
    pub duration: Option<String>,

    /// Command to execute
    #[arg(
        value_name = "COMMAND",
        required_unless_present = "generate_completions"
    )]
    pub command: Option<String>,

    /// Arguments for the command
    #[arg(
        value_name = "ARG",
        trailing_var_arg = true,
        allow_hyphen_values = true
    )]
    pub args: Vec<String>,
}

impl Args {
    /// Get foreground setting with default for non-Unix platforms
    #[cfg(not(unix))]
    pub fn foreground(&self) -> bool {
        false
    }

    #[cfg(unix)]
    pub fn foreground(&self) -> bool {
        self.foreground
    }

    /// Get detect_stopped setting with default for non-Unix platforms
    #[cfg(not(unix))]
    pub fn detect_stopped(&self) -> bool {
        false
    }

    #[cfg(unix)]
    pub fn detect_stopped(&self) -> bool {
        self.detect_stopped
    }

    /// Get no_notify setting with default for non-Unix platforms
    #[cfg(not(unix))]
    pub fn no_notify(&self) -> bool {
        false
    }

    #[cfg(unix)]
    pub fn no_notify(&self) -> bool {
        self.no_notify
    }

    /// Get CPU limit with default for unsupported platforms
    #[cfg(not(any(target_os = "linux", target_os = "freebsd", target_os = "dragonfly")))]
    pub fn cpu_limit(&self) -> Option<u64> {
        None
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "dragonfly"))]
    pub fn cpu_limit(&self) -> Option<u64> {
        self.cpu_limit
    }

    /// Get memory limit with default for unsupported platforms
    #[cfg(not(any(target_os = "linux", target_os = "freebsd", target_os = "dragonfly")))]
    pub fn mem_limit(&self) -> Option<String> {
        None
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "dragonfly"))]
    pub fn mem_limit(&self) -> Option<String> {
        self.mem_limit.clone()
    }
}
