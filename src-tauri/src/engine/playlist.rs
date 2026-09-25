use std::path::Path;
use std::sync::OnceLock;

use crate::models::PlaylistItem;
use super::args::clean_filename;

/// Returns cached compiled regex for parsing playlist progress
pub fn get_playlist_regex() -> &'static regex::Regex {
    static PLAYLIST_REGEX: OnceLock<regex::Regex> = OnceLock::new();
    PLAYLIST_REGEX.get_or_init(|| {
        regex::Regex::new(r"Downloading (?:video|item) (\d+) of (\d+)").unwrap()
    })
}

/// Parses playlist progress from yt-dlp log lines.
/// Matches lines like:
///   "[download] Downloading video 1 of 15"
///   "[download] Downloading item 3 of 10"
pub fn parse_playlist_progress(line: &str) -> Option<(u32, u32)> {
    let re = get_playlist_regex();
    if let Some(caps) = re.captures(line) {
        if let (Some(idx_m), Some(total_m)) = (caps.get(1), caps.get(2)) {
            if let (Ok(idx), Ok(tot)) = (idx_m.as_str().parse::<u32>(), total_m.as_str().parse::<u32>()) {
                return Some((idx, tot));
            }
        }
    }
    None
}

#[derive(Debug, Eq, PartialEq)]
pub enum NaturalKeyPart {
    Str(String),
    Num(u64),
}

impl Ord for NaturalKeyPart {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            (NaturalKeyPart::Num(a), NaturalKeyPart::Num(b)) => a.cmp(b),
            (NaturalKeyPart::Str(a), NaturalKeyPart::Str(b)) => a.cmp(b),
            (NaturalKeyPart::Num(_), NaturalKeyPart::Str(_)) => std::cmp::Ordering::Less,
            (NaturalKeyPart::Str(_), NaturalKeyPart::Num(_)) => std::cmp::Ordering::Greater,
        }
    }
}

impl PartialOrd for NaturalKeyPart {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Tokenizes a string into alternating text and numeric components for natural sorting
/// e.g. "01 - Track.mp4" -> [Num(1), Str(" - track.mp4")]
pub fn natural_sort_key(s: &str) -> Vec<NaturalKeyPart> {
    let mut parts = Vec::new();
    let mut chars = s.chars().peekable();

    while let Some(&c) = chars.peek() {
        if c.is_ascii_digit() {
            let mut num_str = String::new();
            while let Some(&d) = chars.peek() {
                if d.is_ascii_digit() {
                    num_str.push(d);
                    chars.next();
                } else {
                    break;
                }
            }
            if let Ok(num) = num_str.parse::<u64>() {
                parts.push(NaturalKeyPart::Num(num));
            } else {
                parts.push(NaturalKeyPart::Str(num_str));
            }
        } else {
            let mut text = String::new();
            while let Some(&ch) = chars.peek() {
                if !ch.is_ascii_digit() {
                    text.push(ch.to_ascii_lowercase());
                    chars.next();
                } else {
                    break;
                }
            }
            parts.push(NaturalKeyPart::Str(text));
        }
    }

    parts
}

/// Scans a directory for downloaded media files (.mp4, .mkv, .webm, .mov, .avi, .mp3, .m4a, .wav, .flac, .aac, .opus),
/// extracts clean file metadata, and sorts files in natural track order.
pub fn scan_playlist_directory(dir_path: &str) -> Result<Vec<PlaylistItem>, String> {
    let raw_path = dir_path.trim().replace('/', "\\");
    let raw_clean = raw_path.trim_end_matches('\\');
    let path = Path::new(raw_clean);

    let allowed_extensions = [
        "mp4", "mkv", "webm", "mov", "avi", "mp3", "m4a", "wav", "flac", "aac", "opus",
    ];

    // 1. Robust directory resolution:
    // If the path is a file, ends with a known media extension, or doesn't exist but its parent does,
    // gracefully resolve to the parent directory.
    let has_media_ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| allowed_extensions.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false);

    let target_dir = if path.is_dir() {
        path
    } else if path.is_file() || has_media_ext {
        path.parent().unwrap_or(path)
    } else if !path.exists() {
        if let Some(parent) = path.parent() {
            if parent.exists() && parent.is_dir() {
                parent
            } else {
                path
            }
        } else {
            path
        }
    } else {
        path
    };

    if !target_dir.exists() {
        return Err(format!("Directory path does not exist: {}", target_dir.display()));
    }

    if !target_dir.is_dir() {
        return Err(format!("Not a directory: {}", target_dir.display()));
    }

    let mut items = Vec::new();

    let scan_dir_files = |dir: &Path, acc: &mut Vec<PlaylistItem>| -> std::io::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let entry_path = entry.path();
            if entry_path.is_file() {
                if let Some(ext) = entry_path.extension().and_then(|e| e.to_str()) {
                    let ext_lower = ext.to_ascii_lowercase();
                    if allowed_extensions.contains(&ext_lower.as_str()) {
                        let file_name = entry_path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or_default();

                        // Exclude partial or intermediate download files
                        if file_name.ends_with(".part")
                            || file_name.ends_with(".temp")
                            || file_name.ends_with(".ytdl")
                            || file_name.contains(".temp.")
                        {
                            continue;
                        }

                        let file_size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                        let norm_path = entry_path.to_string_lossy().replace('/', "\\");

                        let cleaned_fname = clean_filename(file_name);
                        let name = Path::new(&cleaned_fname)
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or(&cleaned_fname)
                            .to_string();

                        acc.push(PlaylistItem {
                            name,
                            file_path: norm_path,
                            file_size,
                            extension: ext_lower,
                        });
                    }
                }
            }
        }
        Ok(())
    };

    if let Err(e) = scan_dir_files(target_dir, &mut items) {
        return Err(format!(
            "Failed to read directory {}: {e}",
            target_dir.display()
        ));
    }

    // Fallback: If no media files found directly in target_dir, search immediate subdirectories
    // (e.g. when yt-dlp created a subfolder %(playlist_title)s inside save_path)
    if items.is_empty() {
        if let Ok(entries) = std::fs::read_dir(target_dir) {
            for entry in entries.flatten() {
                let sub_path = entry.path();
                if sub_path.is_dir() {
                    let _ = scan_dir_files(&sub_path, &mut items);
                    if !items.is_empty() {
                        break;
                    }
                }
            }
        }
    }

    // Natural sort tracks so "01 - ...", "02 - ...", "10 - ..." remain in correct playlist order
    items.sort_by(|a, b| natural_sort_key(&a.name).cmp(&natural_sort_key(&b.name)));

    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_playlist_progress() {
        assert_eq!(
            parse_playlist_progress("[download] Downloading video 1 of 15"),
            Some((1, 15))
        );
        assert_eq!(
            parse_playlist_progress("[download] Downloading item 3 of 20"),
            Some((3, 20))
        );
        assert_eq!(
            parse_playlist_progress("Downloading video 12 of 100"),
            Some((12, 100))
        );
        assert_eq!(
            parse_playlist_progress("[download] Destination: video.mp4"),
            None
        );
        assert_eq!(
            parse_playlist_progress("download-progress:45.2%|1.2MiB/s|00:15|video.mp4"),
            None
        );
    }

    #[test]
    fn test_natural_sort_key_ordering() {
        let mut titles = vec![
            "10 - Final Track",
            "01 - Intro",
            "2 - Second Track",
            "09 - Ninth Track",
            "02 - Bonus Track",
        ];

        titles.sort_by(|a, b| natural_sort_key(a).cmp(&natural_sort_key(b)));

        assert_eq!(
            titles,
            vec![
                "01 - Intro",
                "02 - Bonus Track",
                "2 - Second Track",
                "09 - Ninth Track",
                "10 - Final Track",
            ]
        );
    }

    #[test]
    fn test_scan_playlist_directory_dummy() {
        let temp_dir = std::env::temp_dir().join(format!(
            "test_playlist_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let f1 = temp_dir.join("02 - Song B.mp3");
        let f2 = temp_dir.join("01 - Song A.mp4");
        let f3 = temp_dir.join("03 - Song C.part"); // Should be ignored
        let f4 = temp_dir.join("notes.txt"); // Non-media, should be ignored

        std::fs::write(&f1, b"mp3 content").unwrap();
        std::fs::write(&f2, b"mp4 content").unwrap();
        std::fs::write(&f3, b"temp").unwrap();
        std::fs::write(&f4, b"text").unwrap();

        let items = scan_playlist_directory(&temp_dir.to_string_lossy()).unwrap();

        // Should contain 2 items, naturally sorted
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name, "01 - Song A");
        assert_eq!(items[0].extension, "mp4");
        assert_eq!(items[1].name, "02 - Song B");
        assert_eq!(items[1].extension, "mp3");

        // Test passing an existing individual file path: should resolve parent dir
        let items_from_file = scan_playlist_directory(&f1.to_string_lossy()).unwrap();
        assert_eq!(items_from_file.len(), 2);

        // Test passing a non-existent media file with special chars: should resolve parent dir
        let ghost_file = temp_dir.join("05 - Ghost Track with Special Char 09_00 PM  10_00 PM.mp4");
        let items_from_ghost = scan_playlist_directory(&ghost_file.to_string_lossy()).unwrap();
        assert_eq!(items_from_ghost.len(), 2);

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
