use crate::providers::{MediaType, SearchResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Availability {
    Available,
    Unavailable,
    Restricted,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct MediaFormat {
    pub format_id: String,
    pub media_type: MediaType,
    pub container: Option<String>,
    pub mime_type: Option<String>,
    pub codec: Option<String>,
    pub bitrate_kbps: Option<u32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub file_size_bytes: Option<u64>,
    pub direct_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedMedia {
    pub title: String,
    pub provider: String,
    pub page_url: String,
    pub availability: Availability,
    pub formats: Vec<MediaFormat>,
    pub message: Option<String>,
}

pub trait MediaResolver: Send + Sync {
    fn provider(&self) -> &'static str;

    fn can_resolve(&self, result: &SearchResult) -> bool {
        result.provider.eq_ignore_ascii_case(self.provider())
    }

    fn resolve(&self, result: &SearchResult) -> Result<ResolvedMedia, String>;
}
