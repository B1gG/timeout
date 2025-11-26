// src/platform/windows.rs
// Windows-specific timeout implementation using tokio async processes

use crate::{Platform, TimeoutError, TimeoutMetrics};
use owo_colors::OwoColorize;
use std::time::{Duration, Instant};
use tokio::process::Command as TokioCommand;

const EXIT_TIMEDOUT: i32 = 124;
const EXIT_CANCELED: i32 = 125;
const EXIT_CANNOT_INVOKE: i32 = 126;
const EXIT_ENOENT: i32 = 127;

#[allow(clippy::too_many_arguments)]
pub async fn run_with_timeout(
    command: &str,
    args: &[String],
    duration: Duration,
    kill_after: Option<Duration>,
    preserve_status: bool,
    verbose: bool,
    status_on_timeout: Option<i32>,
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
        cpu_limit: None,
        memory_limit: None,
        stopped_detected: false,
        platform: Platform::name(),
    };

    // Setup Ctrl+C handling for the timeout process itself
    #[cfg(windows)]
    let mut ctrl_c_stream =
        tokio::signal::windows::ctrl_c().map_err(|e| TimeoutError::SignalSetupFailed {
            signal: "Ctrl+C".to_string(),
            source: e,
        })?;

    // Spawn the child command
    let mut cmd = TokioCommand::new(command);
    cmd.args(args);

    let mut child = cmd.spawn().map_err(|e| {
        let exit_code = match e.kind() {
            std::io::ErrorKind::NotFound => EXIT_ENOENT,
            std::io::ErrorKind::PermissionDenied => EXIT_CANNOT_INVOKE,
            _ => EXIT_CANNOT_INVOKE,
        };
        eprintln!(
            "{}: failed to execute command '{}': {}",
            "Error".red(),
            command,
            e
        );
        TimeoutError::ExecFailed {
            cmd: command.to_string(),
            source: e,
        }
    })?;

    let child_pid = child.id();
    if verbose {
        if let Some(pid) = child_pid {
            eprintln!(
                "{}: Started command '{}' with PID {}.",
                "Info".cyan(),
                command,
                pid
            );
        }
    }

    // Main async timing loop
    let timeout_duration = duration;
    let kill_after_duration = kill_after.unwrap_or(Duration::ZERO);

    let mut initial_timeout_expired = false;
    let mut final_terminate_sent = false;

    loop {
        // Determine the next timeout based on current state
        let timeout_future = if !initial_timeout_expired {
            // Phase 1: Wait for the initial timeout duration
            tokio::time::sleep(timeout_duration)
        } else if !final_terminate_sent && !kill_after_duration.is_zero() {
            // Phase 2: Wait for the kill_after duration
            let kill_phase_end = start_time + timeout_duration + kill_after_duration;
            let remaining = kill_phase_end.saturating_duration_since(Instant::now());
            tokio::time::sleep(remaining)
        } else {
            // Wait briefly for process to exit after termination
            tokio::time::sleep(Duration::from_millis(100))
        };

        tokio::select! {
            _ = timeout_future => {
                if !initial_timeout_expired {
                    // Initial timeout has expired
                    if verbose {
                        eprintln!("{}: Initial timeout ({:?}) expired.", "Timeout".red(), timeout_duration);
                    }
                    initial_timeout_expired = true;
                    metrics.timed_out = true;
                    metrics.signal_sent = Some("TERMINATE".to_string());

                    if kill_after_duration.is_zero() {
                        // No grace period, terminate immediately
                        if verbose {
                            eprintln!("{}: Terminating process (no kill-after grace period).", "Info".cyan());
                        }
                        if let Err(e) = child.kill().await {
                            eprintln!("{}: Failed to terminate child process: {}", "Error".red(), e);
                        }
                        final_terminate_sent = true;
                    }
                    // If kill_after is non-zero, continue to next iteration
                } else if !final_terminate_sent {
                    // Kill-after duration has expired
                    if verbose {
                        eprintln!("{}: Kill-after duration ({:?}) expired. Sending final terminate.", "Kill".bright_red(), kill_after_duration);
                    }
                    metrics.kill_after_used = true;
                    if let Err(e) = child.kill().await {
                        eprintln!("{}: Failed to terminate child process: {}", "Error".red(), e);
                    }
                    final_terminate_sent = true;
                }
            }

            result = child.wait() => {
                match result {
                    Ok(status) => {
                        metrics.elapsed = start_time.elapsed();
                        let code = status.code().unwrap_or(EXIT_CANCELED);

                        if verbose {
                            eprintln!("{}: Child exited with code {}.", "Info".green(), code);
                        }

                        // Determine final exit code
                        metrics.exit_code = if metrics.timed_out {
                            if let Some(custom_status) = status_on_timeout {
                                custom_status
                            } else if preserve_status {
                                code
                            } else {
                                EXIT_TIMEDOUT
                            }
                        } else {
                            code
                        };

                        metrics.log();
                        return Ok(metrics.exit_code);
                    }
                    Err(e) => {
                        eprintln!("{}: Error waiting for child: {}", "Error".red(), e);
                        metrics.elapsed = start_time.elapsed();
                        metrics.exit_code = EXIT_CANCELED;
                        metrics.log();
                        return Ok(EXIT_CANCELED);
                    }
                }
            }

            #[cfg(windows)]
            _ = ctrl_c_stream.recv() => {
                if verbose {
                    eprintln!("{}: Received Ctrl+C for timeout process. Terminating child.", "Signal".yellow());
                }
                if let Err(e) = child.kill().await {
                    eprintln!("{}: Failed to terminate child process on Ctrl+C: {}", "Error".red(), e);
                }
                // Continue loop to wait for child exit
            }
        }
    }
}
