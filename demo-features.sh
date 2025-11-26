#!/usr/bin/env bash
# Examples demonstrating colored output and shell completions

echo "================================"
echo "Colored Output Examples"
echo "================================"
echo ""

# Example 1: Verbose output with colors
echo "1. Verbose mode (colored status messages):"
echo "   $ timeout -v 1s sleep 5"
./target/release/timeout -v 1s sleep 5 2>&1
echo ""

# Example 2: Error message with colors
echo "2. Error handling (colored error messages):"
echo "   $ timeout 5s nonexistent_command"
./target/release/timeout 5s nonexistent_command 2>&1
echo ""

# Example 3: Warning message
echo "3. Platform warnings (colored warnings):"
echo "   $ timeout --cpu-limit 10 5s echo test"
./target/release/timeout --cpu-limit 10 5s echo test 2>&1 || true
echo ""

echo "================================"
echo "Shell Completion Examples"
echo "================================"
echo ""

# Example 4: Generate completions
echo "4. Generate bash completions:"
echo "   $ timeout --generate-completions bash | head -10"
./target/release/timeout --generate-completions bash | head -10
echo "   ..."
echo ""

echo "5. Generate zsh completions:"
echo "   $ timeout --generate-completions zsh | head -10"
./target/release/timeout --generate-completions zsh | head -10
echo "   ..."
echo ""

echo "================================"
echo "Installation"
echo "================================"
echo ""
echo "To install shell completions:"
echo "  ./install-completions.sh"
echo ""
echo "For manual installation:"
echo "  Bash:       timeout --generate-completions bash > /usr/local/etc/bash_completion.d/timeout"
echo "  Zsh:        timeout --generate-completions zsh > ~/.zsh/completions/_timeout"
echo "  Fish:       timeout --generate-completions fish > ~/.config/fish/completions/timeout.fish"
echo "  PowerShell: timeout --generate-completions powershell >> \$PROFILE"
echo ""
