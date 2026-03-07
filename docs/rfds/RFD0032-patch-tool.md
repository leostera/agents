# RFD0032 - Patch Tool (Codex-Style Stripped Patch Format)

##Status
Draft

##Summary
Borg adds a first-class built-in tool for file edits: `Patch-apply`.

The tool accepts a stripped-down patch format (same core shape Codex uses) and applies it directly to the workspace with strict validation and bounded write semantics.

This gives us deterministic multi-file edits without forcing actors to use `CodeMode-executeCode` or ad-hoc shell commands for routine text changes.

##Motivation
Today, actors edit files through two expensive paths:
1. generate and run JavaScript (`CodeMode-executeCode`), or
2. run shell commands that mutate files.

Both paths work, but both are heavier than needed for most edits:
- too many tokens for simple file changes,
- more failure modes (script bugs, quoting bugs, shell escaping),
- weaker observability for exactly what changed,
- higher safety burden than a constrained patch grammar.

We want a fast, explicit, and auditable edit primitive that is easy for models to produce and easy for runtime to validate.

##Goals
1. Introduce a deterministic, text-only edit primitive for add/delete/update/move file operations.
2. Keep patch syntax simple enough for LLMs to produce reliably.
3. Enforce workspace-relative path safety and mutation policy checks.
4. Return structured results with per-file change summaries.
5. Support atomic multi-file patch application (all-or-nothing).

##Non-goals
1. Supporting full `git diff` / unified diff compatibility.
2. Editing binary files.
3. Allowing writes outside workspace/writable roots.
4. Preserving compatibility with legacy ad-hoc “edit file via code execution” flows as the primary path.

##Decisions
1. New built-in tool name: `Patch-apply`.
2. Primary argument shape:

   ```json
   {
     "patch": "*** Begin Patch\n*** Update File: src/main.ts\n@@\n-console.log(\"old\")\n+console.log(\"new\")\n*** End Patch"
   }
   ```

3. Patch grammar is Codex-style and intentionally stripped:

   ```text
   Patch := Begin { FileOp } End
   Begin := "*** Begin Patch" NEWLINE
   End := "*** End Patch" NEWLINE?
   FileOp := AddFile | DeleteFile | UpdateFile
   AddFile := "*** Add File: " path NEWLINE { "+" line NEWLINE }
   DeleteFile := "*** Delete File: " path NEWLINE
   UpdateFile := "*** Update File: " path NEWLINE [ MoveTo ] { Hunk }
   MoveTo := "*** Move to: " newPath NEWLINE
   Hunk := "@@" [ header ] NEWLINE { HunkLine } [ "*** End of File" NEWLINE ]
   HunkLine := (" " | "-" | "+") text NEWLINE
   ```

4. Path rules:
- paths must be relative,
- normalized path must stay under workspace root,
- no absolute paths,
- no traversal outside workspace (`..` escapes),
- same safety rules apply to `Move to`.

5. Apply semantics:
- parse -> validate -> compute target edits -> apply atomically,
- any parse/validation/hunk mismatch aborts the whole patch,
- no partial writes.

6. Structured result contract:
- success: includes `files_changed`, `added_lines`, `removed_lines`, and per-file operations,
- failure: machine-readable `code` + `message` + optional `line_number` when parse fails.

7. Safety and policy:
- mutating tool approval path is enforced the same way as other mutating tools,
- symlink traversal outside workspace is rejected,
- configurable caps: max patch bytes, max file ops, max changed lines.

8. Prompting default:
- coding actors should be instructed to prefer `Patch-apply` for file edits,
- `CodeMode-executeCode` remains for runtime tasks that are not static file patching.

##Before / After
Before:
- actor calls `CodeMode-executeCode` to read file, string-replace content, and write back.
- long payloads and more runtime variance.

After:
- actor emits a compact patch with only changed lines via `Patch-apply`.
- runtime validates and applies in one deterministic step.

Example:

```json
{
  "tool_name": "Patch-apply",
  "arguments": {
    "patch": "*** Begin Patch\n*** Add File: notes/todo.md\n+- [ ] wire Patch-apply into toolchain\n*** End Patch"
  }
}
```

Success result:

```json
{
  "ok": true,
  "result": {
    "files_changed": 1,
    "added_lines": 1,
    "removed_lines": 0,
    "changes": [
      {
        "op": "add",
        "path": "notes/todo.md"
      }
    ]
  }
}
```

##Reference-level design
###1. Parser + validator
Introduce a dedicated parser module (or crate) for the patch grammar.

Responsibilities:
1. parse patch text to AST,
2. produce precise line-numbered parse errors,
3. normalize/validate paths,
4. precompute all file operations before touching disk.

###2. Applicator
Apply AST to filesystem atomically:
1. read target files,
2. resolve hunks in order,
3. build final content in memory,
4. write all updates only if every file operation validates.

###3. Tool registration
Register `Patch-apply` in the exec toolchain alongside existing built-ins so actors can call it in normal turns.

###4. Telemetry + persistence
Persist:
1. raw patch input,
2. normalized operation summary,
3. outcome/error.

This supports replay/debugging and Stage/DevMode visualization.

###5. UI rendering
Stage/DevMode should render patch tool calls compactly:
- default collapsed summary (`N files, +A/-R`),
- expandable detail for each file op and hunk.

##Implementation plan
1. `crates/borg-exec`:
- add `Patch-apply` tool spec + transcoded handler in `tool_runner`.

2. `crates/borg-fs` (or new `crates/borg-patch`):
- implement parser, path validator, and atomic applicator.

3. `crates/borg-cli`:
- add `tools patch apply` command for local testing and fixtures.

4. `crates/borg-agent`:
- ensure tool result envelope carries structured patch summaries.

5. `apps/stage` and `apps/devmode`:
- improve tool-call rendering for patch summaries.

6. Tests:
- parser golden tests,
- apply success/failure tests,
- safety tests (`..`, absolute path, symlink escape),
- atomicity tests (one invalid hunk means zero files changed).

##Prior art
1. Codex `apply_patch`:
- same stripped grammar and “file-op envelope” model,
- proven to be model-friendly and easy to validate.

2. Traditional shell-based editing:
- flexible but unsafe/noisy for routine edits.

3. Full unified diff:
- powerful but unnecessary complexity for LLM-generated edits in our runtime.

##Risks
1. Hunk matching can fail frequently if context is too small.
2. Large patches may still be expensive without size caps.
3. Over-constrained grammar can force fallback to heavier tooling for edge cases.

##Rollout
1. Ship `Patch-apply` behind normal mutating-tool policy checks.
2. Update coding prompts to prefer `Patch-apply` for text edits.
3. Keep `CodeMode-executeCode` available for non-patch tasks.
4. Track usage and failure metrics, then tighten defaults around patch-first editing.

##Open questions
1. Should we support `dry_run: true` in v1?
2. Should `Move to` be allowed without content edits?
3. Should we auto-format touched files (opt-in) after successful patch apply?
