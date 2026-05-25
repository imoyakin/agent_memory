use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::config::RuntimeConfig;
use crate::DEFAULT_ENDPOINT;

pub(crate) fn embed(config: &RuntimeConfig, text: &str) -> Result<Vec<f32>> {
    match config.embedding_provider.as_str() {
        "fake" => Ok(fake_embedding(text, config.embedding_dim)),
        "ollama" => ollama_embedding(config, text),
        "openai" => openai_embedding(config, text),
        other => bail!("unsupported embedding provider: {}", other),
    }
}

pub(crate) fn fake_embedding(text: &str, dim: usize) -> Vec<f32> {
    let mut vector = vec![0.0f32; dim.max(1)];
    let tokens: Vec<_> = text.split_whitespace().collect();
    for token in tokens
        .iter()
        .copied()
        .chain(if tokens.is_empty() { Some(text) } else { None })
    {
        let digest = Sha256::digest(token.to_lowercase().as_bytes());
        let index = usize::from_be_bytes(digest[0..8].try_into().unwrap()) % vector.len();
        let sign = if digest[8] % 2 == 0 { 1.0 } else { -1.0 };
        vector[index] += sign;
    }
    let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm > 0.0 {
        for value in &mut vector {
            *value /= norm;
        }
    }
    vector
}

pub(crate) fn ollama_embedding(config: &RuntimeConfig, text: &str) -> Result<Vec<f32>> {
    let endpoint = config
        .embedding_endpoint
        .as_deref()
        .unwrap_or(DEFAULT_ENDPOINT)
        .trim_end_matches('/');
    let client = reqwest::blocking::Client::new();
    let response: Value = client
        .post(format!("{endpoint}/api/embed"))
        .json(&json!({"model": config.embedding_model, "input": text}))
        .send()?
        .json()?;
    if let Some(first) = response
        .get("embeddings")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(Value::as_array)
    {
        return vector_from_json(first, config.embedding_dim);
    }
    let response: Value = client
        .post(format!("{endpoint}/api/embeddings"))
        .json(&json!({"model": config.embedding_model, "prompt": text}))
        .send()?
        .json()?;
    let values = response
        .get("embedding")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("Ollama response did not contain an embedding"))?;
    vector_from_json(values, config.embedding_dim)
}

pub(crate) fn openai_embedding(config: &RuntimeConfig, text: &str) -> Result<Vec<f32>> {
    let api_key = std::env::var("OPENAI_API_KEY").context("OPENAI_API_KEY is required")?;
    let endpoint = config
        .embedding_endpoint
        .as_deref()
        .unwrap_or("https://api.openai.com/v1/embeddings");
    let response: Value = reqwest::blocking::Client::new()
        .post(endpoint)
        .bearer_auth(api_key)
        .json(&json!({"model": config.embedding_model, "input": text}))
        .send()?
        .json()?;
    let values = response
        .get("data")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|item| item.get("embedding"))
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("OpenAI response did not contain an embedding"))?;
    vector_from_json(values, config.embedding_dim)
}

pub(crate) fn vector_from_json(values: &[Value], dim: usize) -> Result<Vec<f32>> {
    let vector = values
        .iter()
        .map(|value| {
            value
                .as_f64()
                .map(|item| item as f32)
                .ok_or_else(|| anyhow!("embedding vector contains non-number"))
        })
        .collect::<Result<Vec<_>>>()?;
    if vector.len() != dim {
        bail!(
            "embedding dimension mismatch: expected {}, got {}",
            dim,
            vector.len()
        );
    }
    Ok(vector)
}
