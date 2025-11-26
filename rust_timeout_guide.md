# Rust Timeout Implementation - Technical Guide

## Overview

This document provides a comprehensive guide to the Rust implementation of the GNU `timeout` command, including architecture decisions, implementation details, and comparisons with the original C implementation.

## Architecture Overview

### High-Level Design

```
┌─────────────────────────────────────────────────────────┐
│                    Main Program                          │
│  - Parse CLI arguments (clap)                           │
│  - Validate duration and signal                         │
│  - Setup async runtime (tokio)                          │
│  - Platform detection (Unix vs Windows) 🆕              │
└────────────────┬────────────────────────────────────────┘
                 │
    ┌────────────┴────────────┐
    │                         │
    v                         v
┌────────────┐          ┌─────────────┐
│ Unix Path  │          │Windows Path │ 🆕
│  (fork)    │          │  (tokio)    │
└─────┬──────┘          └──────┬──────┘
      │                        │
      └────────────┬───────────┘
                   v
┌─────────────────────────────────────────────────────────┐
│         Concurrent Event Loop (tokio::select!)          │
│                                                          │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐ │
│  │Child Process │  │   Timeout    │  │   Signals    │ │
│  │  Completion  │  │   Expiry     │  │ (Platform-   │ │
│  │              │  │              │  │  specific)   │ │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘ │
│         │                  │                  │          │
│         └──────────────────┴──────────────────┘         │
│                             │                            │
└─────────────────────────────┼────────────────────────────┘
                              │
                              v
                    ┌─────────────────┐
                    │ Handle Result   │
                    │ - Exit codes    │
                    │ - Signal child  │
                    │ - Kill after    │
                    └─────────────────┘
```

### Platform Architecture 🆕

The implementation uses conditional compilation for platform-specific code:

```
src/
├── main.rs           # Shared utilities, entry point
├── args.rs           # CLI parsing with #[cfg] guards
└── platform/
    ├── mod.rs        # Platform abstraction
    ├── unix.rs       # Unix implementation (fork, signals)
    └── windows.rs    # Windows implementation (async process)
```

**Unix Approach:**

- Fork-based process spawning
- Direct signal control (SIGTERM, SIGKILL, etc.)
- Process groups for orphan prevention
- Resource limits via `setrlimit`

**Windows Approach:**

- `tokio::process::Command` for async spawning
- Ctrl+C signal propagation
- Process termination via `child.kill()`
- No signal/resource limit equivalents

### Key Components

1. **Command Line Parser (clap)**

   - Declarative argument definition using derive macros
   - Automatic help generation
   - Type-safe argument parsing

2. **Async Runtime (tokio)**

   - Non-blocking I/O for process management
   - Concurrent event handling with `tokio::select!`
   - Timer-based timeout mechanism

3. **Signal Handling (nix + tokio::signal)**

   - Safe Unix signal operations
   - Signal forwarding to child process
   - Async-aware signal handlers

4. **Process Management (std::process + nix)**
   - Child process spawning
   - PID management
   - Exit status handling

## Implementation Details

### 1. Duration Parsing

The `parse_duration()` function handles flexible duration input:

```rust
fn parse_duration(input: &str) -> Result<Duration, String> {
    // Supports: 10, 10s, 5m, 2h, 1d
    // Also supports floating point: 0.5m = 30 seconds
}
```

**Features:**

- Default to seconds if no suffix
- Supports s/m/h/d suffixes
- Floating-point values allowed
- Validates non-negative values

**Comparison with C:**
The C implementation uses `xstrtod` and manual multiplier application. Our Rust version is more type-safe with Result types and clearer error handling.

### 2. Signal Parsing

```rust
fn parse_signal(signal_str: &str) -> Result<Signal, String> {
    // Supports: TERM, SIGTERM, 15, INT, SIGINT, 2, etc.
}
```

**Features:**

- Case-insensitive matching
- Supports signal names with/without "SIG" prefix
- Supports numeric signal values
- Uses nix::sys::signal::Signal enum for type safety

**Comparison with C:**
C implementation uses `operand2sig()` with manual string parsing. Rust version leverages pattern matching for cleaner code.

### 3. Core Timeout Logic

The `run_with_timeout()` function implements the main logic using `tokio::select!`:

```rust
tokio::select! {
    // Branch 1: Child completes normally
    status = child.wait() => { /* return exit code */ }

    // Branch 2: Timeout expires
    _ = tokio::time::sleep(duration) => {
        // Send termination signal
        // Handle kill_after if specified
    }

    // Branch 3: Forward SIGINT
    _ = sigint.recv() => { /* forward to child */ }

    // Branch 4: Forward SIGTERM
    _ = sigterm.recv() => { /* forward to child */ }
}
```

**Key advantages of tokio::select!:**

- Heterogeneous concurrent operations
- First-to-complete wins
- Automatic cancellation of other branches
- Type-safe and composable

**Comparison with C:**
C implementation uses:

- `fork()` for process creation
- Custom signal handlers with `sigaction()`
- `sigprocmask()` for signal blocking
- `waitpid()` with manual timeout checking

Rust's async approach is more ergonomic and safer.

### 4. Kill After Implementation

When `--kill-after` is specified:

```rust
// First, send termination signal
kill(pid, term_signal)?;

// Then wait with nested select
tokio::select! {
    // Child exits gracefully
    status = child.wait() => { /* return status */ }

    // Kill after timer expires
    _ = tokio::time::sleep(ka_duration) => {
        // Send SIGKILL
        kill(pid, Signal::SIGKILL)?;
        // Wait for forced termination
    }
}
```

**Comparison with C:**
C uses `alarm()` for the kill-after timer and a separate signal handler. Rust's approach is more structured and easier to reason about.

### 5. Exit Code Handling

```rust
const EXIT_TIMEDOUT: i32 = 124;
const EXIT_CANCELED: i32 = 125;
const EXIT_CANNOT_INVOKE: i32 = 126;
const EXIT_ENOENT: i32 = 127;
```

Matches GNU timeout exactly:

- **124**: Command timed out
- **125**: Internal error in timeout itself
- **126**: Command found but cannot be invoked
- **127**: Command not found
- **137**: SIGKILL sent (128 + 9)
- **Other**: Child's actual exit code

## Advanced Features

### Signal Forwarding

The implementation forwards SIGINT and SIGTERM from the parent (timeout process) to the child:

```rust
// Setup signal handlers
let mut sigint = signal(SignalKind::interrupt())?;
let mut sigterm = signal(SignalKind::terminate())?;

// In select! block
_ = sigint.recv() => {
    kill(pid, Signal::SIGINT)?;
    // Wait for child
}
```

This ensures Ctrl-C propagates correctly to the monitored command.

### Preserve Status

When `--preserve-status` is used:

```rust
let code = exit_status.code().unwrap_or(EXIT_CANCELED);
return Ok(if preserve_status { code } else { EXIT_TIMEDOUT });
```

This allows scripts to distinguish between command failures and timeouts.

### Verbose Mode

Diagnostic output for debugging:

```rust
if verbose {
    eprintln!("timeout: sending signal {} to command '{}'",
        signal_to_string(term_signal), command);
}
```

## Performance Considerations

### Memory Efficiency

- **Zero-copy I/O**: Child's stdin/stdout/stderr inherited directly
- **No buffer allocations**: Streams passed through without copying
- **Minimal overhead**: Async runtime adds ~100KB to binary size

### CPU Efficiency

- **No busy-waiting**: Uses event-driven model
- **Efficient signal handling**: Kernel-level notifications
- **Optimized release build**: LTO, single codegen unit, stripped symbols

### Latency

- **Signal delivery**: Near-instantaneous (~1-5ms)
- **Timeout precision**: Limited by OS timer resolution (~1-10ms)
- **Process spawn overhead**: Similar to C implementation

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("10s").unwrap(), Duration::from_secs(10));
        assert_eq!(parse_duration("5m").unwrap(), Duration::from_secs(300));
        // ... more tests
    }

    #[test]
    fn test_parse_signal() {
        assert_eq!(parse_signal("TERM").unwrap(), Signal::SIGTERM);
        assert_eq!(parse_signal("9").unwrap(), Signal::SIGKILL);
        // ... more tests
    }
}
```

### Integration Tests

The included `test_timeout.sh` script covers:

- Basic timeout scenarios
- Signal handling
- Kill-after functionality
- Edge cases
- Exit code verification

## Differences from GNU Timeout

### Similarities ✅

- Command-line interface (arguments, options)
- Duration parsing (s/m/h/d suffixes)
- Signal handling (custom signals, kill-after)
- Exit codes (124, 125, 126, 127, 137)
- Preserve status flag
- Verbose mode
- Process group management with `setpgid`
- Core dump control with `prctl(PR_SET_DUMPABLE)`
- TTY process group handling
- SIGCONT propagation for stopped processes
- Foreground mode for interactive commands

### Differences ⚠️

1. **Platform Support** 🆕:

   - GNU: POSIX systems with limited Windows support
   - Rust: **Full Unix + Windows support** with platform-specific features
   - Unix: Fork-based with full signal control
   - Windows: Async process management with Ctrl+C propagation

2. **Real-time Signals**: Not yet implemented
   - GNU: Supports SIGRTMIN/SIGRTMAX
   - Rust: Supports standard signals only (Unix)

### Enhancement Opportunities

1. ~~**Windows Support**~~: ✅ **Implemented** - Native Windows support with `tokio::process`
2. **Real-time Signals**: Support for SIGRTMIN/SIGRTMAX range
3. **Async I/O Redirection**: Capture/redirect child output
4. ~~**Resource Limits**~~: ✅ **Implemented** - `--cpu-limit` and `--mem-limit` on Linux/BSD
5. **Extended Exit Codes**: ✅ **Implemented** - Custom exit codes via `--status` flag

## Dependencies

### Production Dependencies

```toml
clap = { version = "4.5", features = ["derive"] }
tokio = { version = "1.40", features = ["full"] }
nix = { version = "0.29", features = ["signal", "process"] }
```

**Why these crates?**

- **clap**: Industry-standard CLI parser, excellent derive macro support
- **tokio**: Most mature async runtime in Rust ecosystem
- **nix**: Safe Unix system call wrappers

**Binary size**: ~2-3 MB (release with strip)

### Development Dependencies

```toml
[dev-dependencies]
assert_cmd = "2.0"  # For integration tests
predicates = "3.0"  # For assertion helpers
```

## Build Configuration

```toml
[profile.release]
opt-level = 3          # Maximum optimization
lto = true            # Link-time optimization
codegen-units = 1     # Single codegen unit for better optimization
strip = true          # Remove debug symbols
```

**Result**: Highly optimized binary with minimal size and maximum performance.

## Future Enhancements

### Short-term (Low Effort, High Value)

1. **Unit tests**: Add comprehensive test suite
2. **Documentation**: Add inline docs with `cargo doc`
3. **CI/CD**: GitHub Actions for automated testing
4. **Packaging**: Create .deb/.rpm packages, Windows installer

### Medium-term

1. ~~**Windows support**~~: ✅ **Implemented** - Native Windows support
2. **Better error messages**: More descriptive user-facing errors
3. **Shell completion**: Generate completions for bash/zsh/fish/PowerShell
4. **Man page**: Generate man page documentation

### Long-term (Advanced Features)

1. **Process groups**: Full implementation matching GNU behavior (Unix)
2. **Multiple monitors**: Support multiple child processes
3. ~~**Resource limits**~~: ✅ **Implemented** - CPU/memory limits on Linux/BSD
4. **Monitoring hooks**: Callbacks for custom monitoring
5. **Configuration file**: Support for `.timeoutrc`
6. **Real-time signals**: SIGRTMIN/SIGRTMAX support (Unix)

## Conclusion

This Rust implementation provides a modern, safe, and efficient alternative to GNU timeout while maintaining compatibility with its interface. The use of async/await and tokio::select! makes the code more maintainable and easier to extend than the original C implementation.

### Strengths

- ✅ Type safety and memory safety (Rust guarantees)
- ✅ Clean, readable code with modern patterns
- ✅ Excellent error handling with Result types
- ✅ Easy to test and maintain
- ✅ **Cross-platform support** 🆕 - Full Unix + Windows compatibility
- ✅ **Modern features** 🆕 - Custom exit codes, no-notify mode
- ✅ **Resource limits** 🆕 - CPU/memory constraints (Linux/BSD)

### Areas for Improvement

- ⚠️ ~~Windows support not yet implemented~~ ✅ **Now implemented**
- ⚠️ Some advanced GNU features simplified (real-time signals)
- ⚠️ Binary size larger than C (but still reasonable)
- ⚠️ Windows lacks Unix signal equivalents (platform limitation)

Overall, this implementation demonstrates how Rust's modern language features can be used to recreate classic Unix utilities with improved safety and maintainability.
