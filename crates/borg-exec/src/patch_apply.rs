#![allow(dead_code)]

use anyhow::{Context, Result, anyhow};
use serde::Serialize;
use serde_json::json;
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

const PATCH_BEGIN: &str = "*** Begin Patch";
const PATCH_END: &str = "*** End Patch";
const PATCH_ADD_FILE: &str = "*** Add File: ";
const PATCH_DELETE_FILE: &str = "*** Delete File: ";
const PATCH_UPDATE_FILE: &str = "*** Update File: ";
const PATCH_MOVE_TO: &str = "*** Move to: ";
const PATCH_END_OF_FILE: &str = "*** End of File";
const PATCH_MAX_BYTES: usize = 512 * 1024;
const PATCH_MAX_FILE_OPS: usize = 128;
const PATCH_MAX_CHANGED_LINES: usize = 20_000;

#[derive(Debug, Clone, Serialize)]
pub struct PatchApplyResult {
    pub files_changed: usize,
    pub added_lines: usize,
    pub removed_lines: usize,
    pub changes: Vec<PatchChangeSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PatchChangeSummary {
    pub op: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub moved_to: Option<String>,
    pub added_lines: usize,
    pub removed_lines: usize,
}

#[derive(Debug, Clone)]
enum PatchOp {
    Add {
        path: String,
        lines: Vec<String>,
    },
    Delete {
        path: String,
    },
    Update {
        path: String,
        move_to: Option<String>,
        hunks: Vec<PatchHunk>,
    },
}

#[derive(Debug, Clone)]
struct PatchHunk {
    lines: Vec<PatchHunkLine>,
}

#[derive(Debug, Clone)]
enum PatchHunkLineKind {
    Context,
    Add,
    Remove,
}

#[derive(Debug, Clone)]
struct PatchHunkLine {
    kind: PatchHunkLineKind,
    text: String,
}

#[derive(Debug, Clone)]
struct ParsedPatch {
    ops: Vec<PatchOp>,
}

#[derive(Debug, Clone)]
struct ParsedLine {
    number: usize,
    value: String,
}

#[derive(Debug)]
struct ParseCursor {
    lines: Vec<ParsedLine>,
    index: usize,
}

impl ParseCursor {
    fn new(raw: &str) -> Self {
        let lines = raw
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .split('\n')
            .enumerate()
            .map(|(idx, line)| ParsedLine {
                number: idx + 1,
                value: line.to_string(),
            })
            .collect::<Vec<_>>();
        Self { lines, index: 0 }
    }

    fn peek(&self) -> Option<&ParsedLine> {
        self.lines.get(self.index)
    }

    fn next(&mut self) -> Option<ParsedLine> {
        let out = self.lines.get(self.index).cloned();
        if out.is_some() {
            self.index += 1;
        }
        out
    }
}

#[derive(Debug, Clone)]
struct PatchPlan {
    writes: Vec<(PathBuf, String)>,
    deletes: Vec<PathBuf>,
    result: PatchApplyResult,
}

pub fn apply_patch_payload(raw_patch: &str) -> Result<PatchApplyResult> {
    if raw_patch.trim().is_empty() {
        return Err(patch_err(
            "patch.validation.empty",
            "patch is required",
            None,
        ));
    }
    if raw_patch.len() > PATCH_MAX_BYTES {
        return Err(anyhow!(
            json!({
                "code": "patch.validation.too_large",
                "message": "patch exceeds max bytes",
                "max_bytes": PATCH_MAX_BYTES
            })
            .to_string()
        ));
    }

    let parsed = parse_patch(raw_patch)?;
    if parsed.ops.is_empty() {
        return Err(patch_err(
            "patch.validation.no_ops",
            "patch has no file operations",
            None,
        ));
    }
    if parsed.ops.len() > PATCH_MAX_FILE_OPS {
        return Err(anyhow!(
            json!({
                "code": "patch.validation.too_many_file_ops",
                "message": "patch exceeds max file ops",
                "max_file_ops": PATCH_MAX_FILE_OPS
            })
            .to_string()
        ));
    }
    let changed_lines = parsed
        .ops
        .iter()
        .map(|op| match op {
            PatchOp::Add { lines, .. } => lines.len(),
            PatchOp::Delete { .. } => 0,
            PatchOp::Update { hunks, .. } => hunks.iter().map(|h| h.lines.len()).sum(),
        })
        .sum::<usize>();
    if changed_lines > PATCH_MAX_CHANGED_LINES {
        return Err(anyhow!(
            json!({
                "code": "patch.validation.too_many_changed_lines",
                "message": "patch exceeds max changed lines",
                "max_changed_lines": PATCH_MAX_CHANGED_LINES
            })
            .to_string()
        ));
    }

    let root = std::env::current_dir()?.canonicalize()?;
    let plan = build_patch_plan(&root, parsed)?;
    apply_patch_plan(&plan)?;
    Ok(plan.result)
}

fn patch_err(code: &str, message: &str, line_number: Option<usize>) -> anyhow::Error {
    let mut payload = serde_json::Map::new();
    payload.insert("code".to_string(), json!(code));
    payload.insert("message".to_string(), json!(message));
    if let Some(line_number) = line_number {
        payload.insert("line_number".to_string(), json!(line_number));
    }
    anyhow!(serde_json::Value::Object(payload).to_string())
}

fn parse_patch(raw_patch: &str) -> Result<ParsedPatch> {
    let mut cursor = ParseCursor::new(raw_patch);

    let begin = cursor.next().ok_or_else(|| {
        patch_err(
            "patch.parse.unexpected_eof",
            "missing *** Begin Patch",
            None,
        )
    })?;
    if begin.value != PATCH_BEGIN {
        return Err(patch_err(
            "patch.parse.expected_begin",
            "expected *** Begin Patch",
            Some(begin.number),
        ));
    }

    let mut ops = Vec::new();
    loop {
        let Some(line) = cursor.peek().cloned() else {
            return Err(patch_err(
                "patch.parse.unexpected_eof",
                "missing *** End Patch",
                None,
            ));
        };
        if line.value == PATCH_END {
            let _ = cursor.next();
            break;
        }
        if line.value.starts_with(PATCH_ADD_FILE) {
            ops.push(parse_add_file(&mut cursor)?);
            continue;
        }
        if line.value.starts_with(PATCH_DELETE_FILE) {
            ops.push(parse_delete_file(&mut cursor)?);
            continue;
        }
        if line.value.starts_with(PATCH_UPDATE_FILE) {
            ops.push(parse_update_file(&mut cursor)?);
            continue;
        }
        return Err(anyhow!(
            json!({
                "code": "patch.parse.unknown_directive",
                "message": "unexpected directive",
                "line_number": line.number,
                "line": line.value
            })
            .to_string()
        ));
    }

    while let Some(line) = cursor.next() {
        if !line.value.trim().is_empty() {
            return Err(anyhow!(
                json!({
                    "code": "patch.parse.trailing_content",
                    "message": "unexpected content after *** End Patch",
                    "line_number": line.number
                })
                .to_string()
            ));
        }
    }

    Ok(ParsedPatch { ops })
}

fn parse_add_file(cursor: &mut ParseCursor) -> Result<PatchOp> {
    let header = cursor.next().ok_or_else(|| {
        patch_err(
            "patch.parse.unexpected_eof",
            "missing add-file header",
            None,
        )
    })?;
    let path = header
        .value
        .strip_prefix(PATCH_ADD_FILE)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    if path.is_empty() {
        return Err(patch_err(
            "patch.parse.invalid_path",
            "missing add-file path",
            Some(header.number),
        ));
    }

    let mut lines = Vec::new();
    while let Some(line) = cursor.peek() {
        if is_patch_boundary(&line.value) {
            break;
        }
        let row = cursor.next().ok_or_else(|| {
            patch_err(
                "patch.parse.unexpected_eof",
                "unexpected eof parsing add-file body",
                None,
            )
        })?;
        if !row.value.starts_with('+') {
            return Err(patch_err(
                "patch.parse.invalid_add_line",
                "add file body lines must start with +",
                Some(row.number),
            ));
        }
        lines.push(row.value[1..].to_string());
    }

    if lines.is_empty() {
        return Err(anyhow!(
            json!({
                "code": "patch.parse.empty_add_file",
                "message": "add file requires at least one + line",
                "path": path
            })
            .to_string()
        ));
    }

    Ok(PatchOp::Add { path, lines })
}

fn parse_delete_file(cursor: &mut ParseCursor) -> Result<PatchOp> {
    let header = cursor.next().ok_or_else(|| {
        patch_err(
            "patch.parse.unexpected_eof",
            "missing delete-file header",
            None,
        )
    })?;
    let path = header
        .value
        .strip_prefix(PATCH_DELETE_FILE)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    if path.is_empty() {
        return Err(patch_err(
            "patch.parse.invalid_path",
            "missing delete-file path",
            Some(header.number),
        ));
    }
    Ok(PatchOp::Delete { path })
}

fn parse_update_file(cursor: &mut ParseCursor) -> Result<PatchOp> {
    let header = cursor.next().ok_or_else(|| {
        patch_err(
            "patch.parse.unexpected_eof",
            "missing update-file header",
            None,
        )
    })?;
    let path = header
        .value
        .strip_prefix(PATCH_UPDATE_FILE)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    if path.is_empty() {
        return Err(patch_err(
            "patch.parse.invalid_path",
            "missing update-file path",
            Some(header.number),
        ));
    }

    let mut move_to = None;
    if let Some(line) = cursor.peek()
        && line.value.starts_with(PATCH_MOVE_TO)
    {
        let move_line = cursor
            .next()
            .ok_or_else(|| patch_err("patch.parse.unexpected_eof", "missing move-to path", None))?;
        let path = move_line
            .value
            .strip_prefix(PATCH_MOVE_TO)
            .map(str::trim)
            .unwrap_or_default()
            .to_string();
        if path.is_empty() {
            return Err(patch_err(
                "patch.parse.invalid_path",
                "missing move-to path",
                Some(move_line.number),
            ));
        }
        move_to = Some(path);
    }

    let mut hunks = Vec::new();
    while let Some(line) = cursor.peek() {
        if line.value == PATCH_END_OF_FILE {
            let _ = cursor.next();
            break;
        }
        if is_patch_boundary(&line.value) {
            break;
        }
        if !line.value.starts_with("@@") {
            return Err(patch_err(
                "patch.parse.invalid_hunk_header",
                "expected @@ hunk header",
                Some(line.number),
            ));
        }
        let _ = cursor.next();
        let mut hunk_lines = Vec::new();
        while let Some(hline) = cursor.peek() {
            if hline.value == PATCH_END_OF_FILE
                || is_patch_boundary(&hline.value)
                || hline.value.starts_with("@@")
            {
                break;
            }
            let row = cursor.next().ok_or_else(|| {
                patch_err(
                    "patch.parse.unexpected_eof",
                    "unexpected eof in hunk body",
                    None,
                )
            })?;
            let first = row.value.chars().next().ok_or_else(|| {
                patch_err(
                    "patch.parse.invalid_hunk_line",
                    "empty hunk line",
                    Some(row.number),
                )
            })?;
            let kind = match first {
                ' ' => PatchHunkLineKind::Context,
                '+' => PatchHunkLineKind::Add,
                '-' => PatchHunkLineKind::Remove,
                _ => {
                    return Err(patch_err(
                        "patch.parse.invalid_hunk_line",
                        "hunk line must begin with space/+/-",
                        Some(row.number),
                    ));
                }
            };
            hunk_lines.push(PatchHunkLine {
                kind,
                text: row.value[1..].to_string(),
            });
        }
        if hunk_lines.is_empty() {
            return Err(patch_err(
                "patch.parse.empty_hunk",
                "hunk has no body lines",
                None,
            ));
        }
        hunks.push(PatchHunk { lines: hunk_lines });
    }

    if hunks.is_empty() && move_to.is_none() {
        return Err(anyhow!(
            json!({
                "code": "patch.parse.empty_update",
                "message": "update requires hunks or move-to",
                "path": path
            })
            .to_string()
        ));
    }

    Ok(PatchOp::Update {
        path,
        move_to,
        hunks,
    })
}

fn is_patch_boundary(line: &str) -> bool {
    line == PATCH_END
        || line.starts_with(PATCH_ADD_FILE)
        || line.starts_with(PATCH_DELETE_FILE)
        || line.starts_with(PATCH_UPDATE_FILE)
}

fn normalize_relative_path(raw: &str) -> Result<PathBuf> {
    let value = raw.trim();
    if value.is_empty() {
        return Err(patch_err("patch.path.invalid", "path is empty", None));
    }
    let path = Path::new(value);
    if path.is_absolute() {
        return Err(anyhow!(
            json!({
                "code": "patch.path.absolute_not_allowed",
                "message": "absolute paths are not allowed",
                "path": value
            })
            .to_string()
        ));
    }

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(segment) => normalized.push(segment),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(anyhow!(
                    json!({
                        "code": "patch.path.traversal_not_allowed",
                        "message": "path escapes workspace",
                        "path": value
                    })
                    .to_string()
                ));
            }
        }
    }
    if normalized.as_os_str().is_empty() {
        return Err(anyhow!(
            json!({
                "code": "patch.path.invalid",
                "message": "invalid normalized path",
                "path": value
            })
            .to_string()
        ));
    }
    Ok(normalized)
}

fn ensure_path_safe(root: &Path, relative_path: &Path) -> Result<PathBuf> {
    let mut probe = root.to_path_buf();
    for component in relative_path.components() {
        if let Component::Normal(segment) = component {
            probe.push(segment);
            if let Ok(meta) = std::fs::symlink_metadata(&probe)
                && meta.file_type().is_symlink()
            {
                return Err(anyhow!(
                    json!({
                        "code": "patch.path.symlink_not_allowed",
                        "message": "symlink path segment is not allowed",
                        "path": relative_path.display().to_string()
                    })
                    .to_string()
                ));
            }
        }
    }
    Ok(root.join(relative_path))
}

fn build_patch_plan(root: &Path, parsed: ParsedPatch) -> Result<PatchPlan> {
    let mut writes = Vec::new();
    let mut deletes = Vec::new();
    let mut summaries = Vec::new();
    let mut added_total = 0usize;
    let mut removed_total = 0usize;

    let mut write_paths = HashSet::new();
    let mut delete_paths = HashSet::new();

    for op in parsed.ops {
        match op {
            PatchOp::Add { path, lines } => {
                let rel = normalize_relative_path(&path)?;
                let abs = ensure_path_safe(root, &rel)?;
                if abs.exists() {
                    return Err(anyhow!(
                        json!({
                            "code": "patch.apply.add_target_exists",
                            "message": "add file target already exists",
                            "path": path
                        })
                        .to_string()
                    ));
                }
                if !write_paths.insert(abs.clone()) {
                    return Err(anyhow!(
                        json!({
                            "code": "patch.apply.duplicate_target",
                            "message": "duplicate write target in patch",
                            "path": path
                        })
                        .to_string()
                    ));
                }
                added_total += lines.len();
                summaries.push(PatchChangeSummary {
                    op: "add".to_string(),
                    path,
                    moved_to: None,
                    added_lines: lines.len(),
                    removed_lines: 0,
                });
                writes.push((abs, lines.join("\n")));
            }
            PatchOp::Delete { path } => {
                let rel = normalize_relative_path(&path)?;
                let abs = ensure_path_safe(root, &rel)?;
                if !abs.exists() {
                    return Err(anyhow!(
                        json!({
                            "code": "patch.apply.delete_missing",
                            "message": "delete target missing",
                            "path": path
                        })
                        .to_string()
                    ));
                }
                if !delete_paths.insert(abs.clone()) {
                    return Err(anyhow!(
                        json!({
                            "code": "patch.apply.duplicate_delete",
                            "message": "duplicate delete target in patch",
                            "path": path
                        })
                        .to_string()
                    ));
                }
                summaries.push(PatchChangeSummary {
                    op: "delete".to_string(),
                    path,
                    moved_to: None,
                    added_lines: 0,
                    removed_lines: 0,
                });
                deletes.push(abs);
            }
            PatchOp::Update {
                path,
                move_to,
                hunks,
            } => {
                let src_rel = normalize_relative_path(&path)?;
                let src_abs = ensure_path_safe(root, &src_rel)?;
                if !src_abs.exists() {
                    return Err(anyhow!(
                        json!({
                            "code": "patch.apply.update_missing",
                            "message": "update source missing",
                            "path": path
                        })
                        .to_string()
                    ));
                }

                let src_text = std::fs::read_to_string(&src_abs)
                    .with_context(|| format!("failed reading `{}`", src_abs.display()))?;
                let (source_lines, source_trailing_newline) = split_lines_preserving_eof(&src_text);
                let updated_lines = if hunks.is_empty() {
                    source_lines.clone()
                } else {
                    apply_hunks(&source_lines, &hunks)?
                };

                let mut added = 0usize;
                let mut removed = 0usize;
                for hunk in &hunks {
                    for line in &hunk.lines {
                        match line.kind {
                            PatchHunkLineKind::Add => added += 1,
                            PatchHunkLineKind::Remove => removed += 1,
                            PatchHunkLineKind::Context => {}
                        }
                    }
                }
                added_total += added;
                removed_total += removed;

                let dst_rel = if let Some(dst) = &move_to {
                    normalize_relative_path(dst)?
                } else {
                    src_rel.clone()
                };
                let dst_abs = ensure_path_safe(root, &dst_rel)?;
                if move_to.is_some() && dst_abs != src_abs && dst_abs.exists() {
                    return Err(anyhow!(
                        json!({
                            "code": "patch.apply.move_target_exists",
                            "message": "move target already exists",
                            "path": dst_rel.display().to_string()
                        })
                        .to_string()
                    ));
                }

                if !write_paths.insert(dst_abs.clone()) {
                    return Err(anyhow!(
                        json!({
                            "code": "patch.apply.duplicate_target",
                            "message": "duplicate write target in patch",
                            "path": dst_rel.display().to_string()
                        })
                        .to_string()
                    ));
                }

                if move_to.is_some() && dst_abs != src_abs && delete_paths.insert(src_abs.clone()) {
                    deletes.push(src_abs.clone());
                }

                summaries.push(PatchChangeSummary {
                    op: if move_to.is_some() {
                        "move_update".to_string()
                    } else {
                        "update".to_string()
                    },
                    path,
                    moved_to: move_to,
                    added_lines: added,
                    removed_lines: removed,
                });
                writes.push((
                    dst_abs,
                    join_lines_preserving_eof(updated_lines, source_trailing_newline),
                ));
            }
        }
    }

    Ok(PatchPlan {
        writes,
        deletes,
        result: PatchApplyResult {
            files_changed: summaries.len(),
            added_lines: added_total,
            removed_lines: removed_total,
            changes: summaries,
        },
    })
}

fn apply_patch_plan(plan: &PatchPlan) -> Result<()> {
    for path in &plan.deletes {
        if !path.exists() {
            continue;
        }
        std::fs::remove_file(path)
            .with_context(|| format!("failed deleting `{}`", path.display()))?;
    }
    for (path, content) in &plan.writes {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed creating parent `{}`", parent.display()))?;
        }
        std::fs::write(path, content)
            .with_context(|| format!("failed writing `{}`", path.display()))?;
    }
    Ok(())
}

fn apply_hunks(source: &[String], hunks: &[PatchHunk]) -> Result<Vec<String>> {
    let mut output = Vec::new();
    let mut cursor = 0usize;

    for hunk in hunks {
        let expected = hunk
            .lines
            .iter()
            .filter_map(|line| match line.kind {
                PatchHunkLineKind::Context | PatchHunkLineKind::Remove => Some(line.text.as_str()),
                PatchHunkLineKind::Add => None,
            })
            .collect::<Vec<_>>();

        let start = find_hunk_start(source, cursor, &expected).ok_or_else(|| {
            patch_err(
                "patch.apply.hunk_not_found",
                "failed to match hunk context in source file",
                None,
            )
        })?;

        output.extend_from_slice(&source[cursor..start]);
        let mut src_idx = start;
        for line in &hunk.lines {
            match line.kind {
                PatchHunkLineKind::Context => {
                    let current = source.get(src_idx).ok_or_else(|| {
                        patch_err(
                            "patch.apply.hunk_context_oob",
                            "context line beyond source bounds",
                            None,
                        )
                    })?;
                    if current != &line.text {
                        return Err(patch_err(
                            "patch.apply.hunk_context_mismatch",
                            "context line mismatch",
                            None,
                        ));
                    }
                    output.push(line.text.clone());
                    src_idx += 1;
                }
                PatchHunkLineKind::Remove => {
                    let current = source.get(src_idx).ok_or_else(|| {
                        patch_err(
                            "patch.apply.hunk_remove_oob",
                            "remove line beyond source bounds",
                            None,
                        )
                    })?;
                    if current != &line.text {
                        return Err(patch_err(
                            "patch.apply.hunk_remove_mismatch",
                            "remove line mismatch",
                            None,
                        ));
                    }
                    src_idx += 1;
                }
                PatchHunkLineKind::Add => output.push(line.text.clone()),
            }
        }
        cursor = src_idx;
    }

    output.extend_from_slice(&source[cursor..]);
    Ok(output)
}

fn find_hunk_start(source: &[String], from: usize, expected: &[&str]) -> Option<usize> {
    if expected.is_empty() {
        return Some(from);
    }
    let max = source.len().saturating_sub(expected.len());
    for start in from..=max {
        if expected.iter().enumerate().all(|(offset, needle)| {
            source.get(start + offset).map(|line| line.as_str()) == Some(*needle)
        }) {
            return Some(start);
        }
    }
    None
}

fn split_lines_preserving_eof(input: &str) -> (Vec<String>, bool) {
    let trailing_newline = input.ends_with('\n');
    let mut lines = input
        .split('\n')
        .map(ToOwned::to_owned)
        .collect::<Vec<String>>();
    if trailing_newline {
        let _ = lines.pop();
    }
    (lines, trailing_newline)
}

fn join_lines_preserving_eof(lines: Vec<String>, trailing_newline: bool) -> String {
    if lines.is_empty() {
        return if trailing_newline {
            "\n".to_string()
        } else {
            String::new()
        };
    }
    let mut output = lines.join("\n");
    if trailing_newline {
        output.push('\n');
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(test_name: &str) -> Result<PathBuf> {
        let root =
            std::env::temp_dir().join(format!("borg-patch-{}-{}", test_name, uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root)?;
        Ok(root)
    }

    #[test]
    fn parses_add_update_delete() -> Result<()> {
        let patch = format!(
            "{PATCH_BEGIN}\n*** Add File: notes/a.txt\n+hello\n*** Update File: notes/b.txt\n@@\n-old\n+new\n*** Delete File: notes/c.txt\n{PATCH_END}"
        );
        let parsed = parse_patch(&patch)?;
        assert_eq!(parsed.ops.len(), 3);
        Ok(())
    }

    #[test]
    fn applies_update_patch() -> Result<()> {
        let root = tmp_dir("update")?;
        let file = root.join("src/main.ts");
        std::fs::create_dir_all(file.parent().ok_or_else(|| anyhow!("missing parent"))?)?;
        std::fs::write(&file, "console.log(\"old\")\n")?;

        let patch = format!(
            "{PATCH_BEGIN}\n*** Update File: src/main.ts\n@@\n-console.log(\"old\")\n+console.log(\"new\")\n{PATCH_END}"
        );
        let parsed = parse_patch(&patch)?;
        let plan = build_patch_plan(&root, parsed)?;
        apply_patch_plan(&plan)?;
        assert_eq!(std::fs::read_to_string(&file)?, "console.log(\"new\")\n");
        Ok(())
    }

    #[test]
    fn rejects_path_traversal() {
        let err = normalize_relative_path("../outside.txt").expect_err("expected traversal error");
        assert!(err.to_string().contains("patch.path.traversal_not_allowed"));
    }
}
