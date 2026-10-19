use anyhow::Result;
use akashic_embedding::EmbeddingProvider;
use crate::store::{GraphStore, CodeSymbol, ConfidenceTier, CodeEdge};

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
    let mut texts = Vec::new();
    let mut ids = Vec::new();
    
    for sym in &symbols {
        texts.push(format!("{} {}", sym.signature, sym.doc_comment));
        ids.push(sym.id.clone());
    }
    
    if texts.is_empty() {
        return Ok(0);
    }
    
    let vectors = embedding.embed_batch(&texts).map_err(|e| anyhow::anyhow!("Embed error: {:?}", e))?;
    
    for (id, vec) in ids.iter().zip(vectors.iter()) {
        store.upsert_embedding(id, repo_name, &serde_json::to_string(vec)?)?;
    }
    
    let edges = store.get_edges_by_repo(repo_name)?;
    let mut existing_pairs = std::collections::HashSet::new();
    for e in edges {
        if e.confidence_tier == ConfidenceTier::Extracted {
            existing_pairs.insert((e.source_id.clone(), e.target_id.clone()));
        }
    }
    
    let mut inferred_count = 0;
    
    for i in 0..ids.len() {
        for j in (i + 1)..ids.len() {
            let sim = cosine_similarity(&vectors[i], &vectors[j]);
            if sim >= threshold {
                let s_id = &ids[i];
                let t_id = &ids[j];
                
                if !existing_pairs.contains(&(s_id.clone(), t_id.clone())) && !existing_pairs.contains(&(t_id.clone(), s_id.clone())) {
                    store.upsert_edge(&CodeEdge {
                        source_id: s_id.clone(),
                        target_id: t_id.clone(),
                        edge_type: "SEMANTIC_SIMILARITY".to_string(),
                        confidence_tier: ConfidenceTier::Inferred,
                        confidence_score: sim as f64,
                        evidence: format!("Cosine similarity: {:.3}", sim),
                    })?;
                    inferred_count += 1;
                }
            }
        }
    }
    
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
    
    let query_vec = embedding.embed_batch(&[query.to_string()]).map_err(|e| anyhow::anyhow!("{:?}", e))?.into_iter().next().unwrap();
    let embeddings = store.get_embeddings(repo_name)?;
    
    let mut vec_scores = Vec::new();
    for (id, vec) in embeddings {
        let sim = cosine_similarity(&query_vec, &vec);
        vec_scores.push((id, sim));
    }
    vec_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    
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
    
    for (i, (id, sim)) in vec_scores.into_iter().enumerate().take(limit) {
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
