# Security Policy

## Reporting a vulnerability

If you believe you have found a security vulnerability in Scorium, **do not
open a public issue**. Report it privately to Fiw Labs through the project's
primary security contact (the repository's listed security contact, or the
GitHub "Report a vulnerability" flow if available).

Please include:

- A description of the issue and its impact.
- The smallest reproduction you can manage.
- The Scorium version or commit you tested against.
- Any fix you have considered.

We will acknowledge receipt as soon as we can and aim to keep reporters
informed through to a fix.

## Scope

Scorium's threat model is built around a deliberately small sandbox:

- The `script { }` escape hatch runs real Lua through an `mlua` state opened
  with **only** `math`, `string`, and `table`. There is no `io`, no `os`, no
  `package`, no `debug`, and no unrestricted environment-variable access.
- Loop execution is bounded by `RuntimeOptions::max_loop_iterations`.
- Per-`script` Lua instructions are bounded by `max_script_instructions`,
  enforced by an instruction-counting hook.
- Includes obey the host's `IncludePolicy`, including cycle detection and a
  default ban on `..` parent traversal.

In scope for this policy:

- Any way a `.scor` file can escape the sandbox -- reading or writing files,
  spawning processes, opening network connections, loading native modules,
  reading secret environment variables, or hitting unbounded resource use.
- Crashes, panics, or undefined behaviour reachable from parsing or
  evaluating untrusted input.
- Bypass of the loop or instruction budgets.
- Include path-traversal or cycle-detection bypass.

Out of scope:

- Issues that require the host application to have already registered a
  dangerous host function. The host owns what it exposes; that is the host's
  security decision, not Scorium's.
- Denial of service that requires running a `.scor` file the operator already
  trusts.

## Supported versions

Scorium is pre-1.0. Security fixes go onto the latest `main`. There are no
backport branches yet.

## Hardening guidance for hosts

- Start from `RuntimeOptions::default()` and tighten, not from open.
- Register host functions that are themselves side-effect-free or clearly
  safe; treat every registered function as a capability you are granting the
  config file.
- If you accept untrusted configuration, keep `IncludePolicy::default()`
  (cycle detection on, parent traversal off) and consider disabling includes
  entirely with `IncludePolicy { enabled: false, .. }`.
- Evaluate untrusted input in a process you would be willing to crash.

See [docs/SECURITY.md](./docs/SECURITY.md) for the detailed model.

---

**Legal notice.** This policy states initial project terms and operational
guidance. It has not been reviewed by a lawyer and is not a warranty of any
level of security.
