<!--
Sync Impact Report
===================
- Version change: N/A → 1.0.0 (initial adoption)
- Added principles:
  - I. Code Quality
  - II. Testing Standards
  - III. User Experience Consistency
  - IV. Performance Requirements
- Added sections:
  - Plugin Contract
  - Development Workflow
  - Governance
- Removed sections: none
- Templates requiring updates:
  - .specify/templates/plan-template.md — ✅ no updates needed (generic)
  - .specify/templates/spec-template.md — ✅ no updates needed (generic)
  - .specify/templates/tasks-template.md — ✅ no updates needed (generic)
  - .specify/templates/commands/*.md — ✅ no command files exist
- Follow-up TODOs: none
-->

# haproxy-spoa-hub-plugin-maxmind Constitution

## Core Principles

### I. Code Quality

All code MUST be clear, minimal, and maintainable. Specific rules:

- Every exported function, type, and constant MUST have a clear,
  single responsibility.
- No dead code, unused imports, or commented-out blocks are permitted
  in the main branch.
- Linters and static analysis MUST pass with zero warnings before
  merge. Suppressing warnings requires a documented justification
  in the code.
- Dependencies MUST be kept to the minimum required. Adding a new
  dependency requires explicit justification (no convenience-only
  imports).
- Error handling MUST be explicit — never silently swallow errors.
  Every error path MUST either propagate the error with context or
  log it at an appropriate level.

### II. Testing Standards

All changes MUST be validated by automated tests. Specific rules:

- Bug fixes MUST include a regression test that fails before the fix
  and passes after.
- New features MUST include tests that cover the primary success path
  and at least one error/edge case.
- Tests MUST be deterministic — no reliance on network access,
  external services, or time-sensitive behavior without explicit
  mocking or test fixtures.
- Integration tests that require a MaxMind database MUST use a
  well-known test fixture (bundled or downloaded in CI) so results
  are reproducible.
- Test names MUST describe the scenario and expected outcome, not
  the implementation detail being exercised.

### III. User Experience Consistency

The plugin MUST behave predictably and conform to haproxy-spoa-hub
conventions so operators have a uniform experience. Specific rules:

- Configuration options MUST follow the naming and structure
  conventions established by haproxy-spoa-hub and its existing
  plugins.
- Error messages and log output MUST be actionable: include what
  went wrong, which configuration or input caused it, and what the
  operator can do to resolve it.
- Behavior changes that affect operators (config keys, defaults,
  log formats) MUST be treated as breaking changes and documented
  in release notes.
- The plugin MUST fail fast on startup with a clear message if
  required configuration (e.g., database path) is missing or
  invalid, rather than failing silently at request time.

### IV. Performance Requirements

The plugin MUST not degrade HAProxy request processing. Specific
rules:

- MaxMind database lookups MUST use an in-memory reader (e.g.,
  mmdbReader) — no per-request disk I/O.
- Database reloads (e.g., after a GeoIP update) MUST be performed
  without blocking concurrent lookups. Use atomic pointer swaps
  or equivalent lock-free techniques.
- The plugin MUST NOT allocate unbounded memory. Connection-scoped
  allocations MUST be bounded and released promptly.
- Latency overhead per SPOA message MUST remain below 1 ms at p99
  under normal operation. Any change that risks exceeding this
  budget MUST include benchmark results.

## Plugin Contract

The plugin integrates with haproxy-spoa-hub as a message handler.
The following constraints define the integration surface:

- The plugin MUST implement the handler interface defined by
  haproxy-spoa-hub without wrapping it in unnecessary abstractions.
- SPOA message arguments and return values MUST use the types
  expected by HAProxy (string, integer, IPv4/IPv6) — no custom
  serialization.
- The plugin MUST register itself with a stable, documented message
  name. Renaming the message is a breaking change.

## Development Workflow

- All changes enter via pull request against `main`.
- PRs MUST pass CI (lint, test, build) before merge.
- Commits MUST use conventional commit format
  (`feat:`, `fix:`, `docs:`, `chore:`, `perf:`, `test:`,
  `refactor:`).
- After pushing changes, the CI pipeline MUST be monitored until
  completion. Review comments (including automated reviewers)
  MUST be addressed before merge.

## Governance

This constitution is the authoritative source of project standards.
All pull requests and code reviews MUST verify compliance with
these principles.

- **Amendments**: Any change to this constitution MUST be proposed
  as a pull request, reviewed, and approved before merge. The
  amendment MUST include a version bump and updated Sync Impact
  Report.
- **Versioning**: Constitution versions follow semantic versioning.
  MAJOR for principle removals or redefinitions, MINOR for new
  principles or material expansions, PATCH for clarifications.
- **Compliance review**: At least once per release cycle, verify
  that active code and CI configuration comply with current
  principles. Non-compliance MUST be tracked as issues.

**Version**: 1.0.0 | **Ratified**: 2026-04-09 | **Last Amended**: 2026-04-09
