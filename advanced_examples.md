# Advanced Usage Examples

This document demonstrates real-world scenarios where the advanced features of timeout are essential.

**Platform Note:** Most examples in this document demonstrate Unix-specific features (process groups, signals, resource limits). For Windows-specific examples and limitations, see the Platform Support documentation.

## 1. Killing Entire Process Trees

### Problem: Orphaned Child Processes

When timing out a script that spawns background jobs:

```bash
#!/bin/bash
# bad_script.sh - spawns background jobs
yes > /dev/null &
sleep 100 &
cat /dev/urandom > /dev/null &
wait
```

**Without process groups** (hypothetical basic timeout):

```bash
timeout 5s ./bad_script.sh
# After timeout, all three background processes (yes, sleep, cat) continue running!
# ps aux | grep -E '(yes|sleep|cat)' would show orphaned processes
```

**With process groups** (this implementation):

```bash
timeout 5s ./bad_script.sh
# After timeout, ALL processes in the group are killed
# ps aux shows no orphaned processes ✓
```

### How It Works

```
Process Tree WITHOUT Process Groups:
timeout (PID 1000, PGID 1000)
  └─ bash (PID 1001, PGID 1001)  ← New process group!
       ├─ yes (PID 1002, PGID 1001)
       ├─ sleep (PID 1003, PGID 1001)
       └─ cat (PID 1004, PGID 1001)

Timeout kills PID 1001 only → 1002, 1003, 1004 orphaned

Process Tree WITH Process Groups:
timeout (PID 1000, PGID 1000)  ← setpgid(0,0) called
  └─ bash (PID 1001, PGID 1000)  ← Same group!
       ├─ yes (PID 1002, PGID 1000)
       ├─ sleep (PID 1003, PGID 1000)
       └─ cat (PID 1004, PGID 1000)

Timeout kills PGID 1000 → All processes killed ✓
```

## 2. Handling Pipelines

### The Pipeline Problem

Pipelines in bash create multiple processes:

```bash
timeout 5s cat /dev/urandom | head -c 1000000000 | gzip > output.gz
```

**Process structure:**

- `timeout` spawns `bash`
- `bash` spawns three processes in the pipeline: `cat`, `head`, `gzip`

**With proper process groups:**

```bash
# All three pipeline processes (cat, head, gzip) are killed on timeout
timeout 5s sh -c 'cat /dev/urandom | head -c 1000000000 | gzip > output.gz'
```

**Testing:**

```bash
# Start the pipeline
timeout 10s sh -c 'yes | head -c 10000000000 > /dev/null' &
TIMEOUT_PID=$!

# Check process tree
ps axjf | grep -A5 $TIMEOUT_PID

# Wait for timeout
wait $TIMEOUT_PID

# Verify all processes are gone
ps aux | grep -E '(yes|head)' | grep -v grep
# Should show nothing
```

## 3. Interactive Commands with Foreground Mode

### Use Case: Debugging with GDB

GDB needs TTY access to provide interactive debugging:

```bash
# Without --foreground: GDB can't interact with terminal
timeout 60s gdb ./myprogram
# GDB complains: "Error: Cannot read from stdin"

# With --foreground: Full GDB interactivity
timeout --foreground 60s gdb ./myprogram
# GDB works normally with readline, can set breakpoints, etc.
```

### Use Case: Password Prompts

Commands that need to read passwords:

```bash
# SSH with password authentication
timeout --foreground 30s ssh user@remote-host

# sudo commands
timeout --foreground 10s sudo apt update

# Interactive installers
timeout --foreground 300s ./installer.sh
```

### Behavior Difference

```bash
#!/bin/bash
# test_foreground.sh

echo "Starting background job..."
sleep 100 &
echo "Background PID: $!"
wait
```

**Default mode:**

```bash
timeout 5s ./test_foreground.sh
# After timeout, BOTH the script AND the background sleep are killed
```

**Foreground mode:**

```bash
timeout --foreground 5s ./test_foreground.sh
# After timeout, only the script is killed
# Background sleep continues running!
```

## 4. Handling Stopped Processes

### The SIGCONT Issue

Some processes might be stopped (SIGSTOP) and won't receive signals:

```bash
#!/bin/bash
# stopped_process.sh
sleep 100 &
CHILD_PID=$!
echo "Started sleep with PID $CHILD_PID"
kill -STOP $CHILD_PID
echo "Stopped the sleep process"
wait
```

**Without SIGCONT:**

```bash
timeout 5s ./stopped_process.sh
# Timeout sends SIGTERM, but process is stopped
# Process doesn't respond to SIGTERM
# Hangs until kill-after or manual intervention
```

**With SIGCONT (this implementation):**

```bash
timeout 5s ./stopped_process.sh
# Timeout sends SIGTERM
# Timeout also sends SIGCONT
# Process wakes up, receives SIGTERM, exits cleanly ✓
```

### Real-World Example: Process Suspension

Job control with Ctrl-Z:

```bash
# Terminal 1: Start a long-running process
some_long_command

# Press Ctrl-Z to suspend
^Z
[1]+  Stopped    some_long_command

# Terminal 2: Try to timeout the suspended process
timeout 5s kill -CONT %1; wait %1
# Without SIGCONT propagation, this wouldn't work properly
```

## 5. Kill-After with Stubborn Processes

### Graceful Shutdown, Then Force

Some processes ignore SIGTERM and need SIGKILL:

```bash
#!/bin/bash
# stubborn_daemon.sh
trap 'echo "Ignoring SIGTERM!"; sleep 10' TERM
sleep 1000
```

**Just SIGTERM (waits forever):**

```bash
timeout 5s ./stubborn_daemon.sh
# Sends SIGTERM
# Process ignores it
# Waits indefinitely... ✗
```

**With kill-after (force kill):**

```bash
timeout -k 2s 5s ./stubborn_daemon.sh
# After 5 seconds: sends SIGTERM
# Process ignores SIGTERM
# After 2 more seconds: sends SIGKILL
# Process is forcefully terminated ✓
```

### Database Graceful Shutdown

```bash
# Try graceful shutdown first, force after 10 seconds
timeout -k 10s 60s pg_ctl stop -D /var/lib/postgresql/data

# Same for Docker containers
timeout -k 5s 30s docker stop my_container
```

## 6. Core Dump Prevention

### Security Scenario

When running untrusted or sensitive code:

```bash
# Bad: Core dumps might contain passwords, keys, or PII
./payment_processor --api-key=secret123

# Good: Timeout prevents core dumps while monitoring
timeout 300s ./payment_processor --api-key=secret123
# If payment_processor crashes, no core dump is created for timeout itself
# payment_processor can still generate its own core dumps if needed
```

### Testing Core Dump Control

```bash
# Terminal 1: Check if timeout prevents its own core dumps
cat > test_dump.sh << 'EOF'
#!/bin/bash
# This will segfault after 2 seconds
sleep 2
kill -SEGV $$
EOF
chmod +x test_dump.sh

# Run with timeout
ulimit -c unlimited  # Enable core dumps
timeout 10s ./test_dump.sh

# Check for core files
ls -lah core* 2>/dev/null || echo "No core dump (expected for timeout)"

# The script itself will create a core dump
# But timeout won't create one if it receives a signal
```

## 7. Script Integration Patterns

### Pattern 1: Retry with Timeout

```bash
#!/bin/bash

MAX_RETRIES=3
RETRY_COUNT=0

while [ $RETRY_COUNT -lt $MAX_RETRIES ]; do
    timeout 30s curl https://api.example.com/data > data.json

    if [ $? -eq 0 ]; then
        echo "Success!"
        break
    elif [ $? -eq 124 ]; then
        echo "Timeout on attempt $((RETRY_COUNT + 1))"
        RETRY_COUNT=$((RETRY_COUNT + 1))
    else
        echo "Error (not timeout): $?"
        exit 1
    fi
done
```

### Pattern 2: Health Check with Preserve Status

```bash
#!/bin/bash

# Check if service responds within 5 seconds
timeout --preserve-status 5s curl -f http://localhost:8080/health

EXIT_CODE=$?

case $EXIT_CODE in
    0)
        echo "Service is healthy"
        ;;
    124)
        echo "Health check timed out"
        exit 1
        ;;
    *)
        echo "Service returned error: $EXIT_CODE"
        exit 1
        ;;
esac
```

### Pattern 3: Parallel Task Execution

```bash
#!/bin/bash

# Run multiple tasks with individual timeouts
timeout 60s ./task1.sh &
PID1=$!

timeout 90s ./task2.sh &
PID2=$!

timeout 120s ./task3.sh &
PID3=$!

# Wait for all tasks
wait $PID1
STATUS1=$?

wait $PID2
STATUS2=$?

wait $PID3
STATUS3=$?

# Check which tasks timed out
[ $STATUS1 -eq 124 ] && echo "Task 1 timed out"
[ $STATUS2 -eq 124 ] && echo "Task 2 timed out"
[ $STATUS3 -eq 124 ] && echo "Task 3 timed out"
```

### Pattern 4: CI/CD Test Runner

```bash
#!/bin/bash
# Run tests with timeout to prevent CI hangs

FAILED_TESTS=()

for test in tests/*.sh; do
    echo "Running $test..."

    timeout -v -k 5s 300s "$test"
    EXIT_CODE=$?

    case $EXIT_CODE in
        0)
            echo "✓ $test passed"
            ;;
        124)
            echo "✗ $test timed out after 5 minutes"
            FAILED_TESTS+=("$test (timeout)")
            ;;
        *)
            echo "✗ $test failed with exit code $EXIT_CODE"
            FAILED_TESTS+=("$test (exit $EXIT_CODE)")
            ;;
    esac
done

if [ ${#FAILED_TESTS[@]} -gt 0 ]; then
    echo ""
    echo "Failed tests:"
    printf '%s\n' "${FAILED_TESTS[@]}"
    exit 1
fi
```

## 8. Debugging Timeout Behavior

### Verbose Mode

```bash
# See exactly what signals are sent
timeout -v 5s sleep 10
# Output: timeout: sending signal SIGTERM to command 'sleep'

# With kill-after
timeout -v -k 2s 5s sleep 10
# Output:
# timeout: sending signal SIGTERM to command 'sleep'
# timeout: sending signal SIGKILL to command 'sleep'
```

### Checking Process Groups

```bash
#!/bin/bash
# show_pgids.sh - Display process group information

echo "Timeout PGID: $(ps -o pgid= -p $$)"
echo "Timeout PID: $$"

sleep 100 &
echo "Sleep PGID: $(ps -o pgid= -p $!)"
echo "Sleep PID: $!"

wait
```

Run it:

```bash
timeout 5s ./show_pgids.sh
# Shows that both timeout and sleep are in the same PGID
```

### Monitoring Signal Delivery

```bash
#!/bin/bash
# catch_signals.sh

trap 'echo "Received SIGTERM at $(date +%s)"; exit 0' TERM
trap 'echo "Received SIGCONT at $(date +%s)"' CONT
trap 'echo "Received SIGKILL - this will never print"' KILL

echo "Started at $(date +%s)"
sleep 1000
```

Test it:

```bash
timeout -v 5s ./catch_signals.sh
# Output shows signal delivery timing
```

## 9. Edge Cases and Gotchas

### Edge Case 1: Zero Duration

```bash
# Should timeout immediately
timeout 0s sleep 10
echo $?  # 124

# Same behavior as:
timeout 0 sleep 10
```

### Edge Case 2: Command Not Found

```bash
timeout 5s nonexistent_command
echo $?  # 127 (EXIT_ENOENT)
```

### Edge Case 3: Permission Denied

```bash
touch test.sh
chmod -x test.sh

timeout 5s ./test.sh
echo $?  # 126 (EXIT_CANNOT_INVOKE)
```

### Edge Case 4: Nested Timeouts

```bash
# Inner timeout expires first
timeout 10s timeout 5s sleep 100
echo $?  # 124 (inner timeout)

# Outer timeout expires first
timeout 5s timeout 10s sleep 100
echo $?  # 124 (outer timeout)
```

### Edge Case 5: Signal Cascades

```bash
#!/bin/bash
# signal_cascade.sh
timeout 5s bash -c 'timeout 10s sleep 100'
# Both timeouts use process groups
# Inner bash is in outer timeout's group
# When outer times out, inner timeout AND sleep are killed
```

## Summary

These advanced features make timeout a production-ready tool for:

1. **Complex Process Management**: Handling process trees, pipelines, and background jobs
2. **Security**: Preventing core dumps and information leakage
3. **Reliability**: Properly handling stopped processes and stubborn daemons
4. **Flexibility**: Supporting both batch and interactive use cases
5. **Debuggability**: Verbose mode and proper exit codes

The Rust implementation matches GNU timeout's behavior exactly, making it a drop-in replacement for production use.
