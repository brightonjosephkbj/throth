#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackIdentity {
    pub base_title: String,
    pub artist: Option<String>,
    pub group_key: String,
}

fn normalize(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

fn is_query_noise(word: &str) -> bool {
    [
        "audio",
        "video",
        "artist",
        "slowed",
        "superslowed",
        "ultraslowed",
        "speedup",
        "speededup",
        "reverb",
        "remix",
        "extended",
        "nightcore",
        "instrumental",
        "acapella",
        "karaoke",
        "lyrics",
        "lyric",
        "live",
    ]
    .contains(&word)
}

fn has_version_tag(value: &str) -> bool {
    let value = value.to_lowercase();

    [
        "super slowed",
        "ultra slowed",
        "slowed",
        "sped up",
        "speed up",
        "reverb",
        "remix",
        "extended",
        "nightcore",
        "instrumental",
        "acapella",
        "a cappella",
        "karaoke",
        "official audio",
        "official video",
        "lyrics",
        "lyric video",
        "visualizer",
        "edit",
        "bootleg",
        "flip",
        "mashup",
        "free download",
        "free dl",
        "live",
    ]
    .iter()
    .any(|tag| value.contains(tag))
}

fn clean_title(title: &str) -> String {
    let mut cleaned = title.to_string();

    // Remove bracketed version and promotional descriptions.
    for open in ['(', '[', '{'] {
        let close = match open {
            '(' => ')',
            '[' => ']',
            _ => '}',
        };

        let mut search_from = 0;

        while search_from < cleaned.len() {
            let Some(relative_start) = cleaned[search_from..].find(open) else {
                break;
            };

            let start = search_from + relative_start;

            let Some(relative_end) = cleaned[start + 1..].find(close) else {
                break;
            };

            let end = start + 1 + relative_end;
            let remove = has_version_tag(&cleaned[start + 1..end]);

            if remove {
                cleaned.replace_range(start..=end, " ");
                search_from = start;
            } else {
                search_from = end + 1;
            }
        }
    }

    // Remove featured-artist suffixes.
    let lower = cleaned.to_lowercase();

    for marker in [" feat. ", " feat ", " ft. ", " ft "] {
        if let Some(position) = lower.find(marker) {
            cleaned.truncate(position);
            break;
        }
    }

    // Remove common version and promotional labels.
    let tags = [
        "super slowed",
        "ultra slowed",
        "slowed",
        "sped up",
        "speed up",
        "reverb",
        "remix",
        "extended",
        "nightcore",
        "instrumental",
        "acapella",
        "a cappella",
        "karaoke",
        "official audio",
        "official video",
        "lyric video",
        "lyrics",
        "visualizer",
        "free download",
        "free dl",
        "clean version",
    ];

    for tag in tags {
        let lower = cleaned.to_lowercase();

        if let Some(position) = lower.find(tag) {
            cleaned.replace_range(position..position + tag.len(), " ");
        }
    }

    // Remove live-event suffixes while retaining the song name.
    let lower = cleaned.to_lowercase();

    for marker in [" live @", " live in ", " live at "] {
        if let Some(position) = lower.find(marker) {
            cleaned.truncate(position);
            break;
        }
    }

    cleaned = cleaned
        .replace('|', " ")
        .replace('–', " ")
        .replace('—', " ")
        .replace('-', " ")
        .replace('_', " ")
        .replace('+', " ")
        .replace(':', " ")
        .replace('"', " ")
        .replace('\'', " ");

    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn split_artist_title(title: &str) -> Option<(&str, &str)> {
    for (index, character) in title.char_indices() {
        if character == '-' || character == '–' || character == '—' {
            let left = title[..index].trim();
            let right = title[index + character.len_utf8()..].trim();

            if !left.is_empty() && !right.is_empty() {
                return Some((left, right));
            }
        }
    }

    None
}

fn words_match(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }

    let left: Vec<char> = a.chars().collect();
    let right: Vec<char> = b.chars().collect();

    // Only allow fuzzy matching for reasonably long words.
    if left.len().max(right.len()) < 4 || left.len().abs_diff(right.len()) > 1 {
        return false;
    }

    let mut differences = 0;
    let mut i = 0;
    let mut j = 0;

    while i < left.len() && j < right.len() {
        if left[i] == right[j] {
            i += 1;
            j += 1;
        } else {
            differences += 1;

            if differences > 1 {
                return false;
            }

            if left.len() > right.len() {
                i += 1;
            } else if right.len() > left.len() {
                j += 1;
            } else {
                i += 1;
                j += 1;
            }
        }
    }

    if i < left.len() || j < right.len() {
        differences += 1;
    }

    differences <= 1
}

fn query_score(candidate: &str, query: &str) -> (usize, usize) {
    let candidate_words: Vec<String> = candidate
        .split_whitespace()
        .map(normalize)
        .filter(|word| !word.is_empty())
        .collect();

    let query_words: Vec<String> = query
        .split_whitespace()
        .map(normalize)
        .filter(|word| !word.is_empty() && !is_query_noise(word))
        .collect();

    let matched: Vec<usize> = query_words
        .iter()
        .enumerate()
        .filter(|(_, word)| {
            candidate_words
                .iter()
                .any(|candidate| words_match(candidate, word))
        })
        .map(|(index, _)| index)
        .collect();

    (
        matched.len(),
        matched.first().copied().unwrap_or(usize::MAX),
    )
}

pub fn identify(title: &str, _creator: Option<&str>, query: &str) -> TrackIdentity {
    let (base_title, artist) = match split_artist_title(title) {
        Some((left, right)) => {
            let left_clean = clean_title(left);
            let right_clean = clean_title(right);

            let left_score = query_score(&left_clean, query);
            let right_score = query_score(&right_clean, query);

            let choose_left = if left_score.1 != right_score.1 {
                left_score.1 < right_score.1
            } else if left_score.0 != right_score.0 {
                left_score.0 > right_score.0
            } else {
                // Fallback when the query cannot distinguish the sides.
                let left_words = left_clean.split_whitespace().count();
                let right_words = right_clean.split_whitespace().count();

                left_words > 0 && (right_words == 0 || left_words < right_words)
            };

            if choose_left {
                (left_clean, Some(right.to_string()))
            } else {
                (right_clean, Some(left.to_string()))
            }
        }
        None => (clean_title(title), None),
    };

    let group_key = normalize(&base_title);

    TrackIdentity {
        base_title,
        artist,
        group_key,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_artist_before_title() {
        let result = identify(
            "Travis Scott - FE!N ft. Playboi Carti",
            Some("Travis Scott"),
            "FE!N Travis Scott",
        );

        assert_eq!(result.base_title, "FE!N");
    }

    #[test]
    fn handles_title_before_artist() {
        let result = identify(
            "FE!N - Travis Scott (Sped Up)",
            Some("kilo"),
            "FE!N Travis Scott",
        );

        assert_eq!(result.base_title, "FE!N");
    }

    #[test]
    fn removes_lyrics_and_version_labels() {
        let first = identify("Travis Scott - FE!N (Lyrics)", None, "FE!N Travis Scott");

        let second = identify(
            "FE!N - Travis Scott (Slowed + Reverb)",
            None,
            "FE!N Travis Scott",
        );

        assert_eq!(first.group_key, second.group_key);
    }

    #[test]
    fn removes_edit_names_from_base_title() {
        let first = identify(
            "Travis Scott - FE!N (WESH Remix)",
            None,
            "FE!N Travis Scott",
        );

        let second = identify("Travis Scott - FE!N", None, "FE!N Travis Scott");

        assert_eq!(first.group_key, second.group_key);
    }

    #[test]
    fn handles_titles_without_artist_separator() {
        let result = identify("STRUCT (Super Slowed + Reverb)", None, "STRUCT slowed");

        assert_eq!(result.base_title, "STRUCT");
    }

    #[test]
    fn keeps_different_tracks_separate() {
        let first = identify("Travis Scott - FE!N", None, "FE!N Travis Scott");

        let second = identify("Travis Scott - SICKO MODE", None, "FE!N Travis Scott");

        assert_ne!(first.group_key, second.group_key);
    }

    #[test]
    fn identifies_live_recording_song_title() {
        let result = identify(
            "Travis Scott - FEIN LIVE @ CIRCUS MAXIMUS MILANO",
            None,
            "FE!N Travis Scott",
        );

        assert_eq!(result.base_title, "FEIN");
    }

    #[test]
    fn preserves_mashup_identity() {
        let result = identify("Booyah x FE!N (NOISETIME Edit)", None, "FE!N Travis Scott");

        assert_eq!(result.base_title, "Booyah x FE!N");
        assert_ne!(result.group_key, "fen");
    }

    #[test]
    fn does_not_confuse_artist_with_long_bootleg_title() {
        let result = identify(
            "Travis Scott - FE!N (NOCULA BOOTLEG) [Free Download]",
            None,
            "FE!N Travis Scott",
        );

        assert_eq!(result.base_title, "FE!N");
    }
}
