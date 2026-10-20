# Contributing to NetizenCoreOS

Thank you for your interest in contributing to NetizenCoreOS.

NetizenCoreOS is an operating system development project focused on low-level systems programming, a custom boot process, and a Rust-based kernel. Contributions of all sizes are welcome, from bug reports and documentation improvements to kernel development and testing.

## Table of Contents

- [Getting Started](#getting-started)
- [Development Principles](#development-principles)
- [Types of Contributions](#types-of-contributions)
- [Reporting Bugs](#reporting-bugs)
- [Proposing Changes](#proposing-changes)
- [Code Guidelines](#code-guidelines)
- [Testing](#testing)
- [Documentation](#documentation)
- [Security Issues](#security-issues)
- [Community Standards](#community-standards)

## Getting Started

1. Fork the repository.
2. Clone your fork locally.
3. Create a dedicated branch for your changes.
4. Review the existing source code and documentation before making modifications.
5. Build and test your changes in an appropriate development environment.
6. Submit a pull request describing your changes.

Repository: https://github.com/Berkcan-1/NetizenCoreOS

Example:

```bash
git clone https://github.com/YOUR_USERNAME/NetizenCoreOS.git
cd NetizenCoreOS
git checkout -b feature/your-feature
```

Replace `YOUR_USERNAME` with your GitHub username.

Before contributing, review the repository's README and the relevant documentation in the project directories. Build instructions and supported development environments may change as the project evolves.

## Development Principles

Contributions should follow these principles:

- **Correctness first:** Prefer reliable and predictable behavior over premature optimization.
- **Minimal dependencies:** Avoid introducing unnecessary external dependencies, especially in low-level components.
- **Memory safety:** Use Rust's safety guarantees wherever possible. Unsafe code must have a clear justification.
- **Explicit behavior:** Prefer understandable control flow and well-defined interfaces.
- **Security by design:** Consider privilege boundaries, input validation, memory safety, and failure handling.
- **Maintainability:** Keep implementations modular and avoid unnecessary complexity.
- **Honest documentation:** Clearly distinguish implemented functionality from planned or experimental features.

NetizenCoreOS is an evolving project. Proposed changes should respect its existing architecture while allowing room for future improvements.

## Types of Contributions

You can contribute in several ways:

- Kernel features and bug fixes
- Bootloader and boot-process improvements
- Memory management and paging
- Hardware abstraction and generic driver infrastructure
- Filesystem and storage support
- Networking and protocol implementations
- Security architecture and access-control mechanisms
- Graphics, input handling, and rendering
- Build tooling and development utilities
- Testing, debugging, and performance analysis
- Documentation and examples

Large architectural changes should be discussed with the maintainer before implementation.

## Reporting Bugs

When reporting a bug, include as much relevant information as possible:

- A clear and descriptive title
- The affected component
- The commit, branch, or version being tested
- Your host operating system and development environment
- The emulator or hardware configuration used
- Steps to reproduce the issue
- Expected behavior and actual behavior
- Relevant logs, error messages, screenshots, or minimal reproductions

For boot failures, kernel panics, or graphics issues, include relevant serial output or emulator logs when available.

Please search existing issues before opening a new one.

Do not include credentials, private keys, personal data, or sensitive security details in public reports.

## Proposing Changes

Pull requests should:

1. Explain the problem being solved or the feature being introduced.
2. Describe the implementation and any relevant design decisions.
3. Identify affected components.
4. Include testing steps and their results.
5. Update documentation when behavior or interfaces change.
6. Mention known limitations or untested scenarios.

Keep pull requests focused on a single feature or related group of changes whenever practical.

Avoid mixing unrelated refactoring, formatting changes, and new functionality in one pull request.

For substantial changes to the kernel architecture, boot protocol, memory layout, security model, or public interfaces, open a discussion or issue first.

## Code Guidelines

### Rust

- Follow standard Rust conventions and idiomatic patterns.
- Use `rustfmt` to format Rust code.
- Use meaningful names for types, functions, modules, and variables.
- Prefer explicit error handling over silent failure.
- Avoid unnecessary `unwrap()` and `expect()` calls in production paths, particularly where failures could compromise system stability.
- Document important invariants and non-obvious design decisions.
- Minimize `unsafe` blocks and explain their safety requirements.
- Do not suppress compiler warnings without a clear reason.

### Assembly and Low-Level Code

- Keep bootloader and architecture-specific code focused and understandable.
- Document calling conventions, register assumptions, memory layouts, and execution-mode transitions where necessary.
- Validate assumptions about CPU features, memory regions, and hardware state.
- Avoid relying on undocumented behavior when a defined alternative is available.

### Dependencies

- Justify new dependencies, particularly in core system components.
- Consider dependency size, maintenance status, licensing, and security implications.
- Avoid vendoring large dependencies without a clear need.
- Never commit secrets, private keys, access tokens, or unrelated generated artifacts.

### Formatting and Commits

Use descriptive commit messages, for example:

```text
fix: handle invalid memory region
feat: improve generic driver initialization
docs: clarify boot process
test: add paging validation cases
```

These prefixes are suggestions, not a mandatory commit convention.

## Testing

Test changes as thoroughly as the available environment permits.

Depending on the component, useful checks may include:

- Successful compilation
- Rust formatting and lint checks
- Booting the system in QEMU
- Verifying bootloader-to-kernel handoff
- Testing keyboard, mouse, graphics, and memory behavior
- Checking filesystem operations
- Testing network protocol handling
- Regression testing existing functionality
- Reviewing error paths and malformed inputs

Where applicable, run:

```bash
cargo fmt -- --check
cargo check
cargo test
```

Run these commands from the relevant Rust workspace or package directory. Some checks or tests may not be available for every target, `no_std` component, or boot configuration. Report any limitations instead of claiming that unexecuted tests passed.

Do not test experimental kernel code on important physical hardware without understanding the potential risks.

## Documentation

Documentation is part of the implementation.

Update the relevant README files when changes affect build instructions, architecture, supported features, configuration, or known limitations.

For complex changes, document:

- The purpose of the component
- Its interfaces and dependencies
- Important invariants
- Failure conditions
- Testing procedures
- Current limitations

Avoid describing planned functionality as completed functionality.

## Security Issues

**Do not report exploitable security vulnerabilities through public issues or pull requests.**

Follow the instructions in [`SECURITY.md`](SECURITY.md) to report security concerns privately.

Security-sensitive contributions may require additional review before they can be merged.

## Community Standards

Be respectful, constructive, and open to technical discussion.

- Critique implementations, not contributors.
- Explain technical disagreements with evidence where possible.
- Accept that proposed changes may require revisions.
- Respect project scope and existing design constraints.
- Do not submit malicious code, intentionally weakened security checks, or undocumented backdoors.

Contributions are reviewed based on their correctness, clarity, maintainability, compatibility, and suitability for the project.

## Final Notes

By submitting a contribution, you confirm that you have the right to submit the work under the repository's applicable license.

Thank you for helping improve NetizenCoreOS.
