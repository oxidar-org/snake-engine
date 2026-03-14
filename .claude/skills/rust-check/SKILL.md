---
name: rust-check
description: Run fmt, clippy, and tests after writing Rust code. Use after any code changes to verify correctness.
---

Run the following checks in order. Stop at the first failure and fix the issue before continuing.

1. **Format**: `cargo fmt`
2. **Lint**: `cargo clippy -- -D warnings`
3. **Test**: `cargo test`

If any step fails, fix the problem and re-run from that step. Do not skip steps.
