use super::provider::{
    MediaType,
    ProviderError,
    SearchProvider,
    SearchResult,
};

pub struct DemoProvider;

impl SearchProvider for DemoProvider {
    fn name(&self) -> &'static str {
        "Demo"
    }

    fn search(
        &self,
        query: &str,
    ) -> Result<Vec<SearchResult>, ProviderError> {
        Ok(vec![
            SearchResult {
                title: format!("{} - Demo Result", query),
                creator: Some("OME Test Artist".to_string()),
                url: "https://example.com/media".to_string(),
                provider: self.name().to_string(),
                media_type: MediaType::Audio,
            }
        ])
    }
}
