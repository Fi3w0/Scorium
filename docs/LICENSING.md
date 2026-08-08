# Licensing

Scorium is **source-available**, not open source. This page explains what
that means in plain language. The binding documents are the legal files in
the repository root; this page is a friendly summary and does not override
them.

> **These are initial project terms that have not been reviewed by a lawyer.**
> Treat them as a good-faith statement of intent. Obtain professional legal
> review before relying on them -- especially for commercial use.

---

## The short version

| You want to... | Allowed without permission? |
| --- | --- |
| Read the source | Yes |
| Run it locally for personal use | Yes |
| Learn from it / teach with it | Yes |
| Use it for a hobby project | Yes |
| Modify it privately for testing | Yes |
| Fork on GitHub to send a pull request | Yes |
| Use it commercially | **No** -- needs a written agreement |
| Sell it | **No** |
| Publish it to crates.io / a registry | **No** |
| Ship binaries built from it | **No** |
| Maintain an independent renamed fork | **No** |

This table describes what *you*, a licensee, may do without a separate
agreement. It does not restrict @fi3w0 as copyright holder: official Scorium
crates are published to [crates.io](https://crates.io/crates/scorium-cli) by
the project owner. See [Official releases and status](../TRADEMARKS.md#official-releases-and-status).

---

## The license

Scorium is licensed under the [PolyForm Strict License 1.0.0](../LICENSE).

PolyForm Strict is a source-available license. It is **not** approved by the
Open Source Initiative (OSI) and is **not** one of the common permissive
licenses (MIT, Apache-2.0) or copyleft licenses (GPL). Calling Scorium "open
source" is inaccurate; the accurate terms are:

- **source-available** -- the source is published and readable;
- **community-source** / **publicly developed** -- development happens in the
  open and contributions are welcome;
- **free for noncommercial use** -- personal, educational, hobby, and local
  noncommercial use cost nothing.

---

## The legal documents

| File | Covers |
| --- | --- |
| [`LICENSE`](../LICENSE) | The PolyForm Strict License 1.0.0 itself. |
| [`CONTRIBUTION_PERMISSION.md`](../CONTRIBUTION_PERMISSION.md) | Extra permissions that let you fork, branch, modify, and prepare contributions. |
| [`CONTRIBUTOR_TERMS.md`](../CONTRIBUTOR_TERMS.md) | The rights a contributor grants so Scorium can be maintained, relicensed, and commercially licensed. |
| [`COMMERCIAL.md`](../COMMERCIAL.md) | How commercial use works and how to obtain a commercial license. |
| [`TRADEMARKS.md`](../TRADEMARKS.md) | The "Scorium" name, logo, and branding. |

---

## Why this model

Scorium is developed openly and welcomes community contributions, but its
owner (@fi3w0) keeps the right to:

- control official releases and distributions;
- offer a commercial license separately from the free noncommercial grant;
- prevent unofficial maintained forks, binary distributions, and package
  registrations that could fragment or impersonate the project.

This is why contributors agree to broad licensing terms (see
`CONTRIBUTOR_TERMS.md`): so a future license change or commercial agreement
cannot be blocked by a single contributor's copyright. Contributors keep
their copyright -- they grant a broad **licence**, not an assignment.

---

## Dependency licensing

Scorium itself is PolyForm Strict, but it depends on permissively-licensed
crates. The significant ones:

| Dependency | License | Why |
| --- | --- | --- |
| [`mlua`](https://crates.io/crates/mlua) | MIT | Embeds vendored Lua 5.4, the `script { }` sandbox. |
| [`miette`](https://crates.io/crates/miette) | Apache-2.0 | Diagnostic rendering with source excerpts. |
| [`thiserror`](https://crates.io/crates/thiserror) | MIT OR Apache-2.0 | Error-derive macros. |
| [`clap`](https://crates.io/crates/clap) | MIT OR Apache-2.0 | The `scorium` CLI. |
| [`serde`](https://crates.io/crates/serde) / [`serde_json`](https://crates.io/crates/serde_json) | MIT OR Apache-2.0 | Serialization support. |

Lua itself (vendored by `mlua`) is MIT. Linking these in does not change
Scorium's own license; it does mean the compiled artifacts include
permissively-licensed code under those crates' terms.

---

## Before you rely on this

If your use of Scorium is anything more than personal learning, education,
hobby, or local noncommercial testing -- or if you are unsure -- read the
legal files and, for commercial use, contact me directly at @fi3w0 for a written
agreement.
