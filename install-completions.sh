#!/usr/bin/env bash
# Install shell completions for timeout command

set -e

BINARY="${1:-./target/release/timeout}"

if [ ! -f "$BINARY" ]; then
    echo "Error: Binary not found at $BINARY"
    echo "Usage: $0 [path-to-timeout-binary]"
    exit 1
fi

echo "Installing shell completions for timeout..."
echo ""

# Detect shell
if [ -n "$ZSH_VERSION" ]; then
    SHELL_NAME="zsh"
elif [ -n "$BASH_VERSION" ]; then
    SHELL_NAME="bash"
else
    echo "Please specify your shell (bash, zsh, fish, powershell, elvish):"
    read -r SHELL_NAME
fi

case "$SHELL_NAME" in
    bash)
        COMPLETION_DIR="${BASH_COMPLETION_USER_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/bash-completion}/completions"
        mkdir -p "$COMPLETION_DIR"
        "$BINARY" --generate-completions bash > "$COMPLETION_DIR/timeout"
        echo "✓ Bash completions installed to: $COMPLETION_DIR/timeout"
        echo "  Restart your shell or run: source $COMPLETION_DIR/timeout"
        ;;
    
    zsh)
        # Try common zsh completion directories
        if [ -d "$HOME/.oh-my-zsh" ]; then
            COMPLETION_DIR="$HOME/.oh-my-zsh/completions"
        elif [ -d "/usr/local/share/zsh/site-functions" ]; then
            COMPLETION_DIR="/usr/local/share/zsh/site-functions"
        else
            COMPLETION_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/zsh/site-functions"
        fi
        
        mkdir -p "$COMPLETION_DIR"
        "$BINARY" --generate-completions zsh > "$COMPLETION_DIR/_timeout"
        echo "✓ Zsh completions installed to: $COMPLETION_DIR/_timeout"
        echo "  Restart your shell or run: autoload -U compinit && compinit"
        ;;
    
    fish)
        COMPLETION_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/fish/completions"
        mkdir -p "$COMPLETION_DIR"
        "$BINARY" --generate-completions fish > "$COMPLETION_DIR/timeout.fish"
        echo "✓ Fish completions installed to: $COMPLETION_DIR/timeout.fish"
        echo "  Completions will be available in new fish sessions"
        ;;
    
    powershell)
        echo "PowerShell completion script:"
        echo ""
        "$BINARY" --generate-completions powershell
        echo ""
        echo "To install, add the above to your PowerShell profile"
        echo "Run: notepad \$PROFILE"
        ;;
    
    elvish)
        COMPLETION_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/elvish/lib"
        mkdir -p "$COMPLETION_DIR"
        "$BINARY" --generate-completions elvish > "$COMPLETION_DIR/timeout-completions.elv"
        echo "✓ Elvish completions installed to: $COMPLETION_DIR/timeout-completions.elv"
        echo "  Add to your rc.elv: use timeout-completions"
        ;;
    
    *)
        echo "Unknown shell: $SHELL_NAME"
        echo "Supported shells: bash, zsh, fish, powershell, elvish"
        exit 1
        ;;
esac

echo ""
echo "Done! 🎉"
