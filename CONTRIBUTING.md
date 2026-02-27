# Contributing

## Git Hooks

This repository ships versioned git hooks under `.githooks/`.

Enable them once per clone:

```bash
git config core.hooksPath .githooks
```

Verify:

```bash
git config --get core.hooksPath
```

Expected output:

```text
.githooks
```

### Pre-commit behavior

The pre-commit hook runs:

```bash
cargo test
```

If tests fail, the commit is blocked.

Emergency local bypass (do not use for normal workflow):

```bash
SKIP_PRECOMMIT_TESTS=1 git commit ...
```
