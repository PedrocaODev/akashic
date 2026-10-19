use std::fs;
use std::path::Path;

use crate::frontmatter::{
    is_prohibited_reverse_relation, Frontmatter, ParsedDocument, RawLink,
};
use crate::VaultError;

/// Converts an arbitrary string into a kebab-case slug suitable for a fallback node ID.
pub fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            prev_dash = false;
        } else if c == '-' || c == '_' || c.is_whitespace() {
            if !prev_dash && !out.is_empty() {
                out.push('-');
                prev_dash = true;
            }
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "unnamed".to_string()
    } else {
        out
    }
}

/// Extracts the first H1 heading (`# Title`) from a Markdown body.
pub fn extract_h1(body: &str) -> Option<String> {
    for line in body.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("# ") {
            let title = rest.trim();
            if !title.is_empty() {
                return Some(title.to_string());
            }
        }
    }
    None
}

/// Extracts both standard markdown links `[text](target)` and `[[WikiLinks]]` from content.
pub fn extract_links(content: &str) -> Vec<RawLink> {
    let mut links = Vec::new();
    let bytes = content.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        // Check for [[WikiLink]]
        if i + 1 < len && bytes[i] == b'[' && bytes[i + 1] == b'[' {
            if let Some(end) = content[i + 2..].find("]]") {
                let inner = &content[i + 2..i + 2 + end];
                let (target, text) = if let Some((t, txt)) = inner.split_once('|') {
                    (t.trim(), txt.trim())
                } else {
                    (inner.trim(), inner.trim())
                };
                if !target.is_empty() {
                    links.push(RawLink {
                        target: target.to_string(),
                        text: text.to_string(),
                        is_wikilink: true,
                    });
                }
                i += 2 + end + 2;
                continue;
            }
        }

        // Check for Markdown link [text](target)
        if bytes[i] == b'[' {
            if let Some(text_end) = content[i + 1..].find(']') {
                let text_pos = i + 1 + text_end;
                if text_pos + 1 < len && bytes[text_pos + 1] == b'(' {
                    if let Some(target_end) = content[text_pos + 2..].find(')') {
                        let link_text = &content[i + 1..text_pos];
                        let link_target = &content[text_pos + 2..text_pos + 2 + target_end];
                        let target_clean = link_target.trim();
                        if !target_clean.is_empty() {
                            links.push(RawLink {
                                target: target_clean.to_string(),
                                text: link_text.to_string(),
                                is_wikilink: false,
                            });
                        }
                        i = text_pos + 2 + target_end + 1;
                        continue;
                    }
                }
            }
        }

        i += 1;
    }

    links
}

/// Splits a Markdown note into frontmatter YAML string and Markdown body string.
pub fn split_frontmatter(content: &str) -> Result<(&str, &str), VaultError> {
    let trimmed = content.trim_start_matches('\u{feff}'); // Handle UTF-8 BOM

    if !trimmed.starts_with("---") {
        return Err(VaultError::Parse(
            "Missing opening '---' frontmatter delimiter".to_string(),
        ));
    }

    let rest = &trimmed[3..];
    let after_first_line = match rest.find('\n') {
        Some(pos) => &rest[pos + 1..],
        None => {
            return Err(VaultError::Parse(
                "Missing closing '---' frontmatter delimiter".to_string(),
            ))
        }
    };

    let mut closing_pos = None;
    let mut current_offset = 0;

    for line in after_first_line.split_inclusive('\n') {
        let trimmed_line = line.trim_end_matches(&['\r', '\n'][..]).trim();
        if trimmed_line == "---" || trimmed_line == "..." {
            closing_pos = Some(current_offset);
            break;
        }
        current_offset += line.len();
    }

    let closing_offset = match closing_pos {
        Some(pos) => pos,
        None => {
            return Err(VaultError::Parse(
                "Missing closing '---' frontmatter delimiter".to_string(),
            ))
        }
    };

    let yaml_str = &after_first_line[..closing_offset];
    let body_start = match after_first_line[closing_offset..].find('\n') {
        Some(newline_pos) => closing_offset + newline_pos + 1,
        None => after_first_line.len(),
    };
    let body_str = &after_first_line[body_start..];

    Ok((yaml_str, body_str))
}

/// Parses frontmatter with strict validation, rejecting missing required fields (`id`, `type`, `status`)
/// and rejecting any prohibited reverse relation keys.
pub fn parse_frontmatter_strict(yaml: &str) -> Result<Frontmatter, VaultError> {
    let val: serde_yaml::Value = serde_yaml::from_str(yaml)
        .map_err(|e| VaultError::Parse(format!("Invalid YAML: {}", e)))?;

    let mapping = val
        .as_mapping()
        .ok_or_else(|| VaultError::Parse("Frontmatter must be a YAML mapping".to_string()))?;

    // Validate required fields
    let id_key = serde_yaml::Value::String("id".to_string());
    let id_valid = mapping
        .get(&id_key)
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .is_some();
    if !id_valid {
        return Err(VaultError::Parse("Missing required field 'id'".to_string()));
    }

    let type_key = serde_yaml::Value::String("type".to_string());
    let type_valid = mapping
        .get(&type_key)
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .is_some();
    if !type_valid {
        return Err(VaultError::Parse(
            "Missing required field 'type'".to_string(),
        ));
    }

    let status_key = serde_yaml::Value::String("status".to_string());
    let status_valid = mapping
        .get(&status_key)
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .is_some();
    if !status_valid {
        return Err(VaultError::Parse(
            "Missing required field 'status'".to_string(),
        ));
    }

    // Check prohibited reverse relations in top-level keys
    for k in mapping.keys() {
        if let Some(s) = k.as_str() {
            if is_prohibited_reverse_relation(s) {
                return Err(VaultError::Parse(format!(
                    "Prohibited reverse relation '{}' cannot be declared in frontmatter",
                    s
                )));
            }
        }
    }

    // Check prohibited reverse relations in relations list
    let rel_key = serde_yaml::Value::String("relations".to_string());
    if let Some(serde_yaml::Value::Sequence(seq)) = mapping.get(&rel_key) {
        for rel_item in seq {
            if let Some(rel_m) = rel_item.as_mapping() {
                if let Some(t_val) = rel_m.get(&serde_yaml::Value::String("type".to_string())) {
                    if let Some(t_str) = t_val.as_str() {
                        if is_prohibited_reverse_relation(t_str) {
                            return Err(VaultError::Parse(format!(
                                "Prohibited reverse relation '{}' cannot be declared in relations",
                                t_str
                            )));
                        }
                    }
                }
            }
        }
    }

    let frontmatter: Frontmatter = serde_yaml::from_value(val)
        .map_err(|e| VaultError::Parse(format!("Failed to deserialize frontmatter: {}", e)))?;

    Ok(frontmatter)
}

/// Parses a Markdown document string with fault-tolerant metadata extraction for graph construction and linting.
pub fn parse_markdown_str(
    root: &Path,
    path: &Path,
    content: &str,
) -> Result<ParsedDocument, VaultError> {
    let (yaml_str, body_str) = split_frontmatter(content)?;

    let val: serde_yaml::Value = serde_yaml::from_str(yaml_str)
        .map_err(|e| VaultError::Parse(format!("Invalid YAML frontmatter: {}", e)))?;

    let mut mapping = match val {
        serde_yaml::Value::Mapping(m) => m,
        _ => {
            return Err(VaultError::Parse(
                "Frontmatter must be a YAML mapping".to_string(),
            ))
        }
    };

    let rel_path = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unnamed");

    let mut missing_required_fields = Vec::new();

    // Check id
    let id_key = serde_yaml::Value::String("id".to_string());
    let id_opt = mapping
        .get(&id_key)
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if id_opt.is_none() {
        missing_required_fields.push("id".to_string());
        let fallback_id = slugify(stem);
        mapping.insert(id_key, serde_yaml::Value::String(fallback_id));
    }

    // Check type
    let type_key = serde_yaml::Value::String("type".to_string());
    let type_opt = mapping
        .get(&type_key)
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if type_opt.is_none() {
        missing_required_fields.push("type".to_string());
        mapping.insert(
            type_key,
            serde_yaml::Value::String("concept".to_string()),
        );
    }

    // Check status
    let status_key = serde_yaml::Value::String("status".to_string());
    let status_opt = mapping
        .get(&status_key)
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if status_opt.is_none() {
        missing_required_fields.push("status".to_string());
        mapping.insert(
            status_key,
            serde_yaml::Value::String("draft".to_string()),
        );
    }

    let mut prohibited_relations = Vec::new();

    // Check top-level prohibited reverse relations and remove from mapping so they don't pollute custom
    let prohibited_keys: Vec<serde_yaml::Value> = mapping
        .keys()
        .filter_map(|k| {
            if let Some(s) = k.as_str() {
                if is_prohibited_reverse_relation(s) {
                    return Some(k.clone());
                }
            }
            None
        })
        .collect();

    for k in prohibited_keys {
        if let Some(s) = k.as_str() {
            prohibited_relations.push(s.to_string());
        }
        mapping.remove(&k);
    }

    // Check prohibited reverse relations inside relations array
    let rel_key = serde_yaml::Value::String("relations".to_string());
    if let Some(serde_yaml::Value::Sequence(seq)) = mapping.get_mut(&rel_key) {
        let mut valid_relations = Vec::new();
        for rel_item in seq.drain(..) {
            let mut is_proh = false;
            if let Some(rel_m) = rel_item.as_mapping() {
                if let Some(t_val) = rel_m.get(&serde_yaml::Value::String("type".to_string())) {
                    if let Some(t_str) = t_val.as_str() {
                        if is_prohibited_reverse_relation(t_str) {
                            prohibited_relations.push(t_str.to_string());
                            is_proh = true;
                        }
                    }
                }
            }
            if !is_proh {
                valid_relations.push(rel_item);
            }
        }
        *seq = valid_relations;
    }

    let mut frontmatter: Frontmatter = serde_yaml::from_value(serde_yaml::Value::Mapping(mapping))
        .map_err(|e| VaultError::Parse(format!("Failed to deserialize frontmatter: {}", e)))?;

    // If title is missing or empty, extract from first H1 or file stem
    if frontmatter.title.trim().is_empty() {
        frontmatter.title = extract_h1(body_str).unwrap_or_else(|| stem.to_string());
    }

    let raw_links = extract_links(body_str);

    Ok(ParsedDocument {
        path: path.to_path_buf(),
        relative_path: rel_path,
        frontmatter,
        raw_yaml: Some(yaml_str.to_string()),
        body: body_str.to_string(),
        raw_links,
        missing_required_fields,
        prohibited_relations,
    })
}

/// Reads a Markdown note from disk and parses its frontmatter and content.
pub fn parse_markdown_file(root: &Path, path: &Path) -> Result<ParsedDocument, VaultError> {
    let content = fs::read_to_string(path)?;
    parse_markdown_str(root, path, &content)
}

/// Serializes frontmatter and body into Markdown content with standard YAML frontmatter.
pub fn serialize_document(frontmatter: &Frontmatter, body: &str) -> Result<String, VaultError> {
    let yaml = serde_yaml::to_string(frontmatter)
        .map_err(|e| VaultError::Serialization(e.to_string()))?;
    let trimmed_yaml = yaml.trim_start_matches("---\n").trim();
    let trimmed_body = body.trim();

    if trimmed_body.is_empty() {
        Ok(format!("---\n{}\n---\n", trimmed_yaml))
    } else {
        Ok(format!("---\n{}\n---\n\n{}\n", trimmed_yaml, trimmed_body))
    }
}
