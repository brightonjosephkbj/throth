use crate::engine::resolver::{Availability, MediaFormat, MediaResolver, ResolvedMedia};
use reqwest::blocking::Client;
use serde_json::Value;

use super::provider::{MediaType, ProviderError, SearchProvider, SearchResult};

pub struct SoundCloudProvider {
    client: Client,
}

impl SoundCloudProvider {
    pub fn new() -> Result<Self, ProviderError> {
        let client = Client::builder()
            .user_agent("Mozilla/5.0")
            .timeout(std::time::Duration::from_secs(25))
            .build()
            .map_err(|e| e.to_string())?;

        Ok(Self { client })
    }

    fn discover_client_id(&self) -> Result<String, ProviderError> {
        let page = self
            .client
            .get("https://soundcloud.com/search/sounds")
            .query(&[("q", "music")])
            .send()
            .map_err(|e| format!("Search page request failed: {}", e))?
            .error_for_status()
            .map_err(|e| format!("Search page rejected: {}", e))?
            .text()
            .map_err(|e| e.to_string())?;

        let marker = "window.__sc_hydration =";

        let start = page
            .find(marker)
            .ok_or("SoundCloud hydration data not found")?
            + marker.len();

        let remaining = page[start..].trim_start();

        let end = remaining
            .find(";</script>")
            .ok_or("Hydration JSON boundary not found")?;

        let hydration: Value = serde_json::from_str(&remaining[..end])
            .map_err(|e| format!("Invalid hydration JSON: {}", e))?;

        let entries = hydration.as_array().ok_or("Invalid hydration structure")?;

        let client_id = entries
            .iter()
            .find(|entry| entry.get("hydratable").and_then(Value::as_str) == Some("apiClient"))
            .and_then(|entry| entry.get("data"))
            .and_then(|data| data.get("id"))
            .and_then(Value::as_str)
            .ok_or("SoundCloud API client ID not found")?;

        Ok(client_id.to_string())
    }
}

impl SearchProvider for SoundCloudProvider {
    fn name(&self) -> &'static str {
        "SoundCloud"
    }

    fn search(&self, query: &str) -> Result<Vec<SearchResult>, ProviderError> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }

        let client_id = self.discover_client_id()?;

        let response = self
            .client
            .get("https://api-v2.soundcloud.com/search/tracks")
            .query(&[
                ("q", query),
                ("limit", "20"),
                ("client_id", client_id.as_str()),
            ])
            .send()
            .map_err(|e| format!("SoundCloud search failed: {}", e))?
            .error_for_status()
            .map_err(|e| format!("SoundCloud API error: {}", e))?;

        let data: Value = response
            .json()
            .map_err(|e| format!("Invalid search response: {}", e))?;

        let tracks = data
            .get("collection")
            .and_then(Value::as_array)
            .ok_or("SoundCloud returned no track collection")?;

        let mut results = Vec::new();

        for track in tracks {
            let title = match track.get("title").and_then(Value::as_str) {
                Some(value) => value,
                None => continue,
            };

            let url = match track.get("permalink_url").and_then(Value::as_str) {
                Some(value) => value,
                None => continue,
            };

            let creator = track
                .get("user")
                .and_then(|user| user.get("username"))
                .and_then(Value::as_str)
                .map(String::from);

            let artwork_url = track
                .get("artwork_url")
                .and_then(Value::as_str)
                .or_else(|| {
                    track
                        .get("user")
                        .and_then(|user| user.get("avatar_url"))
                        .and_then(Value::as_str)
                })
                .map(|u| u.replace("-large.", "-t500x500."));

            let duration_ms = track.get("duration").and_then(Value::as_u64);

            results.push(SearchResult {
                title: title.to_string(),
                creator,
                url: url.to_string(),
                provider: self.name().to_string(),
                artwork_url,
                duration_ms,
                media_type: MediaType::Audio,
            });
        }

        Ok(results)
    }
}

impl MediaResolver for SoundCloudProvider {
    fn provider(&self) -> &'static str {
        "SoundCloud"
    }

    fn resolve(&self, result: &SearchResult) -> Result<ResolvedMedia, String> {
        let client_id = self.discover_client_id()?;

        let response = self
            .client
            .get("https://api-v2.soundcloud.com/resolve")
            .query(&[
                ("url", result.url.as_str()),
                ("client_id", client_id.as_str()),
            ])
            .send()
            .map_err(|e| format!("SoundCloud resolve request failed: {}", e))?;

        let status = response.status();

        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Ok(ResolvedMedia {
                title: result.title.clone(),
                provider: self.provider().to_string(),
                page_url: result.url.clone(),
                availability: Availability::Restricted,
                formats: Vec::new(),
                message: Some("SoundCloud restricted access to this track".into()),
            });
        }

        if status.as_u16() == 404 {
            return Ok(ResolvedMedia {
                title: result.title.clone(),
                provider: self.provider().to_string(),
                page_url: result.url.clone(),
                availability: Availability::Unavailable,
                formats: Vec::new(),
                message: Some("Track was not found".into()),
            });
        }

        let data: Value = response
            .error_for_status()
            .map_err(|e| format!("SoundCloud API error: {}", e))?
            .json()
            .map_err(|e| format!("Invalid SoundCloud response: {}", e))?;

        let transcodings = data
            .get("media")
            .and_then(|media| media.get("transcodings"))
            .and_then(Value::as_array);

        let mut formats = Vec::new();

        if let Some(items) = transcodings {
            for item in items {
                let format = item.get("format");

                let preset = item
                    .get("preset")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");

                let protocol = format
                    .and_then(|f| f.get("protocol"))
                    .and_then(Value::as_str)
                    .map(String::from);

                let mime_type = format
                    .and_then(|f| f.get("mime_type"))
                    .and_then(Value::as_str)
                    .map(String::from);

                let transcoding_url = item.get("url").and_then(Value::as_str);

                let playback_url = if let Some(url) = transcoding_url {
                    match self
                        .client
                        .get(url)
                        .query(&[("client_id", client_id.as_str())])
                        .send()
                    {
                        Ok(response) => {
                            let status = response.status();

                            if !status.is_success() {
                                println!("SoundCloud format {}: HTTP {}", preset, status);
                                None
                            } else {
                                match response.json::<Value>() {
                                    Ok(body) => {
                                        body.get("url").and_then(Value::as_str).map(String::from)
                                    }
                                    Err(error) => {
                                        println!(
                                            "SoundCloud format {}: invalid response ({})",
                                            preset, error
                                        );
                                        None
                                    }
                                }
                            }
                        }
                        Err(error) => {
                            println!("SoundCloud format {}: request failed ({})", preset, error);
                            None
                        }
                    }
                } else {
                    None
                };

                formats.push(MediaFormat {
                    format_id: preset.to_string(),
                    media_type: MediaType::Audio,
                    container: protocol,
                    mime_type,
                    codec: None,
                    bitrate_kbps: None,
                    width: None,
                    height: None,
                    file_size_bytes: None,
                    direct_url: playback_url,
                });
            }
        }

        let available = !formats.is_empty();

        Ok(ResolvedMedia {
            title: result.title.clone(),
            provider: self.provider().to_string(),
            page_url: result.url.clone(),
            availability: if available {
                Availability::Available
            } else {
                Availability::Unavailable
            },
            formats,
            message: Some(if available {
                "Playback formats advertised; direct playback not yet tested.".into()
            } else {
                "No playback formats advertised.".into()
            }),
        })
    }
}
