use anyhow::Result;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::config::RuntimeConfig;
use crate::embedding::embed;
use crate::models::{MemoryRecord, SearchResult};
use crate::records::record_search_text;
use crate::storage::search_vector_backend;

pub(crate) fn vector_results(
    root: &Path,
    config: &RuntimeConfig,
    records: &[MemoryRecord],
    query: &str,
    limit: usize,
) -> Result<Vec<SearchResult>> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let vector = embed(config, query)?;
    let hits = search_vector_backend(root, config, &vector, limit)?;
    let records_by_uuid: HashMap<_, _> = records
        .iter()
        .map(|record| (&record.uuid, record))
        .collect();
    let mut results = Vec::new();
    for hit in hits {
        let Some(record) = records_by_uuid.get(&hit.uuid) else {
            continue;
        };
        let relevance = vector_relevance(hit.distance);
        let reliability = reliability_score(record);
        results.push(SearchResult {
            uuid: record.uuid.clone(),
            memory_id: record.uuid.clone(),
            summary: record.summary.clone(),
            content: record.content.clone(),
            keys: record.keys.clone(),
            matched_keys: Vec::new(),
            memory_type: record.memory_type.clone(),
            scope: record.scope.clone(),
            tags: record.tags.clone(),
            source_kind: record.source_kind.clone(),
            source_ref: record.source_ref.clone(),
            updated_at: record.updated_at.clone(),
            confidence: record.confidence,
            reliability,
            relevance,
            score: combined_score(relevance, reliability),
            embedding_status: record.embedding_status.clone(),
            match_reasons: vec!["direct_embedding".to_string()],
        });
    }
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(results)
}

pub(crate) fn merge_search_results(base: &mut Vec<SearchResult>, extra: Vec<SearchResult>) {
    for candidate in extra {
        if let Some(existing) = base.iter_mut().find(|item| item.uuid == candidate.uuid) {
            existing.relevance = existing.relevance.max(candidate.relevance);
            existing.score = combined_score(existing.relevance, existing.reliability);
            for reason in candidate.match_reasons {
                if !existing.match_reasons.contains(&reason) {
                    existing.match_reasons.push(reason);
                }
            }
        } else {
            base.push(candidate);
        }
    }
    base.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

pub(crate) fn vector_relevance(distance: f64) -> f64 {
    round4(distance.clamp(0.0, 1.0))
}

pub(crate) fn exact_results(records: &[MemoryRecord], query: &str) -> Vec<SearchResult> {
    let query_tokens = tokenize(query);
    let query_lower = query.to_lowercase();
    let mut results = Vec::new();
    for record in records {
        let haystack = record_search_text(record);
        let document_tokens = tokenize(&haystack);
        let overlap = query_tokens
            .iter()
            .filter(|token| document_tokens.contains(token))
            .count() as f64;
        let matched_keys = matched_keys(&record.keys, query, &query_tokens);
        let substring_bonus = if query_lower.len() >= 4 && haystack.contains(&query_lower) {
            0.2
        } else {
            0.0
        };
        let key_bonus = if matched_keys.is_empty() {
            0.0
        } else {
            0.25 + (matched_keys.len().min(3) as f64 * 0.05)
        };
        let base = if query_tokens.is_empty() {
            0.0
        } else {
            overlap / query_tokens.len() as f64
        };
        let relevance = (base * 0.65 + substring_bonus + key_bonus).min(1.0);
        if relevance <= 0.0 {
            continue;
        }
        let reliability = reliability_score(record);
        let mut reasons = vec![if matched_keys.is_empty() {
            "direct_text"
        } else {
            "direct_keys"
        }
        .to_string()];
        if record.embedding_status == "pending" {
            reasons.push("pending".to_string());
        }
        results.push(SearchResult {
            uuid: record.uuid.clone(),
            memory_id: record.uuid.clone(),
            summary: record.summary.clone(),
            content: record.content.clone(),
            keys: record.keys.clone(),
            matched_keys,
            memory_type: record.memory_type.clone(),
            scope: record.scope.clone(),
            tags: record.tags.clone(),
            source_kind: record.source_kind.clone(),
            source_ref: record.source_ref.clone(),
            updated_at: record.updated_at.clone(),
            confidence: record.confidence,
            reliability,
            relevance: round4(relevance),
            score: combined_score(relevance, reliability),
            embedding_status: record.embedding_status.clone(),
            match_reasons: reasons,
        });
    }
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results
}

pub(crate) fn matched_keys(keys: &[String], query: &str, query_tokens: &[String]) -> Vec<String> {
    let query_lower = query.to_lowercase();
    let query_set: HashSet<_> = query_tokens.iter().collect();
    keys.iter()
        .filter(|key| {
            let key_lower = key.to_lowercase();
            let key_tokens = tokenize(key);
            (query_lower.len() >= 3
                && (key_lower.contains(&query_lower) || query_lower.contains(&key_lower)))
                || key_tokens.iter().any(|token| query_set.contains(token))
        })
        .cloned()
        .collect()
}

pub(crate) fn tokenize(text: &str) -> Vec<String> {
    Regex::new(r"[\w./:-]+")
        .unwrap()
        .find_iter(text)
        .map(|item| item.as_str().to_lowercase())
        .filter(|item| !item.is_empty())
        .collect()
}

pub(crate) fn reliability_score(record: &MemoryRecord) -> f64 {
    let authority = match record.source_kind.as_str() {
        "user" => 0.95,
        "official_docs" => 0.9,
        "repo" => 0.85,
        "tool_output" => 0.75,
        "agent_inferred" => 0.45,
        _ => 0.5,
    };
    let conflict_penalty = (record.conflict_count as f64 * 0.12).min(0.5);
    let pending_penalty = if matches!(record.embedding_status.as_str(), "pending" | "failed") {
        0.05
    } else {
        0.0
    };
    round4(
        (0.52 * record.confidence + 0.35 * authority + 0.13 - conflict_penalty - pending_penalty)
            .clamp(0.0, 1.0),
    )
}

pub(crate) fn combined_score(relevance: f64, reliability: f64) -> f64 {
    round4(relevance * 0.65 + reliability * 0.35)
}

pub(crate) fn round4(value: f64) -> f64 {
    (value * 10000.0).round() / 10000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::now;

    #[test]
    fn search_defaults_to_one_direct_hit() {
        let record = MemoryRecord {
            uuid: "a".to_string(),
            content: "Use repo truth before stale memory.".to_string(),
            keys: vec!["repo truth".to_string()],
            summary: "Use repo truth before stale memory.".to_string(),
            embedding_status: "pending".to_string(),
            embedding_error: None,
            embedding_attempts: 0,
            memory_type: "preference".to_string(),
            scope: "project".to_string(),
            root_path: ".".to_string(),
            tags: vec![],
            source_kind: "user".to_string(),
            source_ref: "test".to_string(),
            created_at: now(),
            updated_at: now(),
            last_accessed_at: None,
            access_count: 0,
            conflict_count: 0,
            confidence: 0.95,
            verified_at: None,
            stale_after_days: None,
            embedding_provider: "fake".to_string(),
            embedding_model: "fake".to_string(),
            embedding_dim: 16,
            schema_version: 1,
        };
        let results = exact_results(&[record], "repo truth");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].matched_keys, vec!["repo truth".to_string()]);
    }
}
