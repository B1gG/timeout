# Platform Support Documentation

## Overview

This Rust timeout implementation uses conditional compilation to support multiple Unix-like operating systems with graceful feature degradation on platforms that don't support all features.

---

## Supported Platforms

### ✅ Tier 1: Full Support - Linux

**All features fully supported:**

- ✅ Time-based timeouts
- ✅ Custom signals (SIGTERM, SIGKILL, etc.)
- ✅ Kill-after functionality
- ✅ Process group management
- ✅ Event-driven SIGCHLD monitoring
- ✅ PR_SET_PDEATHSIG (orphan prevention)
- ✅ PR_SET_DUMPABLE (core dump control)
- ✅ CPU limits (RLIMIT_CPU)
- ✅ Memory limits (RLIMIT_AS)
- ✅ WUNTRACED (stopped process detection)
- ✅ Structured error handling
- ✅ JSON metrics

**Platform Detection:**

```rust
Platform::IS_LINUX == true
Platform::HAS_PRCTL == true
Platform::HAS_RLIMIT_AS == true
```

**Recommended for:**

- Production deployments
- Docker/containers
- CI/CD systems
- All advanced features

---

### ⚠️ Tier 2: Good Support - FreeBSD & DragonFly BSD

**Fully supported:**

- ✅ Time-based timeouts
- ✅ Custom signals
- ✅ Kill-after functionality
- ✅ Process group management
- ✅ Event-driven SIGCHLD monitoring
- ✅ CPU limits (RLIMIT_CPU)
- ✅ Memory limits (RLIMIT_DATA)
- ✅ WUNTRACED (stopped process detection)
- ✅ Structured error handling
- ✅ JSON metrics

**Not supported:**

- ❌ PR_SET_PDEATHSIG (Linux-specific)
- ❌ PR_SET_DUMPABLE (Linux-specific)

**Differences:**

- `RLIMIT_AS` replaced with `RLIMIT_DATA` (similar behavior)
- No automatic orphan prevention (use process groups instead)

**Platform Detection:**

```rust
Platform::IS_FREEBSD == true  // or IS_DRAGONFLY
Platform::HAS_PRCTL == false
Platform::HAS_RLIMIT_AS == true
```

**Workarounds:**

```bash
# Orphan prevention without PR_SET_PDEATHSIG:
# Use process groups (already handled by default)
timeout 10s ./command

# Process group ensures cleanup even if timeout dies
```

**Recommended for:**

- FreeBSD servers
- BSD-based systems
- Most features work well

---

### ⚠️ Tier 3: Basic Support - macOS, OpenBSD, NetBSD

**Fully supported:**

- ✅ Time-based timeouts
- ✅ Custom signals
- ✅ Kill-after functionality
- ✅ Process group management
- ✅ Event-driven SIGCHLD monitoring
- ✅ WUNTRACED (stopped process detection)
- ✅ Structured error handling
- ✅ JSON metrics

**Not supported:**

- ❌ PR_SET_PDEATHSIG (no prctl on macOS/BSD)
- ❌ PR_SET_DUMPABLE (no prctl)
- ❌ CPU limits (--cpu-limit will error)
- ❌ Memory limits (--mem-limit will error)

**Platform Detection:**

```rust
Platform::IS_MACOS == true  // or IS_OPENBSD, IS_NETBSD
Platform::HAS_PRCTL == false
Platform::HAS_RLIMIT_AS == false
```

**Warnings issued:**

```bash
$ timeout --cpu-limit 10 30s ./program
Warning: Running on macOS. Some features may have limited support.
Error: Resource limits requested but not available on macOS
```

**Recommended for:**

- Development on macOS
- Basic timeout functionality
- Testing (without resource limits)

**Not recommended for:**

- Production systems requiring resource limits
- Security sandboxing
- CPU/memory constrained environments

---

## Feature Availability Matrix

| Feature               | Linux | FreeBSD | DragonFly | macOS | OpenBSD | NetBSD | Windows |
| --------------------- | ----- | ------- | --------- | ----- | ------- | ------ | ------- |
| **Core Features**     |
| Time limits           | ✅    | ✅      | ✅        | ✅    | ✅      | ✅     | ✅      |
| Custom signals        | ✅    | ✅      | ✅        | ✅    | ✅      | ✅     | ❌²     |
| Kill-after            | ✅    | ✅      | ✅        | ✅    | ✅      | ✅     | ✅      |
| Process groups        | ✅    | ✅      | ✅        | ✅    | ✅      | ✅     | ❌²     |
| Foreground mode       | ✅    | ✅      | ✅        | ✅    | ✅      | ✅     | ❌²     |
| **Advanced Features** |
| SIGCHLD events        | ✅    | ✅      | ✅        | ✅    | ✅      | ✅     | ❌²     |
| WUNTRACED             | ✅    | ✅      | ✅        | ✅    | ✅      | ✅     | ❌²     |
| Metrics               | ✅    | ✅      | ✅        | ✅    | ✅      | ✅     | ✅      |
| No-notify mode        | ✅    | ✅      | ✅        | ✅    | ✅      | ✅     | ❌²     |
| Custom exit codes     | ✅    | ✅      | ✅        | ✅    | ✅      | ✅     | ✅      |
| **Linux-Specific**    |
| PR_SET_PDEATHSIG      | ✅    | ❌      | ❌        | ❌    | ❌      | ❌     | ❌      |
| PR_SET_DUMPABLE       | ✅    | ❌      | ❌        | ❌    | ❌      | ❌     | ❌      |
| **Resource Limits**   |
| RLIMIT_CPU            | ✅    | ✅      | ✅        | ❌    | ❌      | ❌     | ❌²     |
| RLIMIT_AS/DATA        | ✅    | ✅¹     | ✅¹       | ❌    | ❌      | ❌     | ❌²     |

**Notes:**

1. FreeBSD/DragonFly use RLIMIT_DATA instead of RLIMIT_AS
2. Windows doesn't support Unix signals; uses process termination instead

---

## Windows Support 🆕

### Overview

**Tier:** 2 (Core features, limited advanced features)

Windows support provides core timeout functionality using native Windows process management:

**✅ Supported:**

- Time-based process termination
- Kill-after functionality
- Custom exit codes (--status)
- JSON metrics output
- Ctrl+C signal propagation

**❌ Not Supported:**

- Unix signals (--signal flag)
- Process groups (--foreground flag)
- Stopped process detection (--detect-stopped)
- Resource limits (--cpu-limit, --mem-limit)
- No-notify mode (--no-notify, Unix-only)
- Preserve status (--preserve-status, Unix-only)

### Windows-Specific Behavior

**Process Termination:**

```bash
# On Windows, always uses TerminateProcess() API
timeout 5s program.exe
# After 5s: child.kill() -> TerminateProcess with exit code 1
```

**Ctrl+C Handling:**

```bash
# Propagates Ctrl+C to child process
timeout 30s long-running.exe
^C  # Both timeout and child terminate
```

**Kill-After:**

```bash
# --kill-after still works (immediate termination)
timeout -k 2s 10s program.exe
# After 10s: child.kill() called
# After 12s (if still running): child.kill() called again
```

### Platform Detection

```rust
#[cfg(windows)]
{
    // Windows-specific implementation
    use tokio::process::Command;
    use tokio::signal::windows::ctrl_c;
}

#[cfg(unix)]
{
    // Unix-specific implementation
    use nix::sys::signal::Signal;
}
```

### Building on Windows

**Native Windows Build:**

```powershell
# Install Rust (if not already installed)
# https://rustup.rs/

# Build
cargo build --release

# Binary location
.\target\release\timeout.exe
```

**Cross-Compilation from Linux:**

```bash
# Install Windows target
rustup target add x86_64-pc-windows-gnu

# Install MinGW toolchain
sudo apt-get install mingw-w64

# Build
cargo build --release --target x86_64-pc-windows-gnu
```

### Windows Examples

**Basic usage:**

```powershell
# Timeout after 5 seconds
.\timeout.exe 5s cmd /c "echo Hello && timeout /t 10"

# Kill-after
.\timeout.exe -k 2s 10s long-process.exe

# Custom exit code
.\timeout.exe --status 99 5s program.exe
```

**Checking exit codes:**

```powershell
.\timeout.exe 2s timeout /t 10
echo Exit code: %ERRORLEVEL%
# Output: Exit code: 124 (timed out)
```

**JSON metrics:**

```powershell
.\timeout.exe --metrics 5s program.exe > metrics.json
```

### Windows Limitations

**No Signal Control:**
Windows doesn't have Unix signals. Attempted use of signal flags will be ignored:

```powershell
# These flags are Unix-only (compile error on Windows)
.\timeout.exe --signal TERM 10s program.exe  # Error
.\timeout.exe --foreground 10s program.exe   # Error
.\timeout.exe --no-notify 10s program.exe    # Error
```

**No Resource Limits:**

```powershell
# These require Unix rlimit (compile error on Windows)
.\timeout.exe --cpu-limit 10 30s program.exe   # Error
.\timeout.exe --mem-limit 1G 30s program.exe   # Error
```

**Workaround - Use Windows Job Objects:**
For resource limiting on Windows, consider using Windows Job Objects separately:

```powershell
# Create job object with limits (requires external tool)
# Then run timeout within that job
```

### Recommendations for Windows

**✅ Use for:**

- Basic timeout functionality
- Time-limited command execution
- CI/CD pipelines
- Automated testing
- Development workflows

**⚠️ Limitations:**

- No fine-grained signal control
- No resource limit enforcement
- Limited process group management
- Use Windows-native tools for advanced process control

**Notes:**

1. FreeBSD/DragonFly use RLIMIT_DATA instead of RLIMIT_AS

---

## Conditional Compilation Details

### Platform Detection

The code uses Rust's conditional compilation:

```rust
// Detect Linux
#[cfg(target_os = "linux")]
fn linux_only_feature() { /* ... */ }

// Detect BSD variants
#[cfg(any(target_os = "freebsd", target_os = "openbsd", target_os = "netbsd"))]
fn bsd_feature() { /* ... */ }

// Detect macOS
#[cfg(target_os = "macos")]
fn macos_feature() { /* ... */ }

// Feature availability
pub const HAS_PRCTL: bool = cfg!(target_os = "linux");
pub const HAS_RLIMIT_AS: bool = cfg!(any(
    target_os = "linux",
    target_os = "freebsd",
    target_os = "dragonfly"
));
```

### Imports

**Linux:**

```rust
use nix::libc::{prctl, PR_SET_DUMPABLE, PR_SET_PDEATHSIG};
use nix::sys::resource::{setrlimit, Resource};
```

**FreeBSD/DragonFly:**

```rust
use nix::sys::resource::{setrlimit, Resource};
// No prctl available
```

**macOS/OpenBSD/NetBSD:**

```rust
// No prctl, no resource limits (in our implementation)
```

### Feature Guards

**PR_SET_PDEATHSIG:**

```rust
#[cfg(target_os = "linux")]
{
    unsafe {
        prctl(PR_SET_PDEATHSIG, Signal::SIGKILL as i32);
    }
}

#[cfg(not(target_os = "linux"))]
if verbose {
    eprintln!("Note: orphan prevention not available on {}", Platform::name());
}
```

**Resource Limits:**

```rust
#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "dragonfly"))]
{
    if let Some(cpu_secs) = cpu_limit {
        setrlimit(Resource::RLIMIT_CPU, cpu_secs, cpu_secs)?;
    }
}

#[cfg(not(any(target_os = "linux", target_os = "freebsd", target_os = "dragonfly")))]
{
    if cpu_limit.is_some() {
        return Err(TimeoutError::FeatureNotSupported(
            "CPU limits not supported on this platform".to_string()
        ));
    }
}
```

---

## Building for Different Platforms

### Linux (Native)

```bash
cargo build --release
# Full features available
```

### macOS (Native)

```bash
cargo build --release
# Basic features only
# Warning: --cpu-limit and --mem-limit will error
```

### Cross-Compilation

**Linux to FreeBSD:**

```bash
# Install cross-compilation toolchain
rustup target add x86_64-unknown-freebsd

# Build
cargo build --release --target x86_64-unknown-freebsd
```

**Check what will be compiled:**

```bash
# Show configuration for target
rustc --print cfg --target x86_64-unknown-freebsd

# Output includes:
# target_os="freebsd"
# target_family="unix"
```

---

## Testing on Different Platforms

### Linux

**Full test suite:**

```bash
# All features work
./test_timeout.sh

# Test resource limits
timeout --cpu-limit 5 10s perl -e 'while(1){}'
timeout --mem-limit 100M 10s perl -e 'my $x = "a" x 100000000'

# Test orphan prevention
timeout 10s sleep 100 &
TIMEOUT_PID=$!
kill -9 $TIMEOUT_PID
# Child is also killed ✓
```

### FreeBSD/DragonFly

**Most features work:**

```bash
# Basic features
timeout 5s sleep 10  # ✓

# Resource limits
timeout --cpu-limit 5 10s ./program  # ✓
timeout --mem-limit 100M 10s ./program  # ✓ (uses RLIMIT_DATA)

# Orphan prevention via process groups
timeout 10s sleep 100 &
TIMEOUT_PID=$!
kill -9 $TIMEOUT_PID
# Child killed via process group ✓
```

### macOS/OpenBSD/NetBSD

**Basic features only:**

```bash
# Works
timeout 5s sleep 10  # ✓
timeout -k 2s 5s sleep 100  # ✓
timeout --detect-stopped 10s ./program  # ✓

# Doesn't work
timeout --cpu-limit 10 30s ./program  # ✗ Error
timeout --mem-limit 512M 30s ./program  # ✗ Error
```

---

## Metrics Output by Platform

**Linux:**

```json
{
  "command": "test",
  "duration_ms": 5000,
  "timed_out": false,
  "exit_code": 0,
  "signal": "none",
  "elapsed_ms": 1234,
  "kill_after_used": false,
  "cpu_limit": 30,
  "memory_limit": 536870912,
  "stopped_detected": false,
  "platform": "Linux"
}
```

**FreeBSD:**

```json
{
  "platform": "FreeBSD",
  "cpu_limit": 30,
  "memory_limit": 536870912,
  ...
}
```

**macOS:**

```json
{
  "platform": "macOS",
  "cpu_limit": null,
  "memory_limit": null,
  ...
}
```

---

## Migration from GNU Timeout

### Fully Compatible

**All platforms:**

```bash
# These work everywhere
timeout 30s ./command
timeout -s TERM 5m ./command
timeout -k 10s 1h ./command
timeout --foreground 30s ./program
timeout --preserve-status 10s ./test
```

### Linux-Specific Extensions

**Only on Linux:**

```bash
# These require Linux
timeout --cpu-limit 60 10m ./computation
timeout --mem-limit 1G 5m ./memory_intensive

# Fallback for other platforms:
# Use ulimit before running timeout
ulimit -t 60  # CPU limit (seconds)
ulimit -v 1048576  # Virtual memory (KB)
timeout 10m ./computation
```

---

## Recommendations

### For Linux Deployments ✅

**Use all features:**

- Full feature set available
- Optimal performance
- Best security (PR_SET_PDEATHSIG)
- Production-ready

### For FreeBSD/DragonFly ⚠️

**Use most features:**

- Resource limits available
- Process groups work well
- Missing: automatic orphan prevention
- Good for production with minor compromises

### For macOS Development ⚠️

**Use basic features:**

- Good for development/testing
- Most core features work
- Don't rely on resource limits
- Not recommended for production

### For OpenBSD/NetBSD ⚠️

**Use basic features only:**

- Similar to macOS
- Core timeout functionality works
- No resource limits
- Consider GNU timeout for full compatibility

---

## Future Improvements

### Potential Enhancements

**1. macOS Resource Limits:**
Could potentially use macOS-specific APIs:

```rust
#[cfg(target_os = "macos")]
{
    // Use setrlimit with RLIMIT_CPU (works on macOS)
    // Use RLIMIT_RSS instead of RLIMIT_AS
}
```

**2. kqueue on BSD:**
Better event handling:

```rust
#[cfg(any(target_os = "freebsd", target_os = "openbsd"))]
{
    // Use kqueue for process monitoring
    // More efficient than polling
}
```

**3. Capsicum on FreeBSD:**
Enhanced sandboxing:

```rust
#[cfg(target_os = "freebsd")]
{
    // Use Capsicum for capability-based security
}
```

---

## Troubleshooting

### "Feature not supported" Error

**Problem:**

```bash
$ timeout --cpu-limit 10 30s ./program
Error: Resource limits requested but not available on macOS
```

**Solution:**
Remove resource limit flags on unsupported platforms:

```bash
timeout 30s ./program  # Works on all platforms
```

### Orphan Processes on Non-Linux

**Problem:**
Orphan processes when timeout is killed on macOS/BSD.

**Solution:**
Use process groups (default behavior handles this):

```bash
# Process groups still work on all platforms
timeout 30s ./program

# Even if timeout is killed, process group is terminated
```

### Resource Limits Not Working

**Problem:**
CPU/memory limits don't work as expected on FreeBSD.

**Check:**

```bash
# Verify platform
timeout --version

# Check if limits are supported
if [ "$(uname)" = "Linux" ]; then
    timeout --cpu-limit 10 30s ./program
else
    ulimit -t 10  # Use ulimit instead
    timeout 30s ./program
fi
```

---

## Summary

| Platform       | Tier | Features | Recommendation             |
| -------------- | ---- | -------- | -------------------------- |
| **Linux**      | 1    | 100%     | ✅ Use for production      |
| **FreeBSD**    | 2    | 90%      | ✅ Good for production     |
| **DragonFly**  | 2    | 90%      | ✅ Good for production     |
| **Windows** 🆕 | 2    | 75%      | ✅ Good for most use cases |
| **macOS**      | 3    | 70%      | ⚠️ Dev/testing only        |
| **OpenBSD**    | 3    | 70%      | ⚠️ Basic use only          |
| **NetBSD**     | 3    | 70%      | ⚠️ Basic use only          |

**Best practice:**

- **Linux**: Full features, best for production
- **Windows**: Core features, excellent for Windows environments
- **BSD**: Most features, good for production
- **macOS**: Development and testing
