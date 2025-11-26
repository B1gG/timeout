# New Features

## 🎨 Colored Output

The timeout command now uses colored output for better visual feedback:

- **Errors** (Red): Critical errors and failures
- **Warnings** (Yellow): Non-critical issues and platform limitations
- **Info/Notes** (Cyan): Informational messages
- **Timeout** (Red): Timeout expiration messages
- **Success** (Green): Successful operations

### Example Output

```bash
# Verbose mode with colored output
$ timeout -v 1s sleep 10
Note: orphan prevention (PR_SET_PDEATHSIG) not available on macOS
Timeout: sending signal SIGTERM to command 'sleep'
```

Colors are automatically disabled when output is redirected to a file or pipe.

## 🔧 Shell Completion

Shell completion scripts are now available for:

- **Bash**
- **Zsh**
- **Fish**
- **PowerShell**
- **Elvish**

### Generate Completions

```bash
# Generate bash completions
timeout --generate-completions bash > /usr/local/etc/bash_completion.d/timeout

# Generate zsh completions
timeout --generate-completions zsh > /usr/local/share/zsh/site-functions/_timeout

# Generate fish completions
timeout --generate-completions fish > ~/.config/fish/completions/timeout.fish
```

### Easy Installation

Use the provided installation script:

```bash
./install-completions.sh
```

The script will automatically:

- Detect your shell
- Install completions to the appropriate directory
- Provide instructions for activation

### Features

With completions installed, you get:

- **Flag completion**: Tab-complete all command flags (`--signal`, `--kill-after`, etc.)
- **Signal names**: Complete signal names (SIGTERM, SIGKILL, etc.)
- **Duration formats**: Suggestions for time formats
- **Command completion**: Complete executable names from PATH

### Examples

```bash
$ timeout --<TAB>
--cpu-limit        --kill-after       --preserve-status  --status
--detect-stopped   --mem-limit        --signal           --verbose
--foreground       --no-notify        --help             --version

$ timeout -s SIG<TAB>
SIGTERM  SIGKILL  SIGINT  SIGHUP  SIGQUIT  SIGUSR1  SIGUSR2

$ timeout 5s <TAB>
# Shows available commands from PATH
```

## 🐛 Bug Fixes

### macOS Signal Handling

Fixed an issue where `killpg()` would fail with `ESRCH` on macOS:

- Now automatically falls back to `kill()` when process group signaling fails
- Maintains compatibility across all Unix platforms
- No user-visible changes required

## 📦 Dependencies

New dependencies added:

- **owo-colors** (4.0): Zero-allocation colored output
- **clap_complete** (4.5): Shell completion generation

Both are lightweight and have minimal impact on binary size.

## 🚀 Performance

The colored output implementation has:

- **Zero allocations** for color codes
- **No runtime overhead** when colors are disabled
- **Compile-time optimization** for color formatting

## 🔄 Compatibility

All existing functionality remains unchanged:

- ✅ All exit codes remain the same
- ✅ All command-line flags work identically
- ✅ Output format unchanged (except colors)
- ✅ 100% backward compatible

## 📝 Usage Examples

### Colored Verbose Output

```bash
# See colored status messages
timeout -v 5s long-running-command

# Colors in warnings
timeout --cpu-limit 10 30s compute-task
Warning: Running on macOS. Some features may have limited support.
```

### Shell Completions in Action

```bash
# After installing completions
timeout --sig<TAB>nal TERM 30s command
                  ↑
            Auto-completed!
```

## 🔮 Future Enhancements

Potential improvements being considered:

1. **Colorscheme customization**: Environment variables to override colors
2. **JSON output mode**: Machine-readable structured output
3. **Progress indicators**: Optional progress bars for long timeouts
4. **Completion contexts**: Context-aware command suggestions

## 📊 Binary Size Impact

With new features:

- **Release build**: ~2.3 MB (was ~2.1 MB)
- **Stripped**: ~800 KB (was ~750 KB)
- **Negligible impact**: <10% size increase for significant UX improvements
