#!/bin/bash

# Test script for the Rust timeout implementation
# This script runs various tests to verify functionality

set -e

TIMEOUT_BIN="./target/release/timeout"
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "======================================="
echo "Testing Rust Timeout Implementation"
echo "======================================="
echo ""

# Check if binary exists
if [ ! -f "$TIMEOUT_BIN" ]; then
    echo -e "${RED}Error: timeout binary not found at $TIMEOUT_BIN${NC}"
    echo "Please build first with: cargo build --release"
    exit 1
fi

# Test counter
TESTS_PASSED=0
TESTS_FAILED=0

# Function to run a test
run_test() {
    local test_name="$1"
    local expected_exit="$2"
    shift 2
    local cmd=("$@")
    
    echo -ne "${YELLOW}Testing: $test_name${NC} ... "
    
    # Run the command
    set +e
    "${cmd[@]}" > /dev/null 2>&1
    local actual_exit=$?
    set -e
    
    if [ "$actual_exit" -eq "$expected_exit" ]; then
        echo -e "${GREEN}PASS${NC} (exit: $actual_exit)"
        TESTS_PASSED=$((TESTS_PASSED + 1))
    else
        echo -e "${RED}FAIL${NC} (expected: $expected_exit, got: $actual_exit)"
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

echo "=== Basic Timeout Tests ==="
echo ""

# Test 1: Command completes before timeout
run_test "Command completes successfully" 0 \
    "$TIMEOUT_BIN" 5s sleep 1

# Test 2: Command times out
run_test "Command times out" 124 \
    "$TIMEOUT_BIN" 1s sleep 10

# Test 3: Different duration formats
run_test "Duration in seconds" 124 \
    "$TIMEOUT_BIN" 1s sleep 5

run_test "Duration in minutes" 0 \
    "$TIMEOUT_BIN" 1m sleep 2

# Test 4: Command not found
run_test "Command not found" 127 \
    "$TIMEOUT_BIN" 5s nonexistent_command_xyz

echo ""
echo "=== Signal Tests ==="
echo ""

# Test 5: Custom signal (INT)
run_test "Custom signal SIGINT" 124 \
    "$TIMEOUT_BIN" -s INT 1s sleep 10

# Test 6: SIGKILL signal - still returns 124 because timeout occurred
# (GNU timeout also returns 124 for timeouts, not the signal number)
run_test "SIGKILL signal" 124 \
    "$TIMEOUT_BIN" -s SIGKILL 1s sleep 10

echo ""
echo "=== Kill After Tests ==="
echo ""

# Test 7: Kill after timeout - returns 124 for timeout, not 137
# The --kill-after just ensures the process is killed if it ignores the first signal
run_test "Kill after additional time" 124 \
    "$TIMEOUT_BIN" -k 1s 1s sleep 30

echo ""
echo "=== Verbose Mode Test ==="
echo ""

# Test 8: Verbose mode (just check it runs, output goes to stderr)
echo -ne "${YELLOW}Testing: Verbose mode${NC} ... "
if "$TIMEOUT_BIN" -v 1s sleep 5 2>&1 | grep -q "sending signal"; then
    echo -e "${GREEN}PASS${NC}"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}FAIL${NC}"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

echo ""
echo "=== Preserve Status Test ==="
echo ""

# Test 9: Preserve status
echo -ne "${YELLOW}Testing: Preserve status flag${NC} ... "
set +e
"$TIMEOUT_BIN" --preserve-status 5s -- sh -c 'exit 42' > /dev/null 2>&1
exit_code=$?
set -e
if [ $exit_code -eq 42 ]; then
    echo -e "${GREEN}PASS${NC} (exit: 42)"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}FAIL${NC} (expected: 42, got: $exit_code)"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

echo ""
echo "=== Floating Point Duration Test ==="
echo ""

# Test 10: Floating point duration
run_test "Floating point duration (0.5s)" 124 \
    "$TIMEOUT_BIN" 0.5s sleep 2

echo ""
echo "=== Edge Cases ==="
echo ""

# Test 11: Zero duration (should timeout immediately)
run_test "Zero duration" 124 \
    "$TIMEOUT_BIN" 0s sleep 1

echo ""
echo "=== Help and Version Tests ==="
echo ""

# Test 12: Help flag
echo -ne "${YELLOW}Testing: Help flag${NC} ... "
if "$TIMEOUT_BIN" --help > /dev/null 2>&1; then
    echo -e "${GREEN}PASS${NC}"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}FAIL${NC}"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# Test 13: Version flag
echo -ne "${YELLOW}Testing: Version flag${NC} ... "
if "$TIMEOUT_BIN" --version > /dev/null 2>&1; then
    echo -e "${GREEN}PASS${NC}"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}FAIL${NC}"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

echo ""
echo "======================================="
echo "Test Summary"
echo "======================================="
echo -e "Tests Passed: ${GREEN}$TESTS_PASSED${NC}"
echo -e "Tests Failed: ${RED}$TESTS_FAILED${NC}"
echo ""

if [ $TESTS_FAILED -eq 0 ]; then
    echo -e "${GREEN}All tests passed! ✓${NC}"
    exit 0
else
    echo -e "${RED}Some tests failed!${NC}"
    exit 1
fi
