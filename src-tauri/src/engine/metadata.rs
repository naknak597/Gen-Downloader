use std::path::Path;
use serde_json::Value;
use tauri::AppHandle;
use tauri_plugin_shell::process::CommandEvent;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::models::MediaMetadata;
use super::binaries::spawn_ytdlp_process;

/// Helper to extract non-empty trimmed string from JSON matching any of candidate keys
pub fn extract_json_string(val: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(s) = val.get(*key).and_then(|v| v.as_str()) {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

/// Helper to resolve the best thumbnail URL from a JSON object
pub fn extract_json_thumbnail(val: &Value) -> Option<String> {
    if let Some(thumb) = val.get("thumbnail").and_then(|v| v.as_str()) {
        let trimmed = thumb.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    val.get("thumbnails").and_then(|v| v.as_array()).and_then(|arr| {
        arr.iter().rev().find_map(|item| {
            item.get("url")
                .and_then(|u| u.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
        })
    })
}

/// Helper to extract numeric duration from a JSON object
pub fn extract_json_duration(val: &Value) -> Option<u64> {
    val.get("duration")
        .and_then(|v| v.as_u64().or_else(|| v.as_f64().map(|d| d.round() as u64)))
}

/// Sorts and prioritizes subtitle language codes:
/// 1. "km" (Khmer)
/// 2. "km-orig" (Khmer Original)
/// 3. "en" (English)
/// 4. English variants ("en-US", "en-GB", etc.)
/// 5. Remaining languages in alphabetical order
pub fn sort_and_prioritize_subtitles(mut langs: Vec<String>) -> Vec<String> {
    langs.sort_by(|a, b| {
        let rank = |s: &str| -> usize {
            match s {
                "km" => 0,
                "km-orig" => 1,
                "en" => 2,
                l if l.starts_with("en-") || l.starts_with("en.") || l.starts_with("en_") => 3,
                _ => 4,
            }
        };
        rank(a).cmp(&rank(b)).then_with(|| a.cmp(b))
    });
    langs.dedup();
    langs
}

/// Helper to extract unique, prioritized subtitle languages from JSON (both manual and auto captions)
pub fn extract_available_subtitles(val: &Value) -> Vec<String> {
    let mut langs = std::collections::HashSet::new();

    let mut collect_keys = |container: &Value| {
        if let Some(obj) = container.get("subtitles").and_then(|v| v.as_object()) {
            for k in obj.keys() {
                let trimmed = k.trim();
                if !trimmed.is_empty() {
                    langs.insert(trimmed.to_string());
                }
            }
        }
        if let Some(obj) = container.get("automatic_captions").and_then(|v| v.as_object()) {
            for k in obj.keys() {
                let trimmed = k.trim();
                if !trimmed.is_empty() {
                    langs.insert(trimmed.to_string());
                }
            }
        }
    };

    collect_keys(val);

    if let Some(entries) = val.get("entries").and_then(|v| v.as_array()) {
        for entry in entries.iter().take(5) {
            collect_keys(entry);
        }
    }

    let list: Vec<String> = langs.into_iter().collect();
    sort_and_prioritize_subtitles(list)
}

/// Parses yt-dlp single-json dump into a structured MediaMetadata object,
/// supporting both direct single videos and playlists (_type == "playlist").
pub fn parse_media_metadata_json(raw_json: &str, fallback_url: &str) -> Result<MediaMetadata, String> {
    let trimmed = raw_json.trim();
    if trimmed.is_empty() {
        return Err("yt-dlp returned empty metadata output".to_string());
    }

    let json_val: Value = match serde_json::from_str::<Value>(trimmed) {
        Ok(v) => v,
        Err(_) => {
            if let (Some(start), Some(end)) = (raw_json.find('{'), raw_json.rfind('}')) {
                if start < end {
                    serde_json::from_str::<Value>(&raw_json[start..=end])
                        .map_err(|e| format!("Failed to parse metadata JSON: {e}"))?
                } else {
                    return Err("Failed to extract valid JSON from metadata response".to_string());
                }
            } else {
                return Err("Failed to parse metadata: response did not contain JSON".to_string());
            }
        }
    };

    let is_playlist = json_val
        .get("_type")
        .and_then(|v| v.as_str())
        .map(|t| t == "playlist" || t == "multi_video")
        .unwrap_or(false)
        || json_val.get("entries").is_some();

    let first_entry = if is_playlist {
        json_val
            .get("entries")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.iter().find(|item| !item.is_null()))
    } else {
        None
    };

    // 1. Title: top-level title, or first entry's title, or fallback
    let title = extract_json_string(&json_val, &["title"])
        .or_else(|| first_entry.and_then(|e| extract_json_string(e, &["title"])))
        .unwrap_or_else(|| "Unknown Title".to_string());

    // 2. Thumbnail: top-level thumbnail, or first entry's thumbnail
    let thumbnail = extract_json_thumbnail(&json_val)
        .or_else(|| first_entry.and_then(extract_json_thumbnail));

    // 3. Duration: top-level or first entry's duration
    let duration = extract_json_duration(&json_val)
        .or_else(|| first_entry.and_then(extract_json_duration));

    // 4. Uploader / Channel: top-level or first entry's channel
    let uploader = extract_json_string(&json_val, &["uploader", "channel", "creator", "uploader_id", "channel_id"])
        .or_else(|| first_entry.and_then(|e| extract_json_string(e, &["uploader", "channel", "creator", "uploader_id", "channel_id"])));

    // 5. Webpage URL: top-level webpage_url, or first entry's URL, or provided fallback
    let webpage_url = extract_json_string(&json_val, &["webpage_url", "original_url"])
        .or_else(|| first_entry.and_then(|e| extract_json_string(e, &["webpage_url", "original_url", "url"])))
        .unwrap_or_else(|| fallback_url.to_string());

    // 6. Subtitles: extract available subtitle languages from subtitles and automatic_captions
    let available_subtitles = extract_available_subtitles(&json_val);

    // 7. Description & Tags
    let description = extract_json_string(&json_val, &["description"])
        .or_else(|| first_entry.and_then(|e| extract_json_string(e, &["description"])));
    let tags = json_val
        .get("tags")
        .and_then(|t| t.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| item.as_str().map(|s| s.to_string()))
                .collect::<Vec<String>>()
        });

    Ok(MediaMetadata {
        title,
        thumbnail,
        duration,
        uploader,
        webpage_url,
        available_subtitles,
        description,
        tags,
    })
}

/// Fetches media preview metadata by running yt-dlp with optimized flags and an 8-second timeout
pub async fn fetch_media_metadata(
    app: &AppHandle,
    url: &str,
    cookies: Option<&str>,
) -> Result<MediaMetadata, String> {
    let trimmed_url = url.trim();
    if trimmed_url.is_empty() {
        return Err("Please provide a valid media URL".to_string());
    }

    // High-performance metadata extraction:
    // - `--skip-download`: do not fetch media streams
    // - `--no-playlist`: extract single video if URL has both v= and list=
    // - `--playlist-items 1`: if pure playlist URL, only extract first item instead of crawling all
    // - `--no-check-formats`: skip stream format probing (< 2 sec response)
    // - `--no-warnings` & `--ignore-errors`: suppress non-fatal warnings
    let mut args = vec![
        "--dump-single-json".to_string(),
        "--skip-download".to_string(),
        "--no-playlist".to_string(),
        "--playlist-items".to_string(),
        "1".to_string(),
        "--no-check-formats".to_string(),
        "--no-warnings".to_string(),
        "--ignore-errors".to_string(),
        "--js-runtimes".to_string(),
        "node".to_string(),
    ];

    if let Some(browser) = cookies.map(str::trim).filter(|c| !c.is_empty()) {
        args.push("--cookies-from-browser".to_string());
        args.push(browser.to_string());
        args.push("--compat-options".to_string());
        args.push("no-keep-subs".to_string());
    }

    args.push(trimmed_url.to_string());

    let (mut rx, child) = spawn_ytdlp_process(app, args)?;

    let timeout_duration = std::time::Duration::from_secs(8);

    let read_result = tokio::time::timeout(timeout_duration, async {
        let mut stdout_buf = Vec::new();
        let mut stderr_buf = Vec::new();
        let mut exit_code: Option<i32> = None;

        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stdout(bytes) => {
                    stdout_buf.extend_from_slice(&bytes);
                }
                CommandEvent::Stderr(bytes) => {
                    stderr_buf.extend_from_slice(&bytes);
                }
                CommandEvent::Error(err) => {
                    return Err(format!("yt-dlp execution error: {err}"));
                }
                CommandEvent::Terminated(payload) => {
                    exit_code = payload.code;
                    break;
                }
                _ => {}
            }
        }

        Ok((stdout_buf, stderr_buf, exit_code))
    })
    .await;

    let (stdout_buf, stderr_buf, exit_code) = match read_result {
        Ok(res) => res?,
        Err(_) => {
            // Process timed out: terminate child process to prevent lingering resource leaks
            #[cfg(windows)]
            {
                let pid = child.pid();
                let mut cmd = std::process::Command::new("taskkill");
                cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
                cmd.args(["/F", "/T", "/PID", &pid.to_string()]);
                let _ = cmd.output();
            }
            let _ = child.kill();
            return Err("Metadata request timed out after 8 seconds. Please check the URL or your connection.".to_string());
        }
    };

    let stdout_str = String::from_utf8_lossy(&stdout_buf);

    if exit_code != Some(0) && stdout_str.trim().is_empty() {
        let stderr_str = String::from_utf8_lossy(&stderr_buf);
        let mut error_lines: Vec<&str> = stderr_str
            .lines()
            .map(|l| l.trim())
            .filter(|l| l.starts_with("ERROR:"))
            .collect();

        if error_lines.is_empty() {
            error_lines = stderr_str
                .lines()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty())
                .collect();
        }

        let err_msg = if !error_lines.is_empty() {
            error_lines.join(" ")
        } else {
            format!("yt-dlp process exited with status {:?}", exit_code)
        };
        return Err(err_msg);
    }

    parse_media_metadata_json(&stdout_str, trimmed_url)
}

/// Reads the yt-dlp `.info.json` file, extracts title, keywords/tags, and description,
/// formats and writes them into a clean UTF-8 `.txt` file, and deletes the intermediate `.info.json`.
pub fn export_metadata_txt(json_path: &Path, output_txt_path: &Path) -> Result<(), String> {
    let content = std::fs::read_to_string(json_path)
        .map_err(|e| format!("Failed to read metadata JSON from {}: {e}", json_path.display()))?;

    let json_val: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse metadata JSON from {}: {e}", json_path.display()))?;

    let title = json_val
        .get("title")
        .and_then(|t| t.as_str())
        .unwrap_or("N/A");

    let tags_str = if let Some(tags_arr) = json_val.get("tags").and_then(|t| t.as_array()) {
        let tags: Vec<&str> = tags_arr.iter().filter_map(|t| t.as_str()).collect();
        if tags.is_empty() {
            "None".to_string()
        } else {
            tags.join(", ")
        }
    } else {
        "None".to_string()
    };

    let description = json_val
        .get("description")
        .and_then(|d| d.as_str())
        .unwrap_or("");

    let formatted_content = format!(
"==================================================
TITLE
==================================================
{title}

==================================================
KEYWORDS / TAGS
==================================================
{tags_str}

==================================================
DESCRIPTION
==================================================
{description}
"
    );

    std::fs::write(output_txt_path, formatted_content.as_bytes())
        .map_err(|e| format!("Failed to write metadata text file to {}: {e}", output_txt_path.display()))?;

    // Delete intermediate .info.json file to keep folder clean
    if let Err(e) = std::fs::remove_file(json_path) {
        eprintln!(
            "[metadata] Notice: could not remove intermediate JSON file {}: {e}",
            json_path.display()
        );
    }

    Ok(())
}

/// Scans target download directory and converts any generated `.info.json` files to `.txt`
pub fn process_downloaded_metadata(target_dir: &Path, final_file_path: Option<&str>) {
    // 1. Direct file stem resolution if a specific destination path is known
    if let Some(file_str) = final_file_path {
        let p = Path::new(file_str);
        if let Some(parent) = p.parent() {
            if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                let candidate_json = parent.join(format!("{stem}.info.json"));
                if candidate_json.exists() {
                    let txt_path = parent.join(format!("{stem}.txt"));
                    let _ = export_metadata_txt(&candidate_json, &txt_path);
                }
            }
        }
    }

    // 2. Scan directory (and 1 level of subdirectories) for all *.info.json files
    let scan_dir = |dir: &Path| {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if name.ends_with(".info.json") {
                            let base = &name[..name.len() - ".info.json".len()];
                            let txt_path = path.with_file_name(format!("{base}.txt"));
                            let _ = export_metadata_txt(&path, &txt_path);
                        }
                    }
                } else if path.is_dir() {
                    if let Ok(sub_entries) = std::fs::read_dir(&path) {
                        for sub_entry in sub_entries.flatten() {
                            let sub_path = sub_entry.path();
                            if sub_path.is_file() {
                                if let Some(sub_name) = sub_path.file_name().and_then(|n| n.to_str()) {
                                    if sub_name.ends_with(".info.json") {
                                        let base = &sub_name[..sub_name.len() - ".info.json".len()];
                                        let txt_path = sub_path.with_file_name(format!("{base}.txt"));
                                        let _ = export_metadata_txt(&sub_path, &txt_path);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    };

    scan_dir(target_dir);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_media_metadata_json() {
        let sample_json = r#"{
            "title": "Amazing Rust & Tauri Tutorial",
            "thumbnail": "https://i.ytimg.com/vi/12345/hqdefault.jpg",
            "duration": 734.8,
            "uploader": "Tech Channel",
            "webpage_url": "https://www.youtube.com/watch?v=12345"
        }"#;

        let meta = parse_media_metadata_json(sample_json, "https://default.url").unwrap();
        assert_eq!(meta.title, "Amazing Rust & Tauri Tutorial");
        assert_eq!(meta.thumbnail.as_deref(), Some("https://i.ytimg.com/vi/12345/hqdefault.jpg"));
        assert_eq!(meta.duration, Some(735));
        assert_eq!(meta.uploader.as_deref(), Some("Tech Channel"));
        assert_eq!(meta.webpage_url, "https://www.youtube.com/watch?v=12345");
    }

    #[test]
    fn test_parse_media_metadata_json_fallbacks() {
        let sample_json = r#"{
            "thumbnails": [{"url": "https://thumb1.jpg"}, {"url": "https://thumb2.jpg"}],
            "channel": "Fallback Channel"
        }"#;

        let meta = parse_media_metadata_json(sample_json, "https://fallback.url").unwrap();
        assert_eq!(meta.title, "Unknown Title");
        assert_eq!(meta.thumbnail.as_deref(), Some("https://thumb2.jpg"));
        assert_eq!(meta.duration, None);
        assert_eq!(meta.uploader.as_deref(), Some("Fallback Channel"));
        assert_eq!(meta.webpage_url, "https://fallback.url");
    }

    #[test]
    fn test_parse_media_metadata_json_playlist() {
        let sample_playlist_json = r#"{
            "_type": "playlist",
            "title": "Best Coding Music 2026",
            "uploader": "Lofi Beats",
            "webpage_url": "https://www.youtube.com/playlist?list=PL12345",
            "entries": [
                {
                    "title": "Track 1 - Ambient Flow",
                    "thumbnail": "https://i.ytimg.com/vi/track1/hqdefault.jpg",
                    "duration": 180,
                    "uploader": "Lofi Producer",
                    "webpage_url": "https://www.youtube.com/watch?v=track1"
                }
            ]
        }"#;

        let meta = parse_media_metadata_json(sample_playlist_json, "https://fallback.url").unwrap();
        assert_eq!(meta.title, "Best Coding Music 2026");
        assert_eq!(meta.thumbnail.as_deref(), Some("https://i.ytimg.com/vi/track1/hqdefault.jpg"));
        assert_eq!(meta.duration, Some(180));
        assert_eq!(meta.uploader.as_deref(), Some("Lofi Beats"));
        assert_eq!(meta.webpage_url, "https://www.youtube.com/playlist?list=PL12345");
    }

    #[test]
    fn test_parse_media_metadata_json_playlist_entry_fallback() {
        let sample_playlist_json = r#"{
            "_type": "playlist",
            "entries": [
                {
                    "title": "First Video In Anonymous Playlist",
                    "thumbnail": "https://i.ytimg.com/vi/first/hqdefault.jpg",
                    "duration": 240,
                    "channel": "Awesome Creator"
                }
            ]
        }"#;

        let meta = parse_media_metadata_json(sample_playlist_json, "https://fallback.url").unwrap();
        assert_eq!(meta.title, "First Video In Anonymous Playlist");
        assert_eq!(meta.thumbnail.as_deref(), Some("https://i.ytimg.com/vi/first/hqdefault.jpg"));
        assert_eq!(meta.duration, Some(240));
        assert_eq!(meta.uploader.as_deref(), Some("Awesome Creator"));
        assert_eq!(meta.webpage_url, "https://fallback.url");
    }

    #[test]
    fn test_sort_and_prioritize_subtitles() {
        let langs = vec![
            "es".to_string(),
            "en".to_string(),
            "km".to_string(),
            "de".to_string(),
            "km-orig".to_string(),
            "en-US".to_string(),
            "fr".to_string(),
            "km".to_string(), // duplicate
        ];

        let prioritized = sort_and_prioritize_subtitles(langs);
        assert_eq!(
            prioritized,
            vec!["km", "km-orig", "en", "en-US", "de", "es", "fr"]
        );
    }

    #[test]
    fn test_parse_media_metadata_json_with_subtitles() {
        let sample_json = r#"{
            "title": "Khmer Voice Dubbing Tutorial",
            "duration": 600,
            "webpage_url": "https://www.youtube.com/watch?v=sample",
            "subtitles": {
                "en": [{"ext": "vtt"}],
                "km": [{"ext": "vtt"}]
            },
            "automatic_captions": {
                "km-orig": [{"ext": "srv3"}],
                "en": [{"ext": "srv3"}],
                "zh": [{"ext": "srv3"}]
            }
        }"#;

        let meta = parse_media_metadata_json(sample_json, "https://sample.url").unwrap();
        assert_eq!(meta.title, "Khmer Voice Dubbing Tutorial");
        assert_eq!(meta.available_subtitles, vec!["km", "km-orig", "en", "zh"]);
    }

    #[test]
    fn test_export_metadata_txt_creation_and_cleanup() {
        let temp_dir = std::env::temp_dir().join(format!(
            "meta_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let json_path = temp_dir.join("test_video.info.json");
        let txt_path = temp_dir.join("test_video.txt");

        let sample_json = r#"{
            "title": "Awesome Tauri v2 Tutorial",
            "description": "Learn Rust and React for desktop development.\nSecond line of description.",
            "tags": ["tauri", "rust", "react", "desktop"]
        }"#;

        std::fs::write(&json_path, sample_json.as_bytes()).unwrap();
        assert!(json_path.exists());

        let result = export_metadata_txt(&json_path, &txt_path);
        assert!(result.is_ok());

        // Output .txt file must exist
        assert!(txt_path.exists());
        let txt_content = std::fs::read_to_string(&txt_path).unwrap();

        assert!(txt_content.contains("TITLE\n==================================================\nAwesome Tauri v2 Tutorial"));
        assert!(txt_content.contains("KEYWORDS / TAGS\n==================================================\ntauri, rust, react, desktop"));
        assert!(txt_content.contains("DESCRIPTION\n==================================================\nLearn Rust and React for desktop development."));

        // Intermediate .info.json must be deleted
        assert!(!json_path.exists());

        // Cleanup temp dir
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
