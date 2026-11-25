use crate::providers::{MediaType, SearchProvider, SearchResult};

pub struct SearchEngine {
    providers: Vec<Box<dyn SearchProvider>>,
}

impl SearchEngine {
    pub fn new() -> Self {
        Self { providers: Vec::new() }
    }

    pub fn register_provider(&mut self, provider: Box<dyn SearchProvider>) {
        self.providers.push(provider);
    }

    pub fn search(&self, query: &str) -> crate::engine::SearchResponse {
        let mut results = Vec::new();
        let mut errors = Vec::new();

        for p in &self.providers {
            match p.search(query) {
                Ok(mut r) => results.append(&mut r),
                Err(e) => errors.push(e.to_string()),
            }
        }

        crate::engine::SearchResponse { results, errors }
    }
}

pub struct SearchResponse {
    pub results: Vec<SearchResult>,
    pub errors: Vec<String>,
}
