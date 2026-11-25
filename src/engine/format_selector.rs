use crate::engine::resolver::MediaFormat;
use crate::providers::MediaType;

#[derive(Debug)]
pub struct FormatSelection<'a> {
    pub primary: &'a MediaFormat,
    pub fallbacks: Vec<&'a MediaFormat>,
}

/// Select the best resolved format and retain fallback candidates.
pub fn select_formats<'a>(
    formats: &'a [MediaFormat],
    desired_type: &MediaType,
) -> Option<FormatSelection<'a>> {
    let mut candidates: Vec<&MediaFormat> = formats
        .iter()
        .filter(|format| format.direct_url.is_some() && format.media_type == *desired_type)
        .collect();

    candidates.sort_by_key(|format| std::cmp::Reverse(format_score(format)));

    if candidates.is_empty() {
        return None;
    }

    let primary = candidates.remove(0);

    Some(FormatSelection {
        primary,
        fallbacks: candidates,
    })
}

fn format_score(format: &MediaFormat) -> u64 {
    let mut score = 100_000;

    let bitrate = format
        .bitrate_kbps
        .unwrap_or_else(|| preset_bitrate(&format.format_id));

    score += bitrate as u64 * 10;

    if let Some(mime) = &format.mime_type {
        if mime.starts_with("audio/mp4") {
            score += 500;
        } else if mime.starts_with("audio/") {
            score += 250;
        }
    }

    score
}

fn preset_bitrate(preset: &str) -> u32 {
    let digits: String = preset
        .rsplit('_')
        .next()
        .unwrap_or("")
        .trim_end_matches('k')
        .chars()
        .filter(|character| character.is_ascii_digit())
        .collect();

    digits.parse().unwrap_or(0)
}
