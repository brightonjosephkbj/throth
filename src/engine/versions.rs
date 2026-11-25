pub fn classify(title: &str) -> String {
    let title = title
        .to_lowercase()
        .replace('-', " ")
        .replace('_', " ")
        .replace('+', " ");

    let mut versions: Vec<&str> = Vec::new();

    if title.contains("super slowed") || title.contains("ultra slowed") {
        versions.push("Super Slowed");
    } else if title.contains("slowed") {
        versions.push("Slowed");
    }

    if title.contains("sped up") || title.contains("speed up") {
        versions.push("Sped Up");
    }

    if title.contains("reverb") {
        versions.push("Reverb");
    }

    if title.contains("remix") {
        versions.push("Remix");
    }

    if title.contains("extended") {
        versions.push("Extended");
    }

    if title.contains("nightcore") {
        versions.push("Nightcore");
    }

    if title.contains("instrumental") {
        versions.push("Instrumental");
    }

    if title.contains("acapella") || title.contains("a cappella") {
        versions.push("Acapella");
    }

    if title.contains("live") {
        versions.push("Live");
    }

    if title.contains("cover") {
        versions.push("Cover");
    }

    if title.contains("karaoke") {
        versions.push("Karaoke");
    }

    if versions.is_empty() {
        "No version tag detected".to_string()
    } else {
        versions.join(" + ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_super_slowed_reverb() {
        assert_eq!(
            classify("STRUCT (Super Slowed + Reverb)"),
            "Super Slowed + Reverb"
        );
    }

    #[test]
    fn detects_sped_up() {
        assert_eq!(classify("STRUCT Sped Up"), "Sped Up");
    }

    #[test]
    fn detects_remix() {
        assert_eq!(classify("STRUCT Remix"), "Remix");
    }
}
