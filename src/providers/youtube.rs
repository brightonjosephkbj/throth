use regex::Regex;
use reqwest::blocking::Client;
use scraper::{Html, Selector};
use serde_json::Value;
use std::collections::HashSet;

use super::provider::{MediaType, ProviderError, SearchProvider, SearchResult};

pub struct YouTubeProvider {
    client: Client,
}

impl YouTubeProvider {
    pub fn new() -> Result<Self, ProviderError> {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (Linux; Android 10) AppleWebKit/537.36 Chrome/120.0.0.0 Mobile Safari/537.36")
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| e.to_string())?;

        Ok(Self { client })
    }

    fn extract_initial_data(html: &str) -> Result<Value, ProviderError> {
        let pattern = Regex::new(r#"ytInitialData\s*=\s*"(.*?)"|ytInitialData\s*=\s*'(.*?)'"#)
            .map_err(|e| e.to_string())?;

        let captures = pattern
            .captures(html)
            .ok_or("YouTube initial data assignment not found")?;

        let encoded = captures
            .get(1)
            .or_else(|| captures.get(2))
            .ok_or("YouTube encoded data missing")?
            .as_str();

        let mut decoded = String::new();
        let mut chars = encoded.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch != '\\' {
                decoded.push(ch);
                continue;
            }

            match chars.next() {
                Some('x') => {
                    let hex: String = chars.by_ref().take(2).collect();
                    let value = u8::from_str_radix(&hex, 16)
                        .map_err(|e| format!("Invalid hex escape: {}", e))?;
                    decoded.push(value as char);
                }
                Some('u') => {
                    let hex: String = chars.by_ref().take(4).collect();
                    let value = u32::from_str_radix(&hex, 16)
                        .map_err(|e| format!("Invalid Unicode escape: {}", e))?;
                    let character = char::from_u32(value).ok_or("Invalid Unicode character")?;
                    decoded.push(character);
                }
                Some('\\') => decoded.push('\\'),
                Some('"') => decoded.push('"'),
                Some('\'') => decoded.push('\''),
                Some('/') => decoded.push('/'),
                Some('n') => decoded.push('\n'),
                Some('r') => decoded.push('\r'),
                Some('t') => decoded.push('\t'),
                Some('b') => decoded.push('\u{0008}'),
                Some('f') => decoded.push('\u{000C}'),
                Some(other) => {
                    decoded.push('\\');
                    decoded.push(other);
                }
                None => return Err("Incomplete escape sequence".to_string()),
            }
        }

        println!("YouTube decoded JSON length: {}", decoded.len());
        println!(
            "YouTube decoded JSON start: {}",
            &decoded[..decoded.len().min(80)]
        );

        let parsed: Value = serde_json::from_str(&decoded)
            .map_err(|e| format!("YouTube JSON decoding failed: {}", e))?;

        println!(
            "YouTube JSON root keys: {:?}",
            parsed
                .as_object()
                .map(|object| object.keys().collect::<Vec<_>>())
        );

        Ok(parsed)
    }

    fn get_text(value: &Value) -> Option<String> {
        if let Some(text) = value.get("simpleText").and_then(Value::as_str) {
            return Some(text.to_string());
        }

        value
            .get("runs")
            .and_then(Value::as_array)
            .map(|runs| {
                runs.iter()
                    .filter_map(|run| run.get("text").and_then(Value::as_str))
                    .collect::<String>()
            })
            .filter(|text| !text.is_empty())
    }

    fn parse_duration(value: &str) -> Option<u64> {
        let parts: Vec<u64> = value
            .split(':')
            .map(str::parse::<u64>)
            .collect::<Result<Vec<_>, _>>()
            .ok()?;

        let seconds = match parts.as_slice() {
            [minutes, seconds] => minutes.checked_mul(60)?.checked_add(*seconds)?,
            [hours, minutes, seconds] => hours
                .checked_mul(3600)?
                .checked_add(minutes.checked_mul(60)?)?
                .checked_add(*seconds)?,
            _ => return None,
        };

        Some(seconds * 1000)
    }

    fn collect_videos(value: &Value, results: &mut Vec<SearchResult>, seen: &mut HashSet<String>) {
        match value {
            Value::Object(object) => {
                let video = object
                    .get("videoRenderer")
                    .or_else(|| object.get("videoWithContextRenderer"));

                if let Some(video) = video {
                    let id = video.get("videoId").and_then(Value::as_str).or_else(|| {
                        video
                            .get("navigationEndpoint")
                            .and_then(|v| v.get("watchEndpoint"))
                            .and_then(|v| v.get("videoId"))
                            .and_then(Value::as_str)
                    });

                    if let Some(id) = id {
                        if seen.insert(id.to_string()) {
                            let title = video
                                .get("title")
                                .or_else(|| video.get("headline"))
                                .and_then(Self::get_text);

                            if let Some(title) = title {
                                let creator = video
                                    .get("ownerText")
                                    .or_else(|| video.get("shortBylineText"))
                                    .and_then(Self::get_text);

                                let duration_ms = video
                                    .get("lengthText")
                                    .and_then(Self::get_text)
                                    .and_then(|text| Self::parse_duration(&text));

                                let artwork_url = video
                                    .get("thumbnail")
                                    .and_then(|v| v.get("thumbnails"))
                                    .and_then(Value::as_array)
                                    .and_then(|items| items.last())
                                    .and_then(|v| v.get("url"))
                                    .and_then(Value::as_str)
                                    .map(String::from);

                                results.push(SearchResult {
                                    title,
                                    creator,
                                    url: format!("https://www.youtube.com/watch?v={}", id),
                                    provider: "YouTube".to_string(),
                                    artwork_url,
                                    duration_ms,
                                    media_type: MediaType::Video,
                                });
                            }
                        }
                    }
                }

                for child in object.values() {
                    Self::collect_videos(child, results, seen);
                }
            }

            Value::Array(items) => {
                for item in items {
                    Self::collect_videos(item, results, seen);
                }
            }

            _ => {}
        }
    }
}

impl SearchProvider for YouTubeProvider {
    fn name(&self) -> &'static str {
        "YouTube"
    }

    fn search(&self, query: &str) -> Result<Vec<SearchResult>, ProviderError> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }

        let response = self
            .client
            .get("https://www.youtube.com/results")
            .query(&[("search_query", query)])
            .send()
            .map_err(|e| format!("YouTube request failed: {}", e))?
            .error_for_status()
            .map_err(|e| format!("YouTube rejected the request: {}", e))?;

        let html = response
            .text()
            .map_err(|e| format!("Could not read YouTube response: {}", e))?;

        let data = Self::extract_initial_data(&html)?;

        let mut results = Vec::new();
        let mut seen = HashSet::new();

        Self::collect_videos(&data, &mut results, &mut seen);

        Ok(results)
    }
}
