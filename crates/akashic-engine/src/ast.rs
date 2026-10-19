use crate::store::{CodeEdge, CodeSymbol, ConfidenceTier};
use akashic_sanitize::SanitizerProfile;
use regex::Regex;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportedLanguage {
    Java,
    Kotlin,
    Rust,
    Python,
    TypeScript,
    JavaScript,
    Go,
    C,
    Cpp,
}

impl SupportedLanguage {
    pub fn from_path(path: &str) -> Option<Self> {
        if path.ends_with(".java") { Some(Self::Java) }
        else if path.ends_with(".kt") { Some(Self::Kotlin) }
        else if path.ends_with(".rs") { Some(Self::Rust) }
        else if path.ends_with(".py") { Some(Self::Python) }
        else if path.ends_with(".ts") || path.ends_with(".tsx") { Some(Self::TypeScript) }
        else if path.ends_with(".js") || path.ends_with(".jsx") { Some(Self::JavaScript) }
        else if path.ends_with(".go") { Some(Self::Go) }
        else if path.ends_with(".c") || path.ends_with(".h") { Some(Self::C) }
        else if path.ends_with(".cpp") || path.ends_with(".hpp") || path.ends_with(".cc") { Some(Self::Cpp) }
        else { None }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "java" => Some(Self::Java),
            "kotlin" => Some(Self::Kotlin),
            "rust" => Some(Self::Rust),
            "python" => Some(Self::Python),
            "typescript" | "ts" => Some(Self::TypeScript),
            "javascript" | "js" => Some(Self::JavaScript),
            "go" => Some(Self::Go),
            "c" => Some(Self::C),
            "cpp" | "c++" => Some(Self::Cpp),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Java => "Java",
            Self::Kotlin => "Kotlin",
            Self::Rust => "Rust",
            Self::Python => "Python",
            Self::TypeScript => "TypeScript",
            Self::JavaScript => "JavaScript",
            Self::Go => "Go",
            Self::C => "C",
            Self::Cpp => "Cpp",
        }
    }
}

pub struct ExtractedFileGraph {
    pub file_path: String,
    pub language: SupportedLanguage,
    pub symbols: Vec<CodeSymbol>,
    pub edges: Vec<CodeEdge>,
}

fn get_text<'a>(node: &tree_sitter::Node<'a>, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or("")
}

fn get_first_identifier<'a>(node: &tree_sitter::Node<'a>, source: &'a str) -> Option<&'a str> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "type_identifier" || child.kind() == "simple_identifier" || child.kind() == "identifier" {
            return Some(get_text(&child, source));
        }
    }
    None
}

fn walk_java<'a>(
    node: tree_sitter::Node<'a>,
    source: &str,
    repo_name: &str,
    file_path: &str,
    commit_sha: &str,
    mut current_class: String,
    mut current_method_id: Option<String>,
    symbols: &mut Vec<CodeSymbol>,
    edges: &mut Vec<CodeEdge>,
) {
    let kind = node.kind();
    
    if kind == "class_declaration" || kind == "interface_declaration" || kind == "enum_declaration" {
        if let Some(name_node) = node.child_by_field_name("name") {
            let name = get_text(&name_node, source);
            current_class = name.to_string();
            let id = format!("{}::{}::{}", repo_name, file_path, name);
            
            let sig_end = node.child_by_field_name("body").map(|b| b.start_byte()).unwrap_or(node.end_byte());
            let sig_text = source[node.start_byte()..sig_end].trim().to_string();
            let vis = if sig_text.contains("private") { "private" } else if sig_text.contains("protected") { "protected" } else { "public" };
            
            symbols.push(CodeSymbol {
                id: id.clone(),
                repo_name: repo_name.to_string(),
                file_path: file_path.to_string(),
                symbol_name: name.to_string(),
                kind: kind.to_string(),
                signature: sig_text,
                visibility: vis.to_string(),
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
                doc_comment: "".to_string(),
                community_id: None,
                commit_sha: commit_sha.to_string(),
            });
            
            if let Some(super_node) = node.child_by_field_name("superclass") {
                let super_name = get_text(&super_node, source).replace("extends ", "").trim().to_string();
                edges.push(CodeEdge {
                    source_id: id.clone(),
                    target_id: super_name,
                    edge_type: "INHERITS".to_string(),
                    confidence_tier: ConfidenceTier::Extracted,
                    confidence_score: 1.0,
                    evidence: "superclass".to_string(),
                });
            }
            if let Some(interfaces_node) = node.child_by_field_name("interfaces") {
                // Simplified implements
                let impl_text = get_text(&interfaces_node, source).replace("implements ", "").trim().to_string();
                for iface in impl_text.split(',') {
                    edges.push(CodeEdge {
                        source_id: id.clone(),
                        target_id: iface.trim().to_string(),
                        edge_type: "IMPLEMENTS".to_string(),
                        confidence_tier: ConfidenceTier::Extracted,
                        confidence_score: 1.0,
                        evidence: "interfaces".to_string(),
                    });
                }
            }
        }
    } else if kind == "method_declaration" || kind == "constructor_declaration" {
        if let Some(name_node) = node.child_by_field_name("name") {
            let name = get_text(&name_node, source);
            let mut param_types = Vec::new();
            if let Some(params_node) = node.child_by_field_name("parameters") {
                let mut cursor = params_node.walk();
                for child in params_node.children(&mut cursor) {
                    if child.kind() == "formal_parameter" || child.kind() == "spread_parameter" {
                        if let Some(type_node) = child.child_by_field_name("type") {
                            param_types.push(get_text(&type_node, source).to_string());
                        }
                    }
                }
            }
            
            let class_prefix = if current_class.is_empty() { "".to_string() } else { format!("{}.", current_class) };
            let id = format!("{}::{}::{}{}({})#{}", repo_name, file_path, class_prefix, name, param_types.join(","), param_types.len());
            
            let sig_end = node.child_by_field_name("body").map(|b| b.start_byte()).unwrap_or(node.end_byte());
            let sig_text = source[node.start_byte()..sig_end].trim().to_string();
            let vis = if sig_text.contains("private") { "private" } else if sig_text.contains("protected") { "protected" } else { "public" };
            
            symbols.push(CodeSymbol {
                id: id.clone(),
                repo_name: repo_name.to_string(),
                file_path: file_path.to_string(),
                symbol_name: name.to_string(),
                kind: kind.to_string(),
                signature: sig_text,
                visibility: vis.to_string(),
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
                doc_comment: "".to_string(),
                community_id: None,
                commit_sha: commit_sha.to_string(),
            });
            current_method_id = Some(id);
        }
    } else if kind == "method_invocation" {
        if let Some(m_id) = &current_method_id {
            if let Some(name_node) = node.child_by_field_name("name") {
                let callee = get_text(&name_node, source);
                edges.push(CodeEdge {
                    source_id: m_id.clone(),
                    target_id: callee.to_string(),
                    edge_type: "CALLS".to_string(),
                    confidence_tier: ConfidenceTier::Extracted,
                    confidence_score: 1.0,
                    evidence: "method_invocation".to_string(),
                });
            }
        }
    } else if kind == "object_creation_expression" {
        if let Some(m_id) = &current_method_id {
            if let Some(type_node) = node.child_by_field_name("type") {
                let callee = get_text(&type_node, source);
                edges.push(CodeEdge {
                    source_id: m_id.clone(),
                    target_id: callee.to_string(),
                    edge_type: "CALLS".to_string(),
                    confidence_tier: ConfidenceTier::Extracted,
                    confidence_score: 1.0,
                    evidence: "object_creation_expression".to_string(),
                });
            }
        }
    } else if kind == "import_declaration" {
        let text = get_text(&node, source).replace("import ", "").replace(";", "").trim().to_string();
        edges.push(CodeEdge {
            source_id: format!("{}::{}", repo_name, file_path),
            target_id: text,
            edge_type: "IMPORTS".to_string(),
            confidence_tier: ConfidenceTier::Extracted,
            confidence_score: 1.0,
            evidence: "import".to_string(),
        });
    }
    
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_java(child, source, repo_name, file_path, commit_sha, current_class.clone(), current_method_id.clone(), symbols, edges);
    }
}

fn walk_kotlin<'a>(
    node: tree_sitter::Node<'a>,
    source: &str,
    repo_name: &str,
    file_path: &str,
    commit_sha: &str,
    mut current_class: String,
    mut current_fn_id: Option<String>,
    symbols: &mut Vec<CodeSymbol>,
    edges: &mut Vec<CodeEdge>,
) {
    let kind = node.kind();
    
    if kind == "class_declaration" || kind == "object_declaration" {
        if let Some(name) = get_first_identifier(&node, source) {
            current_class = name.to_string();
            let id = format!("{}::{}::{}", repo_name, file_path, name);
            
            let sig_end = node.child_by_field_name("class_body").map(|b| b.start_byte()).unwrap_or(node.end_byte());
            let sig_text = source[node.start_byte()..sig_end].trim().to_string();
            let vis = if sig_text.contains("private") { "private" } else if sig_text.contains("protected") { "protected" } else if sig_text.contains("internal") { "internal" } else { "public" };
            
            symbols.push(CodeSymbol {
                id: id.clone(),
                repo_name: repo_name.to_string(),
                file_path: file_path.to_string(),
                symbol_name: name.to_string(),
                kind: kind.to_string(),
                signature: sig_text,
                visibility: vis.to_string(),
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
                doc_comment: "".to_string(),
                community_id: None,
                commit_sha: commit_sha.to_string(),
            });
            
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "delegation_specifier" {
                    let target = get_text(&child, source);
                    edges.push(CodeEdge {
                        source_id: id.clone(),
                        target_id: target.to_string(),
                        edge_type: "INHERITS".to_string(),
                        confidence_tier: ConfidenceTier::Extracted,
                        confidence_score: 1.0,
                        evidence: "delegation_specifier".to_string(),
                    });
                }
            }
        }
    } else if kind == "function_declaration" {
        if let Some(name) = get_first_identifier(&node, source) {
            let mut param_types = Vec::new();
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "function_value_parameters" {
                    let mut p_cursor = child.walk();
                    for p_child in child.children(&mut p_cursor) {
                        if p_child.kind() == "parameter" || p_child.kind() == "parameter_with_optional_type" {
                            let p_text = get_text(&p_child, source);
                            let p_type = p_text
                                .split(':')
                                .nth(1)
                                .map(|s| s.trim().to_string())
                                .unwrap_or_else(|| p_text.trim().to_string());
                            param_types.push(p_type);
                        }
                    }
                }
            }
            
            let class_prefix = if current_class.is_empty() { "".to_string() } else { format!("{}.", current_class) };
            let id = format!("{}::{}::{}{}({})#{}", repo_name, file_path, class_prefix, name, param_types.join(","), param_types.len());
            
            let sig_end = node.child_by_field_name("function_body").map(|b| b.start_byte()).unwrap_or(node.end_byte());
            let sig_text = source[node.start_byte()..sig_end].trim().to_string();
            let vis = if sig_text.contains("private") { "private" } else if sig_text.contains("protected") { "protected" } else { "public" };
            
            symbols.push(CodeSymbol {
                id: id.clone(),
                repo_name: repo_name.to_string(),
                file_path: file_path.to_string(),
                symbol_name: name.to_string(),
                kind: kind.to_string(),
                signature: sig_text,
                visibility: vis.to_string(),
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
                doc_comment: "".to_string(),
                community_id: None,
                commit_sha: commit_sha.to_string(),
            });
            current_fn_id = Some(id);
        }
    } else if kind == "call_expression" {
        if let Some(fn_id) = &current_fn_id {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "simple_identifier" || child.kind() == "type_identifier" {
                    let callee = get_text(&child, source);
                    edges.push(CodeEdge {
                        source_id: fn_id.clone(),
                        target_id: callee.to_string(),
                        edge_type: "CALLS".to_string(),
                        confidence_tier: ConfidenceTier::Extracted,
                        confidence_score: 1.0,
                        evidence: "call_expression".to_string(),
                    });
                    break;
                }
            }
        }
    } else if kind == "import_header" {
        let text = get_text(&node, source).replace("import ", "").trim().to_string();
        edges.push(CodeEdge {
            source_id: format!("{}::{}", repo_name, file_path),
            target_id: text,
            edge_type: "IMPORTS".to_string(),
            confidence_tier: ConfidenceTier::Extracted,
            confidence_score: 1.0,
            evidence: "import".to_string(),
        });
    }
    
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_kotlin(child, source, repo_name, file_path, commit_sha, current_class.clone(), current_fn_id.clone(), symbols, edges);
    }
}

pub fn extract_file_graph<S: SanitizerProfile>(
    repo_name: &str,
    file_path: &str,
    source: &str,
    commit_sha: &str,
    sanitizer: &S,
) -> anyhow::Result<ExtractedFileGraph> {
    let sanitized_source = sanitizer.sanitize(source).sanitized_content;
    let language = SupportedLanguage::from_path(file_path).unwrap_or(SupportedLanguage::Rust);
    
    let mut symbols = Vec::new();
    let mut edges = Vec::new();

    if language == SupportedLanguage::Java {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_java::LANGUAGE.into()).unwrap();
        let tree = parser.parse(&sanitized_source, None).unwrap();
        walk_java(tree.root_node(), &sanitized_source, repo_name, file_path, commit_sha, "".to_string(), None, &mut symbols, &mut edges);
    } else if language == SupportedLanguage::Kotlin {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_kotlin::LANGUAGE.into()).unwrap();
        let tree = parser.parse(&sanitized_source, None).unwrap();
        walk_kotlin(tree.root_node(), &sanitized_source, repo_name, file_path, commit_sha, "".to_string(), None, &mut symbols, &mut edges);
    } else {
        // Fallback structural regex parser
        let re_struct = Regex::new(r"(?m)^\s*(?:pub\s+|export\s+)?(?:struct|class|interface|trait)\s+([a-zA-Z0-9_]+)").unwrap();
        let re_func = Regex::new(r"(?m)^\s*(?:pub\s+|export\s+|private\s+)?(?:fn|def|func|function)\s+([a-zA-Z0-9_]+)\s*\(([^)]*)\)").unwrap();
        let re_call = Regex::new(r"([a-zA-Z_][a-zA-Z0-9_]*)\s*\(").unwrap();
        let re_import = Regex::new(r#"(?m)^\s*(?:use|import|#include)\s+([a-zA-Z0-9_:.\"/<>]+)"#).unwrap();
        
        let mut current_fn: Option<String> = None;
        let mut brace_depth: i32 = 0;
        let mut is_fn_decl_line = false;
        
        for (i, line) in sanitized_source.lines().enumerate() {
            is_fn_decl_line = false;
            let vis = if line.contains("pub ") || line.contains("export ") { "public" } else if line.contains("private") { "private" } else { "public" };
            
            if let Some(caps) = re_struct.captures(line) {
                let name = caps.get(1).unwrap().as_str();
                symbols.push(CodeSymbol {
                    id: format!("{}::{}::{}", repo_name, file_path, name),
                    repo_name: repo_name.to_string(),
                    file_path: file_path.to_string(),
                    symbol_name: name.to_string(),
                    kind: "struct_like".to_string(),
                    signature: line.trim().to_string(),
                    visibility: vis.to_string(),
                    start_line: i + 1,
                    end_line: i + 1,
                    doc_comment: "".to_string(),
                    community_id: None,
                    commit_sha: commit_sha.to_string(),
                });
            } else if let Some(caps) = re_func.captures(line) {
                is_fn_decl_line = true;
                let name = caps.get(1).unwrap().as_str();
                let params = caps.get(2).map_or("", |m| m.as_str());
                let arity = if params.trim().is_empty() { 0 } else { params.split(',').count() };
                
                let id = format!("{}::{}::{}({})#{}", repo_name, file_path, name, params, arity);
                current_fn = Some(id.clone());
                brace_depth = 0;
                
                symbols.push(CodeSymbol {
                    id: id.clone(),
                    repo_name: repo_name.to_string(),
                    file_path: file_path.to_string(),
                    symbol_name: name.to_string(),
                    kind: "function_like".to_string(),
                    signature: line.trim().to_string(),
                    visibility: vis.to_string(),
                    start_line: i + 1,
                    end_line: i + 1,
                    doc_comment: "".to_string(),
                    community_id: None,
                    commit_sha: commit_sha.to_string(),
                });
            } else if let Some(caps) = re_import.captures(line) {
                let target = caps.get(1).unwrap().as_str();
                edges.push(CodeEdge {
                    source_id: format!("{}::{}", repo_name, file_path),
                    target_id: target.to_string(),
                    edge_type: "IMPORTS".to_string(),
                    confidence_tier: ConfidenceTier::Extracted,
                    confidence_score: 1.0,
                    evidence: "import".to_string(),
                });
            }
            
            if let Some(fn_id) = &current_fn {
                if !is_fn_decl_line {
                for caps in re_call.captures_iter(line) {
                    let callee = caps.get(1).unwrap().as_str();
                    let kw = ["if", "for", "while", "match", "switch", "return", "catch", "sizeof", "fn", "def", "func", "function"];
                    if !kw.contains(&callee) {
                        edges.push(CodeEdge {
                            source_id: fn_id.clone(),
                            target_id: callee.to_string(),
                            edge_type: "CALLS".to_string(),
                            confidence_tier: ConfidenceTier::Extracted,
                            confidence_score: 1.0,
                            evidence: "call".to_string(),
                        });
                    }
                }
                }
            }
            
            brace_depth += line.chars().filter(|c| *c == '{').count() as i32;
            brace_depth -= line.chars().filter(|c| *c == '}').count() as i32;
            if brace_depth <= 0 && line.contains('}') {
                current_fn = None;
            }
        }
    }
    
    Ok(ExtractedFileGraph {
        file_path: file_path.to_string(),
        language,
        symbols,
        edges,
    })
}

pub fn resolve_intra_repo_edges(symbols: &[CodeSymbol], edges: &mut Vec<CodeEdge>) {
    let mut name_to_id: HashMap<String, String> = HashMap::new();
    for sym in symbols {
        name_to_id.insert(sym.symbol_name.clone(), sym.id.clone());
        if let Some(parent) = sym.id.split("::").nth(2) {
            let parts: Vec<&str> = parent.split('.').collect();
            if parts.len() >= 2 {
                name_to_id.insert(format!("{}.{}", parts[0], sym.symbol_name), sym.id.clone());
            }
        }
    }
    
    for edge in edges.iter_mut() {
        if let Some(resolved_id) = name_to_id.get(&edge.target_id) {
            edge.target_id = resolved_id.clone();
        }
    }
}
