use crate::providers::MediaType;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchIntent {
    General,
    Artist,
}

#[derive(Debug, Clone)]
pub struct QueryPlan {
    pub search_terms: String,
    pub intent: SearchIntent,
    pub media_filter: Option<MediaType>,
}

impl QueryPlan {
    pub fn parse(input: &str) -> Self {
        let mut terms = Vec::new();
        let mut intent = SearchIntent::General;
        let mut media_filter = None;

        let mut tokens = input.split_whitespace().peekable();

        while let Some(token) = tokens.next() {
            let lower = token.to_lowercase();

            if lower == "artist" || lower.starts_with("artist:") {
                intent = SearchIntent::Artist;

                let value = if lower == "artist" {
                    tokens.next().unwrap_or("")
                } else {
                    token.split_once(':').map(|(_, v)| v).unwrap_or("")
                };

                if !value.is_empty() {
                    terms.push(value.to_string());
                }
            } else if lower == "audio" {
                media_filter = Some(MediaType::Audio);
            } else if lower == "video" {
                media_filter = Some(MediaType::Video);
            } else {
                terms.push(token.to_string());
            }
        }

        let search_terms = if terms.is_empty() {
            input.trim().to_string()
        } else {
            terms.join(" ")
        };

        Self {
            search_terms,
            intent,
            media_filter,
        }
    }
}
