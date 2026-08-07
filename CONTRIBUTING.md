# Contributing to Scorium

Thank you for your interest in contributing to Scorium. This document covers
the practical side of contributing. The legal side is covered in
[CONTRIBUTION_PERMISSION.md](./CONTRIBUTION_PERMISSION.md) and
[CONTRIBUTOR_TERMS.md](./CONTRIBUTOR_TERMS.md); please read both before
opening a pull request.

## Project status and scope

Scorium is a young project. The core language (lexer, parser, AST, typed
values, diagnostics), the sandboxed evaluator (control flow, includes, host
registry), the schema validator, and the canonical formatter are implemented
and tested. See [docs/ROADMAP.md](./docs/ROADMAP.md) for what is planned.

Good first contributions:

- Bug fixes with a regression test.
- New test fixtures that exercise documented behavior.
- Documentation improvements.
- Performance work on the lexer/parser hot paths.
- Clearer diagnostic messages.

Please open an issue before large feature work, so the design can be agreed
before you invest time in it.

## How to contribute

1. Fork the repository on GitHub (this is explicitly permitted by
   [CONTRIBUTION_PERMISSION.md](./CONTRIBUTION_PERMISSION.md)).
2. Create a branch whose primary purpose is preparing a contribution back to
   the official project.
3. Make your change. Match the surrounding style; keep modules focused.
4. Add or update tests. Do not weaken tests to make the build pass.
5. Make sure everything passes (see "Verification" below).
6. Open a pull request describing what changed and why.

## Verification

Before submitting, run from the repository root:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --workspace --all-features
```

All four must pass. The CI workflow enforces the same.

## Code quality expectations

- **Stable Rust.** The workspace targets a minimum Rust version declared in
  the root `Cargo.toml`. Avoid nightly-only features.
- **No `unwrap()` in library code** unless the invariant is provable and
  documented; prefer explicit error handling.
- **No unsafe code** unless strictly necessary and documented.
- **No type-error suppression** (`as any` does not apply; never use
  `unwrap_or` to hide a real failure, etc.).
- **Documented public APIs.** Every public item carries a doc comment.
- **Small modules.** Prefer focused files over oversized ones.
- **Clippy-clean and rustfmt-clean.**
- **Deterministic tests.** No network, no reliance on wall-clock time, no
  hidden filesystem assumptions beyond a writable temp directory.

## Tests

Tests live alongside the code (`crates/*/tests/`) and as a doctest in
`scorium-schema`. Use the fixture files under `tests/fixtures/` for readable
integration tests. When you add a feature, add a test that fails without it.

## Commit messages and history

Write clear commit messages in the imperative mood ("add color darken
method", not "added"). Keep history readable; the maintainers may squash or
rebase before merging.

## Licensing of contributions

By submitting a pull request, you agree to
[CONTRIBUTOR_TERMS.md](./CONTRIBUTOR_TERMS.md), which grants @fi3w0 the
rights needed to maintain, distribute, relicense, and commercially license
Scorium. Scorium is source-available under PolyForm Strict 1.0.0; it is **not
OSI-approved open source**, and contributors must be comfortable with that.

## Conduct

Be kind and technical. Assume good faith. Critique the work, not the person.

---

**Legal notice.** These contribution instructions are initial project terms.
They have not been reviewed by a lawyer.
