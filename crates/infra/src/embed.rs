use anyhow::Result;
use fastembed::{EmbeddingModel, TextEmbedding, TextInitOptions};
use std::cell::RefCell;

pub struct Embedder {
    model: RefCell<TextEmbedding>,
}

impl Embedder {
    pub fn new() -> Result<Self> {
        let model = TextEmbedding::try_new(
            TextInitOptions::new(EmbeddingModel::BGESmallENV15)
                .with_show_download_progress(true),
        )?;
        Ok(Self {
            model: RefCell::new(model),
        })
    }

    pub fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let passages: Vec<String> = texts
            .iter()
            .map(|t| format!("passage: {}", t))
            .collect();
        let embeddings = self.model.borrow_mut().embed(passages, None)?;
        Ok(embeddings)
    }

    pub fn embed_query(&self, query: &str) -> Result<Vec<f32>> {
        let q = format!("query: {}", query);
        let embeddings = self.model.borrow_mut().embed(vec![q], None)?;
        Ok(embeddings.into_iter().next().unwrap_or_default())
    }
}
