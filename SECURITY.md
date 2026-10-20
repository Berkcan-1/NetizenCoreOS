# Security Policy

## Overview

NetizenCoreOS is an actively evolving operating system project. Security is an important design consideration, but the project must not be assumed to provide production-grade security or complete protection against malicious actors.

Features, mitigations, and security mechanisms may be incomplete, experimental, or subject to change.

We welcome responsible security research that helps identify and resolve vulnerabilities.

## Supported Versions

NetizenCoreOS is currently under active development.

| Version | Security Updates |
|---|---|
| Development branch (`main`) | Best effort |
| Tagged releases | Support depends on the release and maintainer availability |
| Older commits and experimental branches | No guaranteed security updates |

The table describes the intended support policy, not a guarantee that every version has received a security audit or security fixes.

Users should review the current repository status before relying on any build.

## Reporting a Vulnerability

**Please do not disclose suspected security vulnerabilities in public GitHub issues, discussions, or pull requests.**

Use one of the following methods:

1. **GitHub Private Vulnerability Reporting:** If enabled for this repository, use the private reporting option under the repository's **Security** tab.
2. **If private reporting is unavailable:** Open a public issue requesting a private security contact. Do not include exploit details, proof-of-concept code, sensitive logs, or other information that could expose the vulnerability.

Repository: https://github.com/Berkcan-1/NetizenCoreOS

Please do not publish a working exploit or detailed reproduction steps until the issue has been assessed and coordinated disclosure has been discussed with the maintainer.

## What to Include in a Report

Where possible, include:

- A concise description of the vulnerability
- The affected component and source file
- The affected commit, branch, or release
- The security impact and potential attack scenarios
- Steps to reproduce the issue in a controlled environment
- A minimal proof of concept, if necessary
- Relevant logs, error messages, and configuration details
- Any known mitigations or potential fixes

Please avoid including credentials, private keys, personal information, or unrelated sensitive data.

## Scope

Security reports relevant to NetizenCoreOS may include vulnerabilities in:

- Bootloader logic and boot-stage transitions
- CPU mode transitions and paging configuration
- Kernel memory management and memory isolation
- Privilege boundaries and access-control mechanisms
- Generic driver infrastructure and hardware interaction
- Filesystem implementations and storage handling
- Network stacks, protocol implementations, and TLS processing
- Input validation, parsing, and malformed-data handling
- Cryptographic implementations and key handling
- Security-sensitive configuration and inter-component communication
- Build scripts, tooling, or dependencies that compromise project integrity

This list is not exhaustive. A vulnerability must have a plausible security impact to qualify for security handling.

## Out of Scope

The following are generally outside the scope of a security vulnerability report unless they demonstrate a concrete security impact:

- Ordinary bugs without security implications
- Feature requests and architectural preferences
- Performance issues without a security consequence
- Theoretical concerns without a plausible attack scenario
- Issues caused solely by unsupported or incorrectly configured environments
- Missing features that have not been advertised as implemented

Experimental features and incomplete mitigations may still be relevant if their current behavior creates a security risk.

## Disclosure Process

The project aims to follow a responsible, coordinated disclosure process:

1. The maintainer reviews the report and assesses its validity and potential impact.
2. The reporter may be contacted for clarification or additional reproduction details.
3. If the vulnerability is confirmed, the maintainer evaluates possible fixes or mitigations.
4. A fix is tested where the available development environment permits.
5. The vulnerability may be publicly disclosed after coordination with the reporter and consideration of user risk.

Response and remediation times are not guaranteed. The project is independently developed, and the maintainer's availability may vary.

Please do not assume that a report has been received or accepted until acknowledgment is provided.

## Security Research Guidelines

When researching NetizenCoreOS:

- Perform testing only on systems you own or are explicitly authorized to test.
- Prefer isolated virtual machines and disposable test environments.
- Avoid actions that damage hardware, corrupt unrelated data, or disrupt third-party services.
- Do not access, modify, or disclose other people's data.
- Do not use vulnerabilities to establish persistence or deploy malicious software.
- Share only the information needed to demonstrate and assess the issue.

No bug bounty program or monetary reward is currently promised by this policy.

## Security Disclaimer

NetizenCoreOS is provided as an evolving software project. Security-related code, including cryptographic, networking, memory-management, and access-control components, must not be considered independently audited unless an audit is explicitly documented.

The presence of a security mechanism does not guarantee that it is correctly implemented or that the system is secure against all attack scenarios.

Users and integrators are responsible for evaluating the suitability of the project for their intended environment.

## Contact

For security-related reports, follow the reporting instructions above.

Do not post sensitive vulnerability details publicly while waiting for a private reporting channel to be established.

Thank you for helping make NetizenCoreOS safer.
