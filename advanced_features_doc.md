# Advanced Features - Deep Dive

This document explains the advanced GNU timeout features that have been fully implemented in the Rust version.

**Note:** Most advanced features described in this document are Unix-specific. Windows support provides core timeout functionality but lacks Unix signal and process group equivalents. See the Platform Support documentation for Windows-specific behavior.

## 1. Process Group Management

### What is a Process Group?

A process group is a collection of one or more processes that can be signaled together as a unit. When timeout creates a new process group using `setpgid(0, 0)`, it ensures that all child processes and their descendants can be killed together when the timeout expires.

### Implementation Details

**Default Behavior (Without `--foreground`):**

```rust
// Create a new process group
setpgid(Pid::from_raw(0), Pid::from_raw(0))
```

This call creates a new process group with the following effects:

- Both the timeout process and its child are in the same new group
- All processes spawned by the child inherit this process group
- When timeout expires, `killpg(pid, signal)` is used to kill the entire process group, ensuring all descendants are terminated

**Foreground Mode (`--foreground`):**

```rust
// Stay in parent's process group (don't call setpgid)
// Only kill the direct child process
kill(child_pid, signal)
```

In foreground mode, the command can use the foreground TTY normally and receive signals directly from the terminal (like Ctrl-C). However, any children of the command will not be timed out.

### Why This Matters

**Pipeline Behavior:**

When you run:

```bash
timeout 5s bash -c "sleep 10 | cat"
```

- **Without `--foreground`**: Both `sleep` and `cat` are in the timeout's process group and will be killed
- **With `--foreground`**: Only the `bash` process is killed; `sleep` and `cat` may continue running

**Example from GNU Timeout:**

When timeout kills a process group with `kill(0, SIGTERM)`, it sends the signal to all processes in the group. The `0` as the PID argument specifies that the entire process group should be targeted.

## 2. Core Dump Control with `prctl`

### What is PR_SET_DUMPABLE?

`prctl(PR_SET_DUMPABLE, 0)` disables core dump generation for a process. This prevents the creation of core dumps when the process receives signals like SIGSEGV that would normally trigger a core dump.

### Why Disable Core Dumps?

1. **Security**: Core dumps may contain sensitive information such as passwords, user data (PAN, SSN), or encryption keys that an attacker might exploit.

2. **Disk Space**: Core dumps of memory-heavy processes may consume disk space equal to or greater than the process's memory footprint.

3. **Performance**: Generating core dumps for memory-heavy processes can waste system resources and delay the cleanup of memory.

### Implementation

```rust
// In parent (timeout process)
unsafe {
    prctl(PR_SET_DUMPABLE, 0);  // Disable core dumps for timeout
}

// In child (monitored command)
unsafe {
    prctl(PR_SET_DUMPABLE, 1);  // Re-enable for the child
}
```

**Why This Pattern?**

- The timeout process shouldn't create core dumps if it crashes (security)
- The monitored command should be able to create core dumps for debugging
- The dumpable attribute is inherited by child processes, so we must explicitly re-enable it after forking

### Side Effects

When `PR_SET_DUMPABLE` is set to 0, the ownership of files in the process's `/proc/[pid]` directory changes to root:root. Setting it back to 1 reverts the ownership to the process's real UID and GID.

## 3. TTY and Signal Handling

### TTY Process Groups

The controlling terminal has a foreground process group. Signals from the terminal (like Ctrl-C generating SIGINT) are sent to all processes in this foreground group.

### Implementation

```rust
// Save original TTY process group
let original_tty_pgrp = if foreground {
    std::fs::File::open("/dev/tty")
        .ok()
        .and_then(|tty| {
            use std::os::unix::io::AsRawFd;
            tcgetpgrp(tty.as_raw_fd()).ok()
        })
} else {
    None
};

// ... after timeout completes ...

// Restore original TTY process group
if let Some(tty_pgrp) = original_tty_pgrp {
    if let Ok(tty) = std::fs::File::open("/dev/tty") {
        use std::os::unix::io::AsRawFd;
        let _ = tcsetpgrp(tty.as_raw_fd(), tty_pgrp);
    }
}
```

### Signal Reset for Child Process

```rust
// Reset signals that might have been ignored
let _ = nix::sys::signal::signal(
    Signal::SIGTTIN,
    nix::sys::signal::SigHandler::SigDfl
);
let _ = nix::sys::signal::signal(
    Signal::SIGTTOU,
    nix::sys::signal::SigHandler::SigDfl
);
```

**Why?** SIGTTIN and SIGTTOU control background process access to the TTY. The child needs default handling to properly interact with the terminal.

## 4. SIGCONT with Process Groups

### The Problem

When you send SIGTERM to a stopped process, it won't receive the signal until it's continued. A process might be stopped by SIGSTOP or SIGTSTP.

### The Solution

```rust
// After sending termination signal to process group
if !foreground {
    let _ = killpg(child_pid, Signal::SIGCONT);
}
```

SIGCONT is not sent in foreground mode as it's generally not needed with foreground processes and can cause intermittent signal delivery issues with programs that are monitors themselves (like GDB).

### Why This Matters

Consider this scenario:

```bash
timeout 10s bash -c "sleep 5 & kill -STOP $!; wait"
```

Without SIGCONT:

1. The background `sleep` is stopped
2. Timeout expires and sends SIGTERM
3. The stopped process doesn't receive SIGTERM
4. Process hangs until manually killed

With SIGCONT:

1. The background `sleep` is stopped
2. Timeout expires and sends SIGTERM
3. Timeout also sends SIGCONT
4. Process wakes up, receives SIGTERM, and exits

## 5. Process Group vs Direct Process Kill

### Code Implementation

```rust
let kill_result = if foreground {
    // In foreground mode, only kill the child process
    kill(child_pid, term_signal)
} else {
    // In background mode, kill the entire process group
    killpg(child_pid, term_signal)
};
```

### Comparison

| Aspect            | `kill(pid, sig)` | `killpg(pgid, sig)`    |
| ----------------- | ---------------- | ---------------------- |
| Target            | Single process   | All processes in group |
| Usage             | Foreground mode  | Default mode           |
| Children affected | No               | Yes                    |
| Pipeline handling | Poor             | Excellent              |
| TTY interaction   | Full             | Limited                |

### Real-World Examples

**Example 1: Simple Command**

```bash
timeout 5s sleep 10
```

- Either method works fine (no children to worry about)

**Example 2: Pipeline**

```bash
timeout 5s sh -c "yes | head -1000000000"
```

- `killpg`: Kills both `yes` and `head` ✓
- `kill`: Only kills `sh`, leaving `yes` and `head` running ✗

**Example 3: Background Jobs**

```bash
timeout 5s sh -c "sleep 100 & sleep 100 & wait"
```

- `killpg`: Kills all three sleep processes ✓
- `kill`: Only kills the shell, orphaning the background sleeps ✗

## 6. Fork/Exec Model

### Why Fork Instead of Spawn?

The Rust standard library's `Command::spawn()` creates a child process but doesn't give us the fine-grained control needed for:

- Process group management
- Signal handling setup
- Core dump control
- TTY manipulation

### Implementation

```rust
match unsafe { fork() } {
    Ok(ForkResult::Parent { child }) => {
        // Parent continues as timeout monitor
        child_pid
    }
    Ok(ForkResult::Child) => {
        // Child process setup
        // ... reset signals, re-enable core dumps, etc.

        // Replace child process with the command
        Command::new(command).args(args).exec();

        // exec() never returns on success
        eprintln!("Failed to exec");
        exit(EXIT_CANNOT_INVOKE);
    }
    Err(e) => {
        return Err(format!("Failed to fork: {}", e));
    }
}
```

### Benefits

1. **Process Group Control**: We can call `setpgid()` before exec
2. **Signal Setup**: Reset signal handlers in child before exec
3. **Core Dump Control**: Precisely control PR_SET_DUMPABLE
4. **TTY Access**: Proper terminal handling for interactive commands

## 7. Exit Code Semantics

### Standard Exit Codes

```rust
const EXIT_TIMEDOUT: i32 = 124;    // Command timed out
const EXIT_CANCELED: i32 = 125;    // Timeout itself failed
const EXIT_CANNOT_INVOKE: i32 = 126; // Command found but can't exec
const EXIT_ENOENT: i32 = 127;      // Command not found
```

### Signal-Based Exit Codes

When a process is killed by a signal:

```rust
exit_code = 128 + signal_number
```

Examples:

- SIGTERM (15): 128 + 15 = 143
- SIGKILL (9): 128 + 9 = 137
- SIGINT (2): 128 + 2 = 130

### Preserve Status Flag

```rust
if preserve_status {
    code  // Return actual exit code
} else {
    EXIT_TIMEDOUT  // Return 124 to indicate timeout
}
```

The `--preserve-status` flag makes timeout exit with the same status as the command, even when the command times out. This allows scripts to distinguish between command failures and timeouts.

## 8. Async Architecture with Fork

### The Challenge

Traditional Unix process management (`fork`, `waitpid`) is synchronous, but we want async benefits:

- Concurrent timeout checking
- Signal handling
- Non-blocking waits

### The Solution

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

Combined with `tokio::select!`:

```rust
tokio::select! {
    _ = wait_for_child(child_pid) => {
        // Child exited naturally
    }
    _ = tokio::time::sleep(duration) => {
        // Timeout expired
    }
    _ = sigint.recv() => {
        // SIGINT received
    }
}
```

### Benefits

1. **Non-Blocking**: Uses `WNOHANG` flag with polling
2. **Concurrent Events**: Can handle multiple conditions simultaneously
3. **Clean Cancellation**: Tokio cancels unneeded branches automatically
4. **Composable**: Easy to add more conditions (kill-after, more signals)

## Testing Process Group Behavior

### Test 1: Simple Process

```bash
timeout 2s sleep 10
echo $?  # Should be 124
```

### Test 2: Process with Children

```bash
timeout 2s bash -c 'sleep 100 & sleep 100 & wait'
# After timeout, check if any sleep processes remain
ps aux | grep sleep
# Should be empty
```

### Test 3: Foreground vs Background

```bash
# Background mode - kills all
timeout 2s bash -c 'yes | head -100000000'
ps aux | grep -E '(yes|head)'  # Should be empty

# Foreground mode - may leave children
timeout --foreground 2s bash -c 'yes | head -100000000'
ps aux | grep -E '(yes|head)'  # Might find orphaned processes
```

### Test 4: Stopped Process

```bash
# Without timeout (manual test)
bash -c 'sleep 100 & PID=$!; kill -STOP $PID; echo "Stopped $PID"; sleep 2; kill $PID; wait'
# Process doesn't die when stopped

# With timeout (tests SIGCONT)
timeout 2s bash -c 'sleep 100 & PID=$!; kill -STOP $PID; wait'
# Should terminate properly after 2 seconds
```

## Summary of Improvements

| Feature            | Basic Version       | Advanced Version                  | Windows Support 🆕   |
| ------------------ | ------------------- | --------------------------------- | -------------------- |
| Process Groups     | Single process      | Full process group management     | ❌                   |
| Child Processes    | Not guaranteed kill | All descendants killed            | ⚠️ Direct child only |
| Core Dumps         | Default behavior    | Controlled with prctl             | N/A                  |
| TTY Handling       | Basic               | Full TTY process group management | Partial (Ctrl+C)     |
| Signal Propagation | Simple forward      | SIGCONT + group signals           | ❌                   |
| Stopped Processes  | May hang            | Properly handled with SIGCONT     | N/A                  |
| Pipeline Support   | Limited             | Full support                      | Limited              |
| Fork/Exec          | std::process        | Manual fork with full control     | tokio::process       |
| Custom Exit Codes  | 124 fixed           | Configurable via --status         | ✅                   |
| No-Notify Mode     | N/A                 | Skip initial signal               | ❌                   |

**Platform Notes:**

- **Unix (Linux/BSD/macOS)**: All advanced features fully implemented
- **Windows**: Core timeout functionality with process termination and Ctrl+C propagation
- Features marked ❌ are not available on Windows due to platform limitations
- Features marked ⚠️ have limited functionality on Windows

All Unix features match the GNU coreutils implementation exactly, providing production-ready behavior for complex scenarios like pipelines, background jobs, and interactive commands. Windows support provides essential timeout functionality adapted to the Windows process model.
