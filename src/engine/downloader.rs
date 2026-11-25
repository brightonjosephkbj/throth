use std::io::Read;
use std::time::Duration;

const MAX_SEGMENT_BYTES: u64 = 8 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 50 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct HlsManifest {
    pub is_master_playlist: bool,
    pub segment_count: usize,
    pub variant_count: usize,
    pub target_duration_seconds: Option<f64>,
    pub total_duration_seconds: f64,
    pub has_end_list: bool,
    pub segment_urls: Vec<String>,
    pub init_segment_url: Option<String>,
    pub encryption_method: Option<String>,
    pub has_byte_ranges: bool,
}

#[derive(Debug, Clone)]
pub struct HlsSegmentInfo {
    pub http_status: u16,
    pub content_type: Option<String>,
    pub byte_count: usize,
}

#[derive(Debug)]
pub struct HlsDownload {
    pub data: Vec<u8>,
    pub segment_count: usize,
    pub total_bytes: usize,
}

fn build_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("OME/0.1")
        .build()
        .map_err(|error| error.to_string())
}

pub fn inspect_hls_manifest(url: &str) -> Result<HlsManifest, String> {
    let client = build_client()?;

    let response = client
        .get(url)
        .send()
        .map_err(|error| format!("Manifest request failed: {}", error))?
        .error_for_status()
        .map_err(|error| format!("Manifest HTTP error: {}", error))?;

    let content = response
        .text()
        .map_err(|error| format!("Could not read manifest: {}", error))?;

    parse_hls_manifest(&content, url)
}

fn fetch_bytes(
    client: &reqwest::blocking::Client,
    url: &str,
    limit: u64,
) -> Result<Vec<u8>, String> {
    let response = client
        .get(url)
        .send()
        .map_err(|error| format!("Media request failed: {}", error))?
        .error_for_status()
        .map_err(|error| format!("Media HTTP error: {}", error))?;

    if let Some(length) = response.content_length() {
        if length > limit {
            return Err(format!("Media component exceeds the {} byte limit.", limit));
        }
    }

    let mut limited_response = response.take(limit + 1);
    let mut data = Vec::new();

    limited_response
        .read_to_end(&mut data)
        .map_err(|error| format!("Could not read media component: {}", error))?;

    if data.len() as u64 > limit {
        return Err(format!("Media component exceeds the {} byte limit.", limit));
    }

    if data.is_empty() {
        return Err("The server returned an empty media component.".to_string());
    }

    Ok(data)
}

/// Retrieves an HLS media playlist and combines its initialization
/// segment and media fragments in playlist order.
///
/// The returned bytes are not yet verified as a playable audio file.
pub fn download_hls_media(manifest: &HlsManifest) -> Result<HlsDownload, String> {
    if manifest.is_master_playlist {
        return Err("Master playlists must be resolved to a media playlist first.".to_string());
    }

    if !manifest.has_end_list {
        return Err("Live or incomplete playlists are not supported yet.".to_string());
    }

    if manifest.segment_urls.is_empty() {
        return Err("The playlist contains no media segments.".to_string());
    }

    if manifest.has_byte_ranges {
        return Err("Byte-range playlists are not supported yet.".to_string());
    }

    if let Some(method) = &manifest.encryption_method {
        if !method.eq_ignore_ascii_case("NONE") {
            return Err(format!(
                "Encrypted HLS is not supported. Encryption method: {}",
                method
            ));
        }
    }

    let client = build_client()?;
    let mut data = Vec::new();

    if let Some(init_url) = &manifest.init_segment_url {
        let init_data = fetch_bytes(&client, init_url, MAX_SEGMENT_BYTES)?;

        if init_data.len() as u64 > MAX_TOTAL_BYTES {
            return Err("Total download exceeds the 50 MB limit.".to_string());
        }

        data.extend_from_slice(&init_data);
    }

    for (index, segment_url) in manifest.segment_urls.iter().enumerate() {
        let remaining = MAX_TOTAL_BYTES.saturating_sub(data.len() as u64);

        if remaining == 0 {
            return Err("Total download exceeds the 50 MB limit.".to_string());
        }

        let segment_limit = MAX_SEGMENT_BYTES.min(remaining);

        let segment_data = fetch_bytes(&client, segment_url, segment_limit)
            .map_err(|error| format!("Segment {} failed: {}", index + 1, error))?;

        data.extend_from_slice(&segment_data);
    }

    Ok(HlsDownload {
        total_bytes: data.len(),
        data,
        segment_count: manifest.segment_urls.len(),
    })
}

pub fn inspect_hls_segment(url: &str) -> Result<HlsSegmentInfo, String> {
    let client = build_client()?;

    let response = client
        .get(url)
        .send()
        .map_err(|error| format!("Segment request failed: {}", error))?
        .error_for_status()
        .map_err(|error| format!("Segment HTTP error: {}", error))?;

    let http_status = response.status().as_u16();

    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);

    if let Some(length) = response.content_length() {
        if length > MAX_SEGMENT_BYTES {
            return Err("Segment exceeds the 8 MB inspection limit.".to_string());
        }
    }

    let mut limited_response = response.take(MAX_SEGMENT_BYTES + 1);
    let mut data = Vec::new();

    limited_response
        .read_to_end(&mut data)
        .map_err(|error| format!("Could not read segment: {}", error))?;

    if data.len() as u64 > MAX_SEGMENT_BYTES {
        return Err("Segment exceeds the 8 MB inspection limit.".to_string());
    }

    if data.is_empty() {
        return Err("The server returned an empty segment.".to_string());
    }

    Ok(HlsSegmentInfo {
        http_status,
        content_type,
        byte_count: data.len(),
    })
}

fn resolve_uri(base_url: &str, uri: &str) -> Result<String, String> {
    reqwest::Url::parse(base_url)
        .and_then(|base| base.join(uri))
        .map(|url| url.to_string())
        .map_err(|error| format!("Could not resolve playlist URI: {}", error))
}

fn extract_quoted_attribute(line: &str, attribute: &str) -> Option<String> {
    let marker = format!("{}=\"", attribute);
    let start = line.find(&marker)? + marker.len();
    let remainder = &line[start..];
    let end = remainder.find('"')?;

    Some(remainder[..end].to_string())
}

fn parse_hls_manifest(content: &str, base_url: &str) -> Result<HlsManifest, String> {
    if !content.trim_start().starts_with("#EXTM3U") {
        return Err("Response is not a valid HLS playlist.".to_string());
    }

    let mut segment_urls = Vec::new();
    let mut variant_count = 0;
    let mut target_duration_seconds = None;
    let mut total_duration_seconds = 0.0;
    let mut is_master_playlist = false;
    let mut has_end_list = false;
    let mut expecting_segment = false;
    let mut expecting_variant = false;
    let mut init_segment_url = None;
    let mut encryption_method = None;
    let mut has_byte_ranges = false;

    for line in content.lines() {
        let line = line.trim();

        if let Some(value) = line.strip_prefix("#EXT-X-TARGETDURATION:") {
            target_duration_seconds = value.parse::<f64>().ok();
        }

        if line.starts_with("#EXT-X-STREAM-INF:") {
            is_master_playlist = true;
            expecting_variant = true;
        }

        if let Some(uri) =
            extract_quoted_attribute(line, "URI").filter(|_| line.starts_with("#EXT-X-MAP:"))
        {
            init_segment_url = Some(resolve_uri(base_url, &uri)?);
        }

        if let Some(attributes) = line.strip_prefix("#EXT-X-KEY:") {
            encryption_method = attributes
                .split(',')
                .find_map(|part| part.trim().strip_prefix("METHOD="))
                .map(str::to_string);
        }

        if line.starts_with("#EXT-X-BYTERANGE:") {
            has_byte_ranges = true;
        }

        if let Some(value) = line.strip_prefix("#EXTINF:") {
            let duration = value.split(',').next().unwrap_or("");

            if let Ok(duration) = duration.parse::<f64>() {
                total_duration_seconds += duration;
            }

            expecting_segment = true;
        }

        if line == "#EXT-X-ENDLIST" {
            has_end_list = true;
        }

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if expecting_variant {
            variant_count += 1;
            expecting_variant = false;
        } else if expecting_segment {
            segment_urls.push(resolve_uri(base_url, line)?);
            expecting_segment = false;
        }
    }

    Ok(HlsManifest {
        is_master_playlist,
        segment_count: segment_urls.len(),
        variant_count,
        target_duration_seconds,
        total_duration_seconds,
        has_end_list,
        segment_urls,
        init_segment_url,
        encryption_method,
        has_byte_ranges,
    })
}

#[cfg(test)]
mod tests {
    use super::parse_hls_manifest;

    #[test]
    fn parses_media_playlist() {
        let playlist = "#EXTM3U
#EXT-X-TARGETDURATION:6
#EXTINF:5.5,
segment1.m4s
#EXTINF:4.5,
segment2.m4s
#EXT-X-ENDLIST
";

        let result = parse_hls_manifest(playlist, "https://example.com/audio/index.m3u8").unwrap();

        assert_eq!(result.segment_count, 2);
        assert_eq!(result.total_duration_seconds, 10.0);
        assert!(result.has_end_list);
        assert!(!result.is_master_playlist);
        assert_eq!(
            result.segment_urls[0],
            "https://example.com/audio/segment1.m4s"
        );
    }

    #[test]
    fn parses_master_playlist() {
        let playlist = "#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=160000
high/index.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=96000
low/index.m3u8
";

        let result = parse_hls_manifest(playlist, "https://example.com/audio/master.m3u8").unwrap();

        assert!(result.is_master_playlist);
        assert_eq!(result.variant_count, 2);
    }

    #[test]
    fn rejects_invalid_playlist() {
        assert!(
            parse_hls_manifest(
                "not an HLS playlist",
                "https://example.com/audio/index.m3u8",
            )
            .is_err()
        );
    }

    #[test]
    fn resolves_absolute_segment_urls() {
        let playlist = "#EXTM3U
#EXTINF:5,
https://cdn.example.com/audio/segment.m4s
";

        let result = parse_hls_manifest(playlist, "https://example.com/audio/index.m3u8").unwrap();

        assert_eq!(
            result.segment_urls[0],
            "https://cdn.example.com/audio/segment.m4s"
        );
    }

    #[test]
    fn detects_initialization_segment() {
        let playlist = "#EXTM3U
#EXT-X-MAP:URI=\"init.mp4\"
#EXTINF:5,
segment1.m4s
";

        let result = parse_hls_manifest(playlist, "https://example.com/audio/index.m3u8").unwrap();

        assert_eq!(
            result.init_segment_url.as_deref(),
            Some("https://example.com/audio/init.mp4")
        );
    }

    #[test]
    fn detects_encryption_and_byte_ranges() {
        let playlist = "#EXTM3U
#EXT-X-KEY:METHOD=AES-128,URI=\"key.bin\"
#EXT-X-BYTERANGE:1000@0
#EXTINF:5,
segment.m4s
";

        let result = parse_hls_manifest(playlist, "https://example.com/audio/index.m3u8").unwrap();

        assert_eq!(result.encryption_method.as_deref(), Some("AES-128"));
        assert!(result.has_byte_ranges);
    }
}

/// Saves downloaded audio using a safe title-based filename.
/// Existing files are preserved; a numbered suffix is used instead.
pub fn save_audio_file(
    data: &[u8],
    directory: &str,
    title: &str,
) -> Result<std::path::PathBuf, String> {
    use std::fs;
    use std::path::PathBuf;

    let safe_name: String = title
        .chars()
        .map(|character| {
            if character.is_alphanumeric()
                || character == ' '
                || character == '-'
                || character == '_'
            {
                character
            } else {
                '_'
            }
        })
        .collect();

    let safe_name = safe_name.trim().trim_matches('.');

    let base_name = if safe_name.is_empty() {
        "audio"
    } else {
        safe_name
    };

    fs::create_dir_all(directory)
        .map_err(|error| format!("Could not create download folder: {}", error))?;

    let mut path = PathBuf::from(directory);
    path.push(format!("{}.m4a", base_name));

    let mut number = 2;

    while path.exists() {
        path = PathBuf::from(directory);
        path.push(format!("{} ({}).m4a", base_name, number));
        number += 1;
    }

    fs::write(&path, data).map_err(|error| format!("Could not save audio file: {}", error))?;

    Ok(path)
}
