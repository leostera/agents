use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use tracing::{debug, warn};

const SDK_TYPES: &str = include_str!(concat!(env!("OUT_DIR"), "/borg_agent_sdk.d.ts"));
const ROOT_SDK_INTERFACE: &str = "BorgSdk";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiCapability {
    pub name: String,
    pub signature: String,
    pub description: String,
}

#[derive(Debug, Clone)]
struct InterfaceMember {
    name: String,
    signature: String,
}

static CAPABILITIES: OnceLock<Vec<ApiCapability>> = OnceLock::new();

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
            target: "borg_rt",
            "failed to parse any interfaces from borg-agent-sdk types"
        );
        return vec![];
    }

    let mut collected = Vec::new();
    let mut visited = HashSet::new();
    collect_capabilities(
        "Borg",
        ROOT_SDK_INTERFACE,
        &interfaces,
        &mut visited,
        &mut collected,
    );

    debug!(
        target: "borg_rt",
        capability_count = collected.len(),
        "parsed sdk capabilities from borg.d.ts"
    );
    collected
}

fn collect_capabilities(
    prefix: &str,
    interface_name: &str,
    interfaces: &HashMap<String, Vec<InterfaceMember>>,
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
            out.push(ApiCapability {
                name: format!("{}.{}", prefix, member_name),
                signature: member.signature.clone(),
                description: format!("SDK API exposed by {}.{}", interface_name, member_name),
            });
            continue;
        }

        if let Some(nested_interface) = extract_interface_name(&member.signature) {
            collect_capabilities(
                &format!("{}.{}", prefix, member_name),
                &nested_interface,
                interfaces,
                visited,
                out,
            );
        }
    }
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

fn parse_member_statement(statement: &str) -> Option<InterfaceMember> {
    let normalized = statement.trim().trim_end_matches(';').trim();
    if normalized.is_empty() {
        return None;
    }

    if let Some(colon_pos) = normalized.find(':') {
        let name = normalized[..colon_pos].trim().to_string();
        let signature = normalized[colon_pos + 1..].trim().replace('\n', " ");
        return Some(InterfaceMember { name, signature });
    }

    if let Some(paren_pos) = normalized.find('(') {
        let name = normalized[..paren_pos].trim().to_string();
        let signature = normalized[paren_pos..].trim().replace('\n', " ");
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
