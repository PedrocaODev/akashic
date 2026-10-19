use crate::store::{GraphStore, ConfidenceTier};
use crate::analytics::{god_nodes, surprising_connections};
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::fs;

pub fn generate_graph_report(store: &GraphStore, repo_name: &str) -> Result<String> {
    let mut out = String::new();
    
    out.push_str(&format!("# Graph Report for `{}`\n\n", repo_name));
    
    let symbols = store.get_symbols_by_repo(repo_name)?;
    let edges = store.get_edges_by_repo(repo_name)?;
    
    let mut ext_edges = 0;
    let mut inf_edges = 0;
    let mut amb_edges = 0;
    
    for edge in &edges {
        match edge.confidence_tier {
            ConfidenceTier::Extracted => ext_edges += 1,
            ConfidenceTier::Inferred => inf_edges += 1,
            ConfidenceTier::Ambiguous => amb_edges += 1,
        }
    }
    
    out.push_str("## Repository Summary\n");
    out.push_str(&format!("- **Total Nodes**: {}\n", symbols.len()));
    out.push_str(&format!("- **Total Edges**: {}\n", edges.len()));
    out.push_str(&format!("  - Extracted: {}\n", ext_edges));
    out.push_str(&format!("  - Inferred: {}\n", inf_edges));
    out.push_str(&format!("  - Ambiguous: {}\n\n", amb_edges));
    
    out.push_str("## Top Graph Communities\n");
    let mut comms = store.get_communities(repo_name)?;
    comms.sort_by(|a, b| b.node_count.cmp(&a.node_count));
    for c in comms.iter().take(10) {
        out.push_str(&format!("- `{}` ({} nodes, cohesion: {:.2}): {}\n", c.community_id, c.node_count, c.cohesion_score, c.label));
    }
    out.push_str("\n");
    
    out.push_str("## God Nodes (High-Degree Architectural Hubs)\n");
    let gods = god_nodes(store, repo_name, 10)?;
    for g in gods {
        out.push_str(&format!("- `{}` (In: {}, Out: {}, Total: {})\n", g.symbol.symbol_name, g.in_degree, g.out_degree, g.total_degree));
    }
    out.push_str("\n");
    
    out.push_str("## Surprising Connections (Cross-Community & Inferred Semantic Bridges)\n");
    let surprises = surprising_connections(store, repo_name, 10)?;
    for s in surprises {
        out.push_str(&format!("- `{}` -> `{}` (Score: {:.2}): {}\n", s.source_symbol, s.target_symbol, s.surprise_score, s.reason));
    }
    out.push_str("\n");
    
    Ok(out)
}

pub fn generate_wiki(store: &GraphStore, repo_name: &str, out_dir: &Path) -> Result<Vec<PathBuf>> {
    fs::create_dir_all(out_dir)?;
    
    let mut files = Vec::new();
    
    let mut index_content = format!("# {} Wiki\n\n", repo_name);
    index_content.push_str("## Communities\n");
    
    let comms = store.get_communities(repo_name)?;
    let symbols = store.get_symbols_by_repo(repo_name)?;
    let edges = store.get_edges_by_repo(repo_name)?;
    
    for comm in &comms {
        let safe_id = comm.community_id.replace("::", "_").replace("-", "_");
        let filename = format!("{}.md", safe_id);
        let filepath = out_dir.join(&filename);
        
        index_content.push_str(&format!("- [{} ({} nodes)]({})\n", comm.label, comm.node_count, filename));
        
        let mut comm_content = format!("# Community: {}\n\n", comm.label);
        comm_content.push_str(&format!("- **ID**: `{}`\n", comm.community_id));
        comm_content.push_str(&format!("- **Nodes**: {}\n", comm.node_count));
        comm_content.push_str(&format!("- **Cohesion**: {:.2}\n\n", comm.cohesion_score));
        
        comm_content.push_str("## Member Symbols\n");
        for sym in symbols.iter().filter(|s| s.community_id.as_deref() == Some(comm.community_id.as_str())) {
            comm_content.push_str(&format!("### `{}`\n", sym.symbol_name));
            comm_content.push_str(&format!("- **Kind**: {}\n", sym.kind));
            comm_content.push_str(&format!("- **File**: `{}`\n", sym.file_path));
            comm_content.push_str(&format!("```\n{}\n```\n\n", sym.signature));
            
            if !sym.doc_comment.is_empty() {
                comm_content.push_str(&format!("{}\n\n", sym.doc_comment));
            }
        }
        
        fs::write(&filepath, comm_content)?;
        files.push(filepath);
    }
    
    let index_path = out_dir.join("index.md");
    fs::write(&index_path, index_content)?;
    files.push(index_path);
    
    Ok(files)
}
