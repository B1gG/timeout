# Contributing to timeout

First off, thank you for considering contributing to timeout! 🎉

## Code of Conduct

This project adheres to a code of conduct. By participating, you are expected to uphold this code. Please be respectful and constructive in all interactions.

## How Can I Contribute?

### 🐛 Reporting Bugs

Before creating bug reports, please check the existing issues to avoid duplicates.

When filing a bug report, include:

- **Clear title and description**
- **Steps to reproduce** the behavior
- **Expected vs actual behavior**
- **Environment details**: OS, Rust version, timeout version
- **Command that failed** (if applicable)
- **Relevant logs or output**

### 💡 Suggesting Features

Feature suggestions are welcome! Please:

- **Check existing feature requests** first
- **Describe the use case** clearly
- **Explain why this would be useful** to most users
- **Consider implementation complexity**

### 🔧 Pull Requests

1. **Fork the repository** and create your branch from `master`
2. **Follow the existing code style**
3. **Add tests** for new functionality
4. **Update documentation** as needed
5. **Ensure all tests pass**: `./rust_timeout_tests.sh`
6. **Run clippy**: `cargo clippy`
7. **Format code**: `cargo fmt`

#### Pull Request Checklist

- [ ] Code follows the project style
- [ ] Tests added/updated
- [ ] Documentation updated
- [ ] All tests passing
- [ ] No clippy warnings
- [ ] Commit messages are clear
- [ ] CHANGELOG.md updated (if applicable)

## Development Setup

### Prerequisites

```bash
# Install Rust (1.70+)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone the repository
git clone https://github.com/b1gg/timeout.git
cd timeout
```

### Building

```bash
# Debug build
cargo build

# Release build
cargo build --release

# Run tests
cargo test

# Run the test suite
./rust_timeout_tests.sh

# Check code quality
cargo clippy

# Format code
cargo fmt
```

### Testing

```bash
# Run unit tests
cargo test

# Run integration tests
./rust_timeout_tests.sh

# Test on specific platform
cargo test --target x86_64-pc-windows-gnu  # Cross-compile example
```

### Platform-Specific Development

#### Unix (Linux/macOS/BSD)

- Full feature set available
- Test with `fork()`, signals, and resource limits

#### Windows

- Core features only (no signals)
- Test with async process management
- Requires Windows SDK for cross-compilation

## Code Style

- **Use `cargo fmt`** for formatting
- **Follow Rust conventions**: snake_case, etc.
- **Write clear comments** for complex logic
- **Use descriptive variable names**
- **Prefer explicit over implicit**

### Documentation Style

- Use `///` for public API documentation
- Use `//` for implementation comments
- Include examples in doc comments
- Keep lines under 100 characters

## Project Structure

```
src/
├── main.rs           # Entry point and shared utilities
├── args.rs           # CLI argument parsing
└── platform/
    ├── mod.rs        # Platform abstraction
    ├── unix.rs       # Unix implementation
    └── windows.rs    # Windows implementation
```

## Commit Message Guidelines

Use conventional commits format:

```
<type>(<scope>): <subject>

<body>

<footer>
```

**Types:**

- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation only
- `style`: Code style changes (formatting)
- `refactor`: Code refactoring
- `perf`: Performance improvements
- `test`: Adding/updating tests
- `chore`: Maintenance tasks

**Examples:**

```
feat(unix): add support for SIGRTMIN/SIGRTMAX

fix(windows): handle Ctrl+Break signal properly

docs: update platform compatibility matrix

test: add tests for floating-point durations
```

## Areas for Contribution

We especially welcome contributions in these areas:

### High Priority

- [ ] Windows Job Objects integration
- [ ] More comprehensive Windows testing
- [ ] Real-time signal support (Unix)
- [ ] Package manager distributions

### Medium Priority

- [ ] Configuration file support
- [ ] Async I/O redirection
- [ ] Multiple process monitoring
- [ ] Enhanced error messages

### Low Priority

- [ ] Colorscheme customization
- [ ] Progress indicators
- [ ] JSON output mode
- [ ] More shell integration examples

## Questions?

Feel free to:

- Open an issue for discussion
- Ask in GitHub Discussions
- Reach out to maintainers

## License

By contributing, you agree that your contributions will be licensed under the MIT License.

---

Thank you for contributing! 🚀
