# Security Policy

## Supported development line

Ghost Talk is pre-1.0 software. Security fixes are maintained on the current `0.1.0` development line; repository revisions are tracked separately in `REVISION`.

## Reporting a vulnerability

Please report security vulnerabilities privately rather than opening a public issue. Include the affected commit/revision, reproduction steps, impact, and any proof-of-concept material needed to validate the report. Do not include private keys, seed material, production credentials, or third-party personal data.

Until a dedicated security mailbox is published, use GitHub's private vulnerability reporting for the repository when available. If private reporting is unavailable, open a minimal public issue asking the maintainers for a private security contact without disclosing vulnerability details.

## Handling expectations

Maintainers should acknowledge a report, reproduce it against the identified revision, keep exploit details private while remediation is in progress, and document the fix in `CHANGELOG.md` when disclosure is safe. Security-sensitive changes must pass the complete quality gates and the release gate before a production release.
