# Rust Timeout Implementation - Improvement Opportunities

## Executive Summary

This report identifies potential improvements to the Rust timeout implementation, comparing it against the GNU C version and leveraging modern Rust language features. The analysis is organized into three categories: **Critical**, **Recommended**, and **Nice-to-Have**.

---

## Critical Improvements

### 1. **Replace Polling with Signal-Based Child Monitoring**

**Current Implementation:**
```rust
async fn wait_for_child(pid: Pid) {
    loop {
        match waitpid(pid, Some(WaitPidFlag::WNOHANG)) {
            Ok(WaitStatus::StillAlive) => {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            _ => break,
        }
    }
}
```

**Problem:** 
- Wastes CPU cycles polling every 10ms
- Adds ~10ms average latency to child exit detection
- Not scalable for monitoring multiple processes

**Modern Rust Solution:**
Use `tokio::signal::unix::signal(SignalKind::child())` to get async SIGCHLD notifications:

```rust
async fn wait_for_child_async(pid: Pid) -> Result<WaitStatus, Error> {
    let mut sigchld = signal(SignalKind::child())?;
    
    loop {
        // First check if child already exited (non-blocking)
        match waitpid(pid, Some(WaitPidFlag::WNOHANG)) {
            Ok(WaitStatus::StillAlive) => {
                // Wait for SIGCHLD signal
                sigchld.recv().await;
                // Loop back to check which child exited
            }
            other => return Ok(other?),
        }
    }
}
```

**Benefits:**
- Zero CPU usage while waiting (event-driven)
- Instant notification when child exits (no polling delay)
- Follows GNU timeout's signal-based approach
- Truly async - no busy waiting

**Impact:** HIGH - Eliminates unnecessary CPU usage and improves responsiveness

---

### 2. **Use signalfd or pidfd for Race-Free Signal Handling**

**Current Issue:** 
Signals can be lost or delayed in high-frequency scenarios. Modern Linux provides better primitives.

**Modern Linux Approach (Linux 5.3+):**
```rust
use nix::sys::signalfd::{SignalFd, SigSet};
use nix::sys::signal::Signal;

// Block signals and handle them through signalfd
let mut mask = SigSet::empty();
mask.add(Signal::SIGCHLD);
mask.add(Signal::SIGINT);
mask.add(Signal::SIGTERM);
mask.thread_block()?;

let mut sfd = SignalFd::new(&mask)?;

// Can now poll signalfd alongside other events
tokio::select! {
    _ = async { sfd.read_signal().await } => { /* handle signal */ }
    _ = timeout_future => { /* timeout logic */ }
}
```

**Alternative - pidfd (Linux 5.4+):**
```rust
use std::os::unix::io::AsRawFd;

// Create pidfd for the child process
let pidfd = unsafe { 
    libc::syscall(libc::SYS_pidfd_open, child_pid, 0) 
};

// Can now poll pidfd directly - notified when process exits
// No SIGCHLD handling needed!
```

**Benefits:**
- No signal loss in race conditions
- Can integrate with tokio's AsyncFd
- More reliable than traditional signal handling
- Matches modern Linux best practices

**Impact:** MEDIUM-HIGH - Improves reliability in edge cases

---

### 3. **Add PR_SET_PDEATHSIG for Orphan Prevention**

**Current Gap:**
If timeout itself crashes or is killed unexpectedly, the child process becomes an orphan and continues running indefinitely.

**Solution:**
```rust
use nix::libc::{prctl, PR_SET_PDEATHSIG};

// In child process after fork, BEFORE exec
unsafe {
    // Child will receive SIGKILL if timeout (parent) dies
    prctl(PR_SET_PDEATHSIG, Signal::SIGKILL as i32);
    
    // Race condition protection: check parent still alive
    if getppid() != parent_pid_before_fork {
        // Parent already died before prctl took effect
        exit(1);
    }
}

// Now safe to exec
Command::new(command).args(args).exec();
```

**Benefits:**
- Prevents runaway processes if timeout crashes
- Matches systemd and container runtime best practices
- Defense-in-depth security measure
- No overhead when timeout works normally

**Impact:** MEDIUM - Important for production reliability

---

## Recommended Improvements

### 4. **Optimize Exit Code Handling with match Guards**

**Current:**
```rust
match waitpid(child_pid, None) {
    Ok(WaitStatus::Exited(_, code)) => {
        if preserve_status { code } else { EXIT_TIMEDOUT }
    }
    Ok(WaitStatus::Signaled(_, sig, _)) => {
        if preserve_status { 128 + sig as i32 } else { EXIT_TIMEDOUT }
    }
    _ => EXIT_TIMEDOUT,
}
```

**Better Rust Idiom:**
```rust
match (waitpid(child_pid, None), preserve_status) {
    (Ok(WaitStatus::Exited(_, code)), true) => code,
    (Ok(WaitStatus::Signaled(_, sig, _)), true) => 128 + sig as i32,
    (Ok(WaitStatus::Exited(_, _)), false) => EXIT_TIMEDOUT,
    (Ok(WaitStatus::Signaled(_, _, _)), false) => EXIT_TIMEDOUT,
    _ => EXIT_CANCELED,
}
```

**Benefits:**
- More explicit about all cases
- Compiler enforces exhaustiveness
- Easier to verify correctness

**Impact:** LOW - Code quality improvement

---

### 5. **Use Type-Safe Signal Wrapper**

**Current:**
Multiple places manually convert signals to strings and handle signal numbers.

**Better:**
```rust
#[derive(Debug, Clone, Copy)]
struct TimeoutSignal(Signal);

impl TimeoutSignal {
    fn as_str(&self) -> &'static str {
        match self.0 {
            Signal::SIGTERM => "SIGTERM",
            Signal::SIGKILL => "SIGKILL",
            // ... all signals
        }
    }
    
    fn from_str_or_num(s: &str) -> Result<Self, String> {
        // Parsing logic here
    }
    
    fn send_to_process(&self, pid: Pid) -> Result<(), Error> {
        kill(pid, self.0).map_err(|e| e.into())
    }
    
    fn send_to_group(&self, pgid: Pid) -> Result<(), Error> {
        killpg(pgid, self.0).map_err(|e| e.into())
    }
}
```

**Benefits:**
- Single source of truth for signal operations
- Type safety prevents invalid signal values
- Cleaner API

**Impact:** MEDIUM - Better maintainability

---

### 6. **Structured Error Handling with thiserror**

**Current:**
```rust
return Err(format!("Failed to fork: {}", e));
```

**Better with thiserror:**
```rust
use thiserror::Error;

#[derive(Error, Debug)]
enum TimeoutError {
    #[error("Failed to fork: {0}")]
    ForkFailed(#[from] nix::Error),
    
    #[error("Failed to execute command '{cmd}': {source}")]
    ExecFailed {
        cmd: String,
        source: std::io::Error,
    },
    
    #[error("Invalid duration '{0}': {1}")]
    InvalidDuration(String, String),
    
    #[error("Unknown signal: {0}")]
    UnknownSignal(String),
}
```

**Benefits:**
- Structured error types
- Better error reporting
- Easier to handle different error cases programmatically
- Standard Rust practice

**Impact:** MEDIUM - Improved error UX

---

### 7. **Add Metrics and Observability**

**Enhancement:**
```rust
struct TimeoutMetrics {
    command: String,
    duration: Duration,
    timed_out: bool,
    exit_code: i32,
    signal_sent: Option<Signal>,
    elapsed: Duration,
}

impl TimeoutMetrics {
    fn log(&self) {
        if std::env::var("TIMEOUT_METRICS").is_ok() {
            eprintln!("{{\"command\":\"{}\",\"timed_out\":{},\"elapsed_ms\":{},\"exit_code\":{}}}",
                self.command, self.timed_out, 
                self.elapsed.as_millis(), self.exit_code);
        }
    }
}
```

**Benefits:**
- Optional JSON output for monitoring
- No overhead when disabled
- Useful for CI/CD integration
- Can track timeout patterns

**Impact:** LOW-MEDIUM - DevOps improvement

---

## Nice-to-Have Improvements

### 8. **Add WUNTRACED Support for Stopped Process Detection**

**Enhancement:**
```rust
// Detect when child is stopped (SIGSTOP), not just terminated
match waitpid(pid, Some(WaitPidFlag::WNOHANG | WaitPidFlag::WUNTRACED)) {
    Ok(WaitStatus::Stopped(_, sig)) => {
        if verbose {
            eprintln!("timeout: child stopped by signal {}", sig);
        }
        // Could send SIGCONT here if needed
    }
    // ... other cases
}
```

**Benefits:**
- Better debugging visibility
- Can detect and handle stopped processes
- Matches some GNU timeout behavior in edge cases

**Impact:** LOW - Niche use case

---

### 9. **Add CPU and Memory Limit Integration**

**GNU timeout doesn't do this, but modern use cases need it:**

```rust
use nix::sys::resource::{setrlimit, Resource};

fn set_resource_limits(cpu_secs: Option<u64>, mem_bytes: Option<u64>) -> Result<(), Error> {
    if let Some(cpu) = cpu_secs {
        setrlimit(Resource::RLIMIT_CPU, cpu, cpu)?;
    }
    if let Some(mem) = mem_bytes {
        setrlimit(Resource::RLIMIT_AS, mem, mem)?;
    }
    Ok(())
}
```

**Usage:**
```bash
timeout --cpu-limit 60 --mem-limit 1G ./resource_hog
```

**Benefits:**
- Beyond time limits - resource limits too
- Common requirement in testing and CI
- Rust implementation can be more featureful than GNU version

**Impact:** LOW - Feature enhancement beyond GNU timeout

---

### 10. **Async-Aware Process Spawning with tokio::process**

**Investigation Needed:**
Could hybrid approach work? Use tokio::process::Command for spawning, then get the child PID for our manual management:

```rust
let mut child = tokio::process::Command::new(command)
    .args(args)
    .stdin(Stdio::inherit())
    .stdout(Stdio::inherit())
    .stderr(Stdio::inherit())
    .spawn()?;

let pid = Pid::from_raw(child.id().unwrap() as i32);

// Now use our manual waitpid logic with the pid
// But also leverage tokio::process's async .wait()
tokio::select! {
    status = child.wait() => { /* use tokio's result */ }
    _ = timeout_future => { /* our timeout logic */ }
}
```

**Consideration:**
- tokio::process might interfere with process group management
- Need to verify it doesn't set up conflicting SIGCHLD handlers
- Could simplify code if compatible

**Impact:** LOW - Needs careful evaluation

---

### 11. **Add Config File Support**

**Beyond GNU timeout:**

```toml
# ~/.config/timeout/config.toml
[defaults]
verbose = true
kill_after = "5s"

[profiles.database]
signal = "SIGTERM"
kill_after = "30s"
verbose = true

[profiles.quick]
kill_after = "1s"
```

Usage:
```bash
timeout --profile database 60s pg_dump mydb
```

**Impact:** LOW - Nice for power users

---

### 12. **Better Test Coverage**

**Add:**
- Property-based testing with proptest
- Concurrent test execution
- Fuzzing with cargo-fuzz
- Integration tests in Docker

```rust
#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    
    proptest! {
        #[test]
        fn duration_parsing_never_panics(s in "\\PC*") {
            let _ = parse_duration(&s);
        }
    }
}
```

**Impact:** MEDIUM - Quality assurance

---

## Summary Table

| # | Improvement | Benefit | Effort | Impact | Priority |
|---|------------|---------|--------|--------|----------|
| 1 | Signal-based child wait | No polling, instant response | Medium | High | **Critical** |
| 2 | signalfd/pidfd integration | Race-free signals | Medium | Med-High | **Critical** |
| 3 | PR_SET_PDEATHSIG | Orphan prevention | Low | Medium | **Critical** |
| 4 | Match guard optimization | Code clarity | Low | Low | Recommended |
| 5 | Type-safe signal wrapper | Maintainability | Medium | Medium | Recommended |
| 6 | Structured errors (thiserror) | Better UX | Low | Medium | Recommended |
| 7 | Metrics/observability | DevOps integration | Low | Low-Med | Recommended |
| 8 | WUNTRACED support | Edge case handling | Low | Low | Nice-to-have |
| 9 | Resource limits | Feature enhancement | Medium | Low | Nice-to-have |
| 10 | tokio::process hybrid | Code simplification | High | Low | Nice-to-have |
| 11 | Config file support | Power user feature | Medium | Low | Nice-to-have |
| 12 | Better test coverage | Quality assurance | High | Medium | Nice-to-have |

---

## Recommendation Priority

**Immediate (Next Iteration):**
1. Signal-based child monitoring (#1) - Eliminates polling overhead
2. PR_SET_PDEATHSIG (#3) - Critical for production reliability
3. Structured error handling (#6) - Low effort, high return

**Short Term:**
4. signalfd/pidfd (#2) - If targeting modern Linux only
5. Type-safe signal wrapper (#5) - Improves maintainability
6. Metrics (#7) - Useful for production deployments

**Long Term:**
7. Better testing (#12) - Continuous improvement
8. Resource limits (#9) - If expanding beyond GNU timeout scope
9. Rest are optional enhancements

---

## Conclusion

The current Rust implementation is **production-ready and feature-complete** compared to GNU timeout. The improvements listed here are optimizations and enhancements that leverage:

- Modern Linux kernel features (pidfd, signalfd, PR_SET_PDEATHSIG)
- Rust's type system and error handling
- Tokio's async ecosystem
- Production observability needs

**Key Takeaway:** The implementation is already excellent. These improvements would make it exceptional, with better performance (eliminate polling), better reliability (orphan prevention), and better maintainability (type safety, structured errors).
