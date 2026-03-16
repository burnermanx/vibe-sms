---
name: build-check
description: Run cargo check, cargo clippy, and cargo build --release. Report all errors and warnings with file:line references. Use after any non-trivial code change to catch issues before committing.
---

Run the following commands in order and report all output:

1. `cargo check 2>&1`
2. `cargo clippy 2>&1`
3. `cargo build --release 2>&1`

For each command, show:
- Whether it succeeded or failed
- All `error[...]` lines with their locations
- All `warning[...]` lines with their locations (excluding the final "N warnings generated" summary line)

If `cargo check` fails, skip the remaining steps and report only those errors — fixing compile errors takes priority.

End with a one-line summary: "✓ Clean" if zero errors and zero warnings, otherwise "✗ N error(s), M warning(s)" with a bullet list of what needs fixing.
