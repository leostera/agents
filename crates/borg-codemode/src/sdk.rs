use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use tracing::{debug, warn};

const SDK_TYPES: &str = include_str!(concat!(env!("OUT_DIR"), "/borg_agent_sdk.d.ts"));
const ROOT_SDK_INTERFACE: &str = "BorgSdk";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiCapability {
    pub name: String,
    pub symbol: String,
    pub signature: String,
    pub type_definition: String,
    pub description: String,
}

#[derive(Debug, Clone)]
struct InterfaceMember {
    name: String,
    signature: String,
}

static CAPABILITIES: OnceLock<Vec<ApiCapability>> = OnceLock::new();

pub fn sdk_types() -> &'static str {
    SDK_TYPES
}

pub fn search_capabilities(query: &str) -> Vec<ApiCapability> {
    let catalog = CAPABILITIES.get_or_init(parse_capabilities);
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return catalog.clone();
    }

    let matches: Vec<ApiCapability> = catalog
        .iter()
        .filter(|item| {
            item.name.to_lowercase().contains(&needle)
                || item.signature.to_lowercase().contains(&needle)
                || item.description.to_lowercase().contains(&needle)
        })
        .cloned()
        .collect();

    if matches.is_empty() {
        catalog.clone()
    } else {
        matches
    }
}

fn parse_capabilities() -> Vec<ApiCapability> {
    let interfaces = parse_interfaces(SDK_TYPES);
    if interfaces.is_empty() {
        warn!(
            target: "borg_codemode",
            "failed to parse any interfaces from borg-agent-sdk types"
        );
        return vec![];
    }
    let declarations = parse_declarations(SDK_TYPES);

    let mut collected = Vec::new();
    let mut visited = HashSet::new();
    collect_capabilities(
        "Borg",
        ROOT_SDK_INTERFACE,
        &interfaces,
        &declarations,
        &mut visited,
        &mut collected,
    );

    debug!(
        target: "borg_codemode",
        capability_count = collected.len(),
        "parsed sdk capabilities from borg.d.ts"
    );
    collected
}

fn collect_capabilities(
    prefix: &str,
    interface_name: &str,
    interfaces: &HashMap<String, Vec<InterfaceMember>>,
    declarations: &HashMap<String, String>,
    visited: &mut HashSet<String>,
    out: &mut Vec<ApiCapability>,
) {
    let visit_key = format!("{}::{}", prefix, interface_name);
    if !visited.insert(visit_key) {
        return;
    }

    let Some(members) = interfaces.get(interface_name) else {
        return;
    };

    for member in members {
        let member_name = member.name.trim_end_matches('?');
        if is_function_signature(&member.signature) {
            let symbol = format!("{}.{}", prefix, member_name)
                .trim_start_matches("Borg.")
                .to_string();
            let type_definition = build_type_definition(&member.signature, declarations);
            out.push(ApiCapability {
                name: format!("{}.{}", prefix, member_name),
                symbol,
                signature: member.signature.clone(),
                type_definition,
                description: format!(
                    "SDK function {}.{} in borg.d.ts",
                    interface_name, member_name
                ),
            });
            continue;
        }

        if let Some(nested_interface) = extract_interface_name(&member.signature) {
            collect_capabilities(
                &format!("{}.{}", prefix, member_name),
                &nested_interface,
                interfaces,
                declarations,
                visited,
                out,
            );
        }
    }
}

fn parse_declarations(source: &str) -> HashMap<String, String> {
    let mut declarations = HashMap::new();

    for (name, members) in parse_interfaces(source) {
        let mut body = String::new();
        for member in members {
            body.push_str("  ");
            body.push_str(member.name.as_str());
            body.push_str(": ");
            body.push_str(member.signature.as_str());
            if !member.signature.trim_end().ends_with(';') {
                body.push(';');
            }
            body.push('\n');
        }
        declarations.insert(name.clone(), format!("interface {} {{\n{}}}", name, body));
    }

    let mut offset = 0;
    while let Some(relative_pos) = source[offset..].find("type ") {
        let start = offset + relative_pos;
        let rest = &source[start + "type ".len()..];
        let Some(eq_pos) = rest.find('=') else {
            break;
        };
        let name = rest[..eq_pos].trim();
        if name.is_empty() {
            break;
        }

        let after_eq = &rest[eq_pos + 1..];
        let Some(end_rel) = find_statement_end(after_eq) else {
            break;
        };
        let rhs = after_eq[..end_rel].trim();
        declarations.insert(name.to_string(), format!("type {} = {};", name, rhs));
        offset = start + "type ".len() + eq_pos + 1 + end_rel + 1;
    }

    declarations
}

fn parse_interfaces(source: &str) -> HashMap<String, Vec<InterfaceMember>> {
    let mut interfaces = HashMap::new();
    let mut offset = 0;

    while let Some(relative_pos) = source[offset..].find("interface ") {
        let start = offset + relative_pos;
        let rest = &source[start + "interface ".len()..];
        let name = rest
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .trim()
            .to_string();
        if name.is_empty() {
            break;
        }

        let Some(open_brace_relative) = source[start..].find('{') else {
            break;
        };
        let open_brace = start + open_brace_relative;
        let Some(close_brace) = find_matching_brace(source, open_brace) else {
            break;
        };

        let body = &source[open_brace + 1..close_brace];
        interfaces.insert(name, parse_interface_members(body));
        offset = close_brace + 1;
    }

    interfaces
}

fn parse_interface_members(body: &str) -> Vec<InterfaceMember> {
    let body = strip_ts_comments(body);
    let mut members = Vec::new();
    let mut statement = String::new();
    let mut paren_depth = 0_usize;
    let mut brace_depth = 0_usize;
    let mut bracket_depth = 0_usize;

    for ch in body.chars() {
        statement.push(ch);
        match ch {
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            ';' if paren_depth == 0 && brace_depth == 0 && bracket_depth == 0 => {
                if let Some(member) = parse_member_statement(statement.trim()) {
                    members.push(member);
                }
                statement.clear();
            }
            _ => {}
        }
    }

    members
}

fn strip_ts_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    let mut in_line_comment = false;
    let mut in_block_comment = false;

    while let Some(ch) = chars.next() {
        if in_line_comment {
            if ch == '\n' {
                in_line_comment = false;
                out.push('\n');
            }
            continue;
        }
        if in_block_comment {
            if ch == '*' && chars.peek() == Some(&'/') {
                let _ = chars.next();
                in_block_comment = false;
            }
            continue;
        }

        if ch == '/' && chars.peek() == Some(&'/') {
            let _ = chars.next();
            in_line_comment = true;
            continue;
        }
        if ch == '/' && chars.peek() == Some(&'*') {
            let _ = chars.next();
            in_block_comment = true;
            continue;
        }

        out.push(ch);
    }

    out
}

fn parse_member_statement(statement: &str) -> Option<InterfaceMember> {
    let normalized = statement.trim().trim_end_matches(';').trim();
    if normalized.is_empty() {
        return None;
    }

    let paren_pos = normalized.find('(');
    let colon_pos = normalized.find(':');

    // Method declarations, e.g. `ls(path?: string): Result`.
    if let Some(paren_pos) = paren_pos {
        let is_method = match colon_pos {
            Some(colon_pos) => paren_pos < colon_pos,
            None => true,
        };
        if is_method {
            let name = normalized[..paren_pos].trim().to_string();
            let signature = normalized[paren_pos..].trim().replace('\n', " ");
            return Some(InterfaceMember { name, signature });
        }
    }

    // Property declarations, e.g. `fetch: (url: string) => Result`.
    if let Some(colon_pos) = colon_pos {
        let name = normalized[..colon_pos].trim().to_string();
        let signature = normalized[colon_pos + 1..].trim().replace('\n', " ");
        return Some(InterfaceMember { name, signature });
    }

    None
}

fn find_matching_brace(source: &str, open_index: usize) -> Option<usize> {
    let mut depth = 0_usize;
    for (idx, ch) in source.char_indices().skip(open_index) {
        if ch == '{' {
            depth += 1;
            continue;
        }
        if ch == '}' {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(idx);
            }
        }
    }
    None
}

fn is_function_signature(signature: &str) -> bool {
    let trimmed = signature.trim();
    trimmed.contains("=>") || (trimmed.contains('(') && trimmed.contains(')'))
}

fn extract_interface_name(signature: &str) -> Option<String> {
    let trimmed = signature.trim();
    if trimmed.is_empty() || trimmed.contains('(') || trimmed.contains("=>") {
        return None;
    }
    let candidate = trimmed.trim_end_matches(';').trim();
    if candidate
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        Some(candidate.to_string())
    } else {
        None
    }
}

fn build_type_definition(signature: &str, declarations: &HashMap<String, String>) -> String {
    let mut required = HashSet::new();
    collect_required_declarations(signature, declarations, &mut required);
    let mut names = required.into_iter().collect::<Vec<_>>();
    names.sort();

    let mut parts = vec![format!("type Fn = {};", signature.trim())];
    for name in names {
        if let Some(decl) = declarations.get(&name) {
            parts.push(decl.clone());
        }
    }
    parts.join("\n\n")
}

fn collect_required_declarations(
    text: &str,
    declarations: &HashMap<String, String>,
    required: &mut HashSet<String>,
) {
    for ident in extract_type_identifiers(text) {
        if !declarations.contains_key(&ident) {
            continue;
        }
        if required.insert(ident.clone())
            && let Some(decl) = declarations.get(&ident)
        {
            collect_required_declarations(decl, declarations, required);
        }
    }
}

fn extract_type_identifiers(text: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut token = String::new();

    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            token.push(ch);
            continue;
        }
        push_identifier(&mut out, &token);
        token.clear();
    }
    push_identifier(&mut out, &token);
    out
}

fn push_identifier(set: &mut HashSet<String>, token: &str) {
    if token.is_empty() {
        return;
    }
    if token.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return;
    }
    if is_ignored_identifier(token) {
        return;
    }
    set.insert(token.to_string());
}

fn is_ignored_identifier(token: &str) -> bool {
    matches!(
        token,
        "type"
            | "interface"
            | "extends"
            | "string"
            | "number"
            | "boolean"
            | "unknown"
            | "null"
            | "undefined"
            | "Record"
            | "Array"
            | "ReadonlyArray"
            | "Promise"
            | "const"
            | "let"
            | "var"
            | "return"
            | "true"
            | "false"
    )
}

fn find_statement_end(source: &str) -> Option<usize> {
    let mut brace_depth = 0_usize;
    let mut paren_depth = 0_usize;
    let mut bracket_depth = 0_usize;
    for (idx, ch) in source.char_indices() {
        match ch {
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            ';' if brace_depth == 0 && paren_depth == 0 && bracket_depth == 0 => return Some(idx),
            _ => {}
        }
    }
    None
}
