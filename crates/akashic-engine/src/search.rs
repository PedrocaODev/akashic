use anyhow::Result;
use akashic_embedding::EmbeddingProvider;
use crate::store::{GraphStore, CodeSymbol, ConfidenceTier, CodeEdge};

pub fn canonical_pair(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

pub fn compute_embeddings_and_inferred_edges<E: EmbeddingProvider>(
    store: &GraphStore,
    repo_name: &str,
    embedding: &E,
    threshold: f32,
) -> Result<usize> {
    if !embedding.is_available() {
        return Ok(0);
    }
    
    let symbols = store.get_symbols_by_repo(repo_name)?;
    if symbols.is_empty() {
        return Ok(0);
    }

    let mut symbol_vecs: Vec<(String, String, Vec<f32>)> = Vec::new();
    for chunk in symbols.chunks(64) {
        let texts: Vec<String> = chunk
            .iter()
            .map(|sym| format!("{} {}", sym.signature, sym.doc_comment))
            .collect();
        let vectors = embedding
            .embed_batch(&texts)
            .map_err(|e| anyhow::anyhow!("Embed error: {:?}", e))?;
        for (sym, vec) in chunk.iter().zip(vectors.into_iter()) {
            symbol_vecs.push((sym.id.clone(), repo_name.to_string(), vec));
        }
    }

    store.upsert_embeddings_batch(&symbol_vecs)?;

    let existing_edges = store.list_edges(Some(repo_name))?;
    let mut existing_pairs: std::collections::HashSet<(String, String)> = existing_edges
        .iter()
        .map(|e| canonical_pair(&e.source_id, &e.target_id))
        .collect();

    let mut new_edges = Vec::new();

    for (id_a, _, vec_a) in &symbol_vecs {
        if vec_a.iter().all(|&x| x == 0.0) && threshold > 0.0 {
            continue;
        }
        let candidates = store.search_similar_embeddings(
            vec_a,
            Some(repo_name),
            symbol_vecs.len().min(4096),
            Some((1.0 - threshold) + 1e-6),
        )?;

        for (id_b, sim) in candidates {
            if id_a != &id_b && sim >= threshold {
                let pair = canonical_pair(id_a, &id_b);
                if existing_pairs.insert(pair.clone()) {
                    new_edges.push(CodeEdge {
                        source_id: pair.0,
                        target_id: pair.1,
                        edge_type: "INFERRED".to_string(),
                        confidence_tier: ConfidenceTier::Inferred,
                        confidence_score: (sim as f64).clamp(0.0, 1.0),
                        evidence: format!("{{\"cosine_similarity\":{sim:.4}}}"),
                    });
                }
            }
        }
    }

    let inferred_count = new_edges.len();
    store.upsert_edges_batch(&new_edges)?;

    Ok(inferred_count)
}

#[derive(Debug, Clone)]
pub struct HybridSearchHit {
    pub symbol: CodeSymbol,
    pub rrf_score: f64,
    pub fts_rank: Option<usize>,
    pub vector_rank: Option<usize>,
    pub vector_similarity: Option<f64>,
}

pub fn hybrid_search<E: EmbeddingProvider>(
    store: &GraphStore,
    embedding: &E,
    query: &str,
    repo_name: Option<&str>,
    limit: usize,
    rrf_k: usize,
) -> Result<Vec<HybridSearchHit>> {
    let fts_results = store.fts_search(query, repo_name, limit)?;

    if !embedding.is_available() {
        let mut hits = Vec::new();
        for (i, sym) in fts_results.into_iter().enumerate() {
            hits.push(HybridSearchHit {
                symbol: sym,
                rrf_score: 1.0 / (rrf_k as f64 + i as f64 + 1.0),
                fts_rank: Some(i),
                vector_rank: None,
                vector_similarity: None,
            });
        }
        return Ok(hits);
    }

    let mut vec_hits = Vec::new();
    if let Ok(mut query_vecs) = embedding.embed_batch(&[query.to_string()]) {
        if let Some(query_vec) = query_vecs.pop() {
            vec_hits = store.search_similar_embeddings(&query_vec, repo_name, limit, None)?;
        }
    }

    let mut hit_map = std::collections::HashMap::new();

    for (i, sym) in fts_results.into_iter().enumerate() {
        hit_map.insert(sym.id.clone(), HybridSearchHit {
            symbol: sym,
            rrf_score: 1.0 / (rrf_k as f64 + i as f64 + 1.0),
            fts_rank: Some(i),
            vector_rank: None,
            vector_similarity: None,
        });
    }

    for (i, (id, sim)) in vec_hits.into_iter().enumerate().take(limit) {
        let rrf = 1.0 / (rrf_k as f64 + i as f64 + 1.0);
        if let Some(hit) = hit_map.get_mut(&id) {
            hit.rrf_score += rrf;
            hit.vector_rank = Some(i);
            hit.vector_similarity = Some(sim as f64);
        } else if let Ok(Some(sym)) = store.get_symbol_by_id(&id) {
            hit_map.insert(id, HybridSearchHit {
                symbol: sym,
                rrf_score: rrf,
                fts_rank: None,
                vector_rank: Some(i),
                vector_similarity: Some(sim as f64),
            });
        }
    }

    let mut final_hits: Vec<_> = hit_map.into_values().collect();
    final_hits.sort_by(|a, b| b.rrf_score.partial_cmp(&a.rrf_score).unwrap_or(std::cmp::Ordering::Equal));
    final_hits.truncate(limit);

    Ok(final_hits)
}

