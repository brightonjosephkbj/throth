#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum MediaType {
    Audio,
    Video,
    Unknown,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchResult {
    pub title: String,
    pub creator: Option<String>,
    pub url: String,
    pub provider: String,
    pub artwork_url: Option<String>,
    pub duration_ms: Option<u64>,
    pub media_type: MediaType,
}

pub type ProviderError = String;

pub trait SearchProvider: Send + Sync {
    fn name(&self) -> &'static str;

    fn search(&self, query: &str) -> Result<Vec<SearchResult>, ProviderError>;
}
