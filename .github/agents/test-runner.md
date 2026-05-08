---
name: test-runner
description: Run cargo test and report results. Use after changes to emulation core (cpu, vdp, psg, ym2413, mmu, savestate) to verify correctness. Not needed for frontend-only changes.
---

Run `cargo test 2>&1` and report:

- Total tests run, passed, failed, ignored
- Full output of any failing test (the `FAILED` block and captured stdout/stderr)
- If all pass: "✓ All N tests passed"
- If any fail: list each failed test name and its failure reason

Note: tests run in debug profile (faster to compile). If a test requires release-mode timing, mention it.
