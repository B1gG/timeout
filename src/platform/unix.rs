// src/platform/unix.rs
// Unix-specific timeout implementation using fork() and signals

use crate::{Platform, TimeoutError, TimeoutMetrics, TimeoutSignal};
use nix::sys::signal::Signal;
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::{fork, setpgid, ForkResult, Pid};
use owo_colors::OwoColorize;
use std::os::unix::process::CommandExt;
use std::process::{exit, Command};
use std::time::{Duration, Instant};
use tokio::signal::unix::{signal, SignalKind};

// Platform-specific imports
#[cfg(target_os = "linux")]
use nix::libc::{prctl, PR_SET_DUMPABLE, PR_SET_PDEATHSIG};

#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "dragonfly"))]
use nix::sys::resource::{setrlimit, Resource};

const EXIT_TIMEDOUT: i32 = 124;
const EXIT_CANCELED: i32 = 125;
const EXIT_CANNOT_INVOKE: i32 = 126;
const EXIT_ENOENT: i32 = 127;

/// Helper to determine exit code on timeout
fn timeout_exit_code(
    child_code: i32,
    preserve_status: bool,
    status_on_timeout: Option<i32>,
) -> i32 {
    if let Some(custom_status) = status_on_timeout {
        custom_status
    } else if preserve_status {
        child_code
    } else {
        EXIT_TIMEDOUT
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn run_with_timeout(
    command: &str,
    args: &[String],
    duration: Duration,
    term_signal: TimeoutSignal,
    kill_after: Option<Duration>,
    foreground: bool,
    preserve_status: bool,
    verbose: bool,
    detect_stopped: bool,
    no_notify: bool,
    status_on_timeout: Option<i32>,
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
                if unsafe { prctl(PR_SET_PDEATHSIG, Signal::SIGKILL as i32) } == -1 {
                    eprintln!("{}: failed to set parent death signal", "Warning".yellow());
                }
            }

            // BSD/macOS: Warning about missing orphan prevention
            #[cfg(not(target_os = "linux"))]
            if verbose {
                eprintln!(
                    "{}: orphan prevention (PR_SET_PDEATHSIG) not available on {}",
                    "Note".cyan(),
                    Platform::name()
                );
            }

            // Set resource limits (Linux/FreeBSD/DragonFly)
            #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "dragonfly"))]
            {
                if let Some(cpu_secs) = cpu_limit {
                    if let Err(e) = setrlimit(Resource::RLIMIT_CPU, cpu_secs, cpu_secs) {
                        eprintln!("{}: failed to set CPU limit: {}", "Warning".yellow(), e);
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
                        eprintln!("{}: failed to set memory limit: {}", "Warning".yellow(), e);
                    }
                }
            }

            // macOS/OpenBSD/NetBSD: Warning about resource limits
            #[cfg(not(any(target_os = "linux", target_os = "freebsd", target_os = "dragonfly")))]
            {
                if cpu_limit.is_some() || mem_limit.is_some() {
                    eprintln!(
                        "{}: resource limits not fully supported on {}",
                        "Warning".yellow(),
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

            // If we get here, exec failed
            eprintln!(
                "{}: failed to run command '{}': {}",
                "Error".red(),
                command,
                error
            );
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
                        eprintln!("{}: process stopped by signal {}", "Info".blue(), sig);
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

            // Send initial signal unless --no-notify is specified
            if !no_notify {
                metrics.signal_sent = Some(term_signal);

                if verbose {
                    eprintln!("{}: sending signal {} to command '{}'", "Timeout".red(), term_signal, command);
                }

                if foreground {
                    term_signal.send_to_process(child_pid)?;
                } else {
                    term_signal.send_to_group(child_pid)?;
                }

                if !foreground {
                    let _ = TimeoutSignal(Signal::SIGCONT).send_to_group(child_pid);
                }
            } else if verbose {
                eprintln!("{}: skipping initial signal (--no-notify), will send SIGKILL after grace period", "Info".cyan());
            }

            if let Some(ka_duration) = kill_after {
                metrics.kill_after_used = true;

                tokio::select! {
                    _ = sigchld.recv() => {
                        metrics.elapsed = start_time.elapsed();

                        let code = match waitpid(child_pid, Some(WaitPidFlag::WNOHANG)) {
                            Ok(WaitStatus::Exited(_, c)) => {
                                timeout_exit_code(c, preserve_status, status_on_timeout)
                            }
                            Ok(WaitStatus::Signaled(_, sig, _)) => {
                                timeout_exit_code(128 + sig as i32, preserve_status, status_on_timeout)
                            }
                            _ => status_on_timeout.unwrap_or(EXIT_TIMEDOUT),
                        };

                        metrics.exit_code = code;
                        metrics.log();
                        code
                    }

                    _ = tokio::time::sleep(ka_duration) => {
                        if verbose {
                            eprintln!("{}: sending signal SIGKILL to command '{}'", "Kill".bright_red(), command);
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
                        timeout_exit_code(c, preserve_status, status_on_timeout)
                    }
                    Ok(WaitStatus::Signaled(_, sig, _)) => {
                        timeout_exit_code(128 + sig as i32, preserve_status, status_on_timeout)
                    }
                    _ => status_on_timeout.unwrap_or(EXIT_TIMEDOUT),
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
