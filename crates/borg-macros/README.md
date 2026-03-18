# borg-macros

`borg-macros` contains the proc macros used across the workspace.

It provides:

- `#[suite]`
- `#[eval]`
- `#[grade]`
- `#[derive(Agent)]`
- `#[derive(Tool)]`

These macros expand to the typed runtime in `borg-agent` and `borg-evals`.
