use crate::ast::ExtractedFileGraph;

#[derive(Debug)]
pub enum SignificanceVerdict {
    StructurallySignificant { reasons: Vec<String> },
    ImplementationOnly,
    NoSemanticChange,
    ExcludedPath,
}

pub fn evaluate_file_significance(
    file_path: &str,
    before: Option<&ExtractedFileGraph>,
    after: Option<&ExtractedFileGraph>,
) -> SignificanceVerdict {
    if file_path.contains(".git/") || file_path.contains(".akashic/") 
        || file_path.contains("build/") || file_path.contains("target/")
        || file_path.contains("node_modules/") || file_path.contains(".gradle/")
        || file_path.ends_with(".md") || file_path.ends_with(".txt")
        || file_path.ends_with(".json") || file_path.ends_with(".lock") {
        return SignificanceVerdict::ExcludedPath;
    }

    match (before, after) {
        (None, None) => SignificanceVerdict::NoSemanticChange,
        (None, Some(_)) => SignificanceVerdict::StructurallySignificant { reasons: vec!["New file".into()] },
        (Some(_), None) => SignificanceVerdict::StructurallySignificant { reasons: vec!["Deleted file".into()] },
        (Some(b), Some(a)) => {
            let mut reasons = Vec::new();
            
            let b_syms: std::collections::HashSet<_> = b.symbols.iter().map(|s| (&s.id, &s.kind, &s.signature, &s.visibility)).collect();
            let a_syms: std::collections::HashSet<_> = a.symbols.iter().map(|s| (&s.id, &s.kind, &s.signature, &s.visibility)).collect();
            
            if b_syms != a_syms {
                reasons.push("Symbols changed".into());
            }

            let b_edges: std::collections::HashSet<_> = b.edges.iter().map(|e| (&e.source_id, &e.target_id, &e.edge_type)).collect();
            let a_edges: std::collections::HashSet<_> = a.edges.iter().map(|e| (&e.source_id, &e.target_id, &e.edge_type)).collect();
            
            if b_edges != a_edges {
                reasons.push("Structural edges changed".into());
            }

            if reasons.is_empty() {
                if b.symbols.is_empty() && a.symbols.is_empty() {
                    SignificanceVerdict::NoSemanticChange
                } else {
                    SignificanceVerdict::ImplementationOnly
                }
            } else {
                SignificanceVerdict::StructurallySignificant { reasons }
            }
        }
    }
}
