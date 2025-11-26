# Security Policy

## Supported Versions

Currently supported versions:

| Version | Supported          |
| ------- | ------------------ |
| 1.0.x   | :white_check_mark: |

## Reporting a Vulnerability

If you discover a security vulnerability in timeout, please report it responsibly.

### How to Report

**DO NOT** open a public issue for security vulnerabilities.

Instead, please report security issues by emailing:

- **Email**: youremail@example.com
- **Subject**: [SECURITY] timeout vulnerability report

### What to Include

Please include the following in your report:

1. **Description** of the vulnerability
2. **Steps to reproduce** the issue
3. **Potential impact** (privilege escalation, DoS, etc.)
4. **Affected versions** (if known)
5. **Suggested fix** (if available)
6. **Your contact information** for follow-up

### Response Timeline

- **Initial Response**: Within 48 hours
- **Status Update**: Within 7 days
- **Fix Timeline**: Varies by severity
  - Critical: 7-14 days
  - High: 14-30 days
  - Medium: 30-60 days
  - Low: 60-90 days

### Security Update Process

1. Vulnerability reported and confirmed
2. Fix developed and tested privately
3. Security advisory drafted
4. Patch released with advisory
5. Public disclosure after patch availability

## Security Considerations

### Process Execution

This tool executes arbitrary commands with user privileges. Security considerations:

- **No privilege escalation**: timeout runs commands with the same privileges as the user
- **Signal handling**: Proper cleanup of child processes
- **Resource limits**: Can restrict CPU/memory on Unix systems
- **Input validation**: Arguments are properly parsed and validated

### Platform-Specific Concerns

#### Unix/Linux

- Uses `fork()` for process creation
- Signals can be intercepted by child processes
- Resource limits rely on `setrlimit()`
- Death signal (PR_SET_PDEATHSIG) prevents orphans on Linux

#### Windows

- Uses Windows process APIs
- Limited signal support (Ctrl+C/Break)
- Process termination uses TerminateProcess
- No resource limit support

### Known Limitations

1. **Signal races**: Brief race window between fork and exec
2. **Orphan processes**: Possible on non-Linux Unix systems without proper cleanup
3. **Terminal control**: TTY handling may have edge cases
4. **Resource limits**: Not available on Windows

### Best Practices

When using timeout in production:

- **Validate inputs**: Ensure command arguments are properly sanitized
- **Set resource limits**: Use `--cpu-limit` and `--mem-limit` on Unix
- **Handle signals**: Be aware of signal propagation behavior
- **Monitor logs**: Use `--verbose` for debugging in production
- **Test thoroughly**: Especially on target platforms

## Disclosure Policy

We follow **coordinated disclosure**:

1. Vulnerability reported privately
2. Fix developed and tested
3. Security advisory prepared
4. Patch released
5. Public disclosure 7 days after patch (or when actively exploited)

## Security Updates

Security updates will be:

- Released as patch versions (1.0.x)
- Announced in CHANGELOG.md
- Tagged with [SECURITY] in release notes
- Documented in GitHub Security Advisories

## Contact

For general issues:

- Use GitHub Issues (non-security)

---

Thank you for helping keep timeout secure! 🔒
