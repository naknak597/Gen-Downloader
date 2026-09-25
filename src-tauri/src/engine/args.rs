use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

use crate::models::DownloadPayload;
use super::binaries::{is_nvenc_functional, resolve_ffmpeg_path};

/// Strips intermediate stream identifiers and temporary extensions
/// e.g. "Video Title.f298.mp4" -> "Video Title.mp4"
///      "Video Title.f140.m4a" -> "Video Title.mp4"
///      "Video Title.temp.mp4" -> "Video Title.mp4"
///      "Video Title.mp4.part" -> "Video Title.mp4"
pub fn clean_filename(raw: &str) -> String {
    let path = Path::new(raw);
    let fname = path.file_name().and_then(|f| f.to_str()).unwrap_or(raw);

    let mut s = fname.to_string();

    // Strip .part if present
    if s.ends_with(".part") {
        s.truncate(s.len() - 5);
    }

    // Strip .temp.mp4 or .temp
    s = s.replace(".temp.", ".");
    if s.ends_with(".temp") {
        s.truncate(s.len() - 5);
    }

    // Strip YouTube format tags like .f298 or .f140
    if let Some(idx) = s.rfind(".f") {
        let after = &s[idx + 2..];
        if let Some(dot_idx) = after.find('.') {
            let digits_part = &after[..dot_idx];
            if digits_part.chars().all(|c| c.is_ascii_digit()) {
                let ext = &after[dot_idx..];
                s = format!("{}{}", &s[..idx], ext);
            }
        }
    }

    s
}

/// Cleans temporary tags while preserving the full directory path
pub fn clean_filepath(raw: &str) -> String {
    let p = Path::new(raw);
    let fname = p.file_name().and_then(|f| f.to_str()).unwrap_or(raw);
    let cleaned_fname = clean_filename(fname);
    if let Some(parent) = p.parent() {
        if parent.as_os_str().is_empty() {
            cleaned_fname
        } else {
            parent.join(cleaned_fname).to_string_lossy().to_string()
        }
    } else {
        cleaned_fname
    }
}

/// Extracts raw destination path from yt-dlp log lines (Merger, Destination, etc.)
pub fn extract_destination_filepath(line: &str) -> Option<String> {
    if line.contains("[Merger] Merging formats into \"") {
        let prefix = "[Merger] Merging formats into \"";
        if let Some(start) = line.find(prefix) {
            let rest = &line[start + prefix.len()..];
            if let Some(end) = rest.rfind('"') {
                return Some(rest[..end].trim().to_string());
            }
        }
    } else if line.contains("Destination: ") {
        if let Some(start) = line.find("Destination: ") {
            let rest = line[start + "Destination: ".len()..].trim().trim_matches('"');
            if !rest.is_empty() {
                return Some(rest.to_string());
            }
        }
    }
    None
}

/// Extracts clean target filename from yt-dlp log lines (Merger, Destination, etc.)
pub fn extract_destination_filename(line: &str) -> Option<String> {
    if let Some(path_str) = extract_destination_filepath(line) {
        let fname = Path::new(&path_str).file_name()?.to_string_lossy().to_string();
        let cleaned = clean_filename(&fname);
        if !cleaned.is_empty() {
            return Some(cleaned);
        }
    }
    None
}

/// Resolves the base save directory, ensuring it exists on disk,
/// and returns a normalized cross-platform path string (using forward slashes).
pub fn resolve_safe_save_path(app: &AppHandle, save_path: Option<&str>) -> (PathBuf, String) {
    let target_dir: PathBuf = match save_path {
        Some(path_str) if !path_str.trim().is_empty() => {
            let trimmed = path_str.trim();
            PathBuf::from(trimmed)
        }
        _ => app
            .path()
            .download_dir()
            .unwrap_or_else(|_| PathBuf::from(".")),
    };

    // Ensure the base destination directory exists on disk
    if let Err(e) = std::fs::create_dir_all(&target_dir) {
        eprintln!(
            "[download] Warning: unable to create destination directory {}: {e}",
            target_dir.display()
        );
    }

    // Strip extended length prefix (\\?\ or \\?\UNC\) if present
    let raw_str = target_dir.to_string_lossy();
    let stripped = if let Some(s) = raw_str.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{s}")
    } else if let Some(s) = raw_str.strip_prefix(r"\\?\") {
        s.to_string()
    } else {
        raw_str.to_string()
    };

    // Normalize Windows backslashes to forward slashes for safe cross-platform CLI arguments
    let normalized = stripped.replace('\\', "/");
    let clean_path = normalized.trim_end_matches('/').to_string();

    (target_dir, clean_path)
}

/// Builds the output template string for yt-dlp based on playlist status and save directory.
pub fn build_output_template(clean_save_path: &str, is_playlist: bool) -> String {
    if is_playlist {
        if clean_save_path.is_empty() || clean_save_path == "." {
            "%(playlist_title)s/%(playlist_index)02d - %(title)s.%(ext)s".to_string()
        } else {
            format!("{clean_save_path}/%(playlist_title)s/%(playlist_index)02d - %(title)s.%(ext)s")
        }
    } else {
        if clean_save_path.is_empty() || clean_save_path == "." {
            "%(title)s.%(ext)s".to_string()
        } else {
            format!("{clean_save_path}/%(title)s.%(ext)s")
        }
    }
}

/// Internal pure argument builder for yt-dlp execution
pub fn build_arguments_internal(ffmpeg_path_opt: Option<String>, payload: &DownloadPayload) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "--newline".into(),
        "--no-colors".into(),
        "--progress".into(),
        // Structured progress template for easy line parsing
        "--progress-template".into(),
        "download-progress:%(progress._percent_str)s|%(progress._speed_str)s|%(progress._eta_str)s|%(progress.filename)s".into(),
        // Strictly ensure intermediate stream files are deleted after muxing
        "--no-keep-video".into(),
        "--no-warnings".into(),
        // Utilize system Node.js runtime for JavaScript challenge & signature solving
        "--js-runtimes".into(),
        "node".into(),
    ];

    // Playlist handling & error resilience for unavailable items
    if payload.is_playlist {
        args.push("--yes-playlist".into());
        args.push("--ignore-errors".into());
        args.push("--no-abort-on-error".into());
    } else {
        args.push("--no-playlist".into());
    }

    // Pass resolved absolute FFmpeg location
    if let Some(ref ffmpeg_path) = ffmpeg_path_opt {
        args.push("--ffmpeg-location".into());
        args.push(ffmpeg_path.clone());
    }

    // Browser Cookies handling for age-restricted / login-gated videos (e.g. Brave, Chrome, Firefox)
    if let Some(ref browser) = payload.browser_cookies {
        let browser_trimmed = browser.trim();
        if !browser_trimmed.is_empty() {
            args.push("--cookies-from-browser".into());
            args.push(browser_trimmed.to_string());
            if !args.iter().any(|a| a == "--compat-options") {
                args.push("--compat-options".into());
                args.push("no-keep-subs".into());
            }
        }
    }

    // Subtitle download handling: strictly original spoken language (creator or auto-caption)
    if payload.download_subtitle {
        args.push("--write-subs".into());
        args.push("--write-auto-subs".into());
        args.push("--sub-langs".into());
        args.push("orig,.*-orig,default,km.*,en.*".into());
        args.push("--convert-subs".into());
        args.push("srt".into());
        if !args.iter().any(|a| a == "--compat-options") {
            args.push("--compat-options".into());
            args.push("no-keep-subs".into());
        }

        // 1. Bypass YouTube TimedText Rate Limits (HTTP Error 429: Too Many Requests)
        args.push("--sleep-subtitles".into());
        args.push("2".into());

        // 2. Prevent Subtitle Errors from Killing the Entire Download
        args.push("--no-abort-on-error".into());
        args.push("--ignore-no-formats-error".into());

        // 3. Retry Strategy: handle temporary 429 hiccups gracefully
        args.push("--extractor-retries".into());
        args.push("3".into());
        args.push("--retry-sleep".into());
        args.push("extractor:3".into());
    }

    // HD Thumbnail download handling (converted to .jpg)
    if payload.download_thumbnail {
        args.push("--write-thumbnail".into());
        args.push("--convert-thumbnails".into());
        args.push("jpg".into());
    }

    // Video Metadata .txt export handling (write intermediate .info.json)
    if payload.download_metadata {
        args.push("--write-info-json".into());
    }

    // Format & Transcoding Flags
    if payload.is_audio() {
        let raw_audio_fmt = payload.format_type.to_lowercase();
        let audio_fmt = if raw_audio_fmt == "audio" || raw_audio_fmt.trim().is_empty() {
            payload.resolved_audio_ext().to_string()
        } else {
            raw_audio_fmt
        };

        // High quality audio extraction cleanly removing source container
        args.push("-x".into());
        args.push("--audio-format".into());
        args.push(audio_fmt);
        args.push("--audio-quality".into());
        args.push("0".into());

        // Safe non-interactive FFmpeg parameters preventing stdin hangs
        args.push("--postprocessor-args".into());
        args.push("ffmpeg:-nostdin -y".into());
    } else {
        let video_ext = payload.resolved_video_ext();
        let q = payload.quality.trim().to_lowercase();
        let max_h = match q.as_str() {
            "4k" | "2160p" | "2160" => "2160",
            "1080p" | "1080" => "1080",
            "720p" | "720" => "720",
            _ => "best",
        };

        let format_selector = if max_h != "best" {
            format!("bestvideo[height<={max_h}]+bestaudio/best[height<={max_h}]/best")
        } else {
            "bestvideo+bestaudio/best".to_string()
        };
        args.push("-f".into());
        args.push(format_selector);

        // Dynamically handle container constraints:
        // - MKV: fast remux via --merge-output-format mkv (accepts VP9, AV1, H264, Opus, AAC without recoding)
        // - WebM: fast remux via --merge-output-format webm (YouTube natively streams VP9 + Opus in WebM, DO NOT recode)
        // - MP4: merge output format mp4 and transcode audio to AAC if source is Opus for universal compatibility
        // - MOV / AVI: transcode streams via --recode-video into container-compliant codecs
        match video_ext {
            "mkv" => {
                args.push("--merge-output-format".into());
                args.push("mkv".into());
            }
            "webm" => {
                args.push("--merge-output-format".into());
                args.push("webm".into());
            }
            "mp4" => {
                args.push("--merge-output-format".into());
                args.push("mp4".into());
                args.push("--postprocessor-args".into());
                args.push("Merger:-c:a aac -b:a 192k".into());
            }
            "mov" | "avi" => {
                args.push("--recode-video".into());
                args.push(video_ext.into());
            }
            _ => {
                args.push("--merge-output-format".into());
                args.push("mp4".into());
            }
        }

        // Safe non-interactive FFmpeg postprocessor parameters preventing stdin hangs & overwrite prompts
        args.push("--postprocessor-args".into());
        args.push("ffmpeg:-nostdin -y".into());

        // Ensure GPU postprocessor flags only apply to compatible formats (e.g. MOV hardware encoding)
        // and do NOT conflict with WebM (which requires VP9/Opus, rejecting H.264 NVENC) or AVI
        let can_use_nvenc = payload.use_gpu
            && video_ext == "mov"
            && ffmpeg_path_opt
                .as_ref()
                .map(|p| is_nvenc_functional(p))
                .unwrap_or(false);

        if can_use_nvenc {
            args.push("--postprocessor-args".into());
            args.push("VideoConvertor:-nostdin -y -c:v h264_nvenc -preset p4 -cq 23 -c:a aac -b:a 192k".into());
        }
    }

    // Save path & Output filename template
    let clean_save_path = match payload.save_path.as_deref() {
        Some(p) if !p.trim().is_empty() => p.trim().replace('\\', "/").trim_end_matches('/').to_string(),
        _ => ".".to_string(),
    };
    let output_template = build_output_template(&clean_save_path, payload.is_playlist);

    args.push("-o".into());
    args.push(output_template);

    // Target URL
    args.push(payload.url.clone());

    args
}

/// Pure argument builder for yt-dlp execution
pub fn build_arguments(app: &AppHandle, payload: &DownloadPayload) -> Vec<String> {
    let (target_dir, _) = resolve_safe_save_path(app, payload.save_path.as_deref());
    let mut payload_with_safe_path = payload.clone();
    payload_with_safe_path.save_path = Some(target_dir.to_string_lossy().to_string());

    let ffmpeg_path_opt = resolve_ffmpeg_path(Some(app));
    build_arguments_internal(ffmpeg_path_opt, &payload_with_safe_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_output_template_playlist() {
        let template = build_output_template("C:/Downloads", true);
        assert_eq!(
            template,
            "C:/Downloads/%(playlist_title)s/%(playlist_index)02d - %(title)s.%(ext)s"
        );
    }

    #[test]
    fn test_build_output_template_single_video() {
        let template = build_output_template("C:/Downloads", false);
        assert_eq!(template, "C:/Downloads/%(title)s.%(ext)s");
    }

    #[test]
    fn test_build_output_template_empty_or_relative() {
        assert_eq!(
            build_output_template(".", true),
            "%(playlist_title)s/%(playlist_index)02d - %(title)s.%(ext)s"
        );
        assert_eq!(
            build_output_template("", true),
            "%(playlist_title)s/%(playlist_index)02d - %(title)s.%(ext)s"
        );
        assert_eq!(
            build_output_template(".", false),
            "%(title)s.%(ext)s"
        );
        assert_eq!(
            build_output_template("", false),
            "%(title)s.%(ext)s"
        );
    }

    #[test]
    fn test_clean_filename_handling() {
        assert_eq!(
            clean_filename(r"C:\Downloads\My Playlist\01 - Track Title.mp4"),
            "01 - Track Title.mp4"
        );
        assert_eq!(
            clean_filename("C:/Downloads/My Playlist/02 - Track Title.f140.m4a"),
            "02 - Track Title.m4a"
        );
        assert_eq!(
            clean_filename("03 - Track Title.temp.mp4"),
            "03 - Track Title.mp4"
        );
        assert_eq!(
            clean_filename(r"C:\Downloads\04 - Track Title.mp4.part"),
            "04 - Track Title.mp4"
        );
    }

    #[test]
    fn test_clean_filepath_handling() {
        let cleaned = clean_filepath(r"C:\Downloads\My Playlist\01 - Track.temp.mp4");
        assert_eq!(
            cleaned.replace('/', "\\"),
            r"C:\Downloads\My Playlist\01 - Track.mp4"
        );
        let cleaned2 = clean_filepath(r"C:\Downloads\02 - Track.f298.mp4");
        assert_eq!(
            cleaned2.replace('/', "\\"),
            r"C:\Downloads\02 - Track.mp4"
        );
    }

    #[test]
    fn test_extract_destination_filepath() {
        assert_eq!(
            extract_destination_filepath(r#"[Merger] Merging formats into "C:\Downloads\Video.mp4""#),
            Some(r"C:\Downloads\Video.mp4".to_string())
        );
        assert_eq!(
            extract_destination_filepath(r#"Destination: C:\Downloads\Audio.mp3"#),
            Some(r"C:\Downloads\Audio.mp3".to_string())
        );
    }

    #[test]
    fn test_build_arguments_with_subtitles_and_thumbnail() {
        let payload = DownloadPayload {
            task_id: Some("test_task".into()),
            url: "https://www.youtube.com/watch?v=subtest".into(),
            format_type: "video".into(),
            quality: "1080p".into(),
            is_playlist: false,
            use_gpu: false,
            save_path: Some("C:/Downloads".into()),
            download_subtitle: true,
            subtitle_lang: Some("orig".into()),
            download_thumbnail: true,
            download_metadata: false,
            browser_cookies: None,
        };

        let args = build_arguments_internal(Some("C:/ffmpeg/bin/ffmpeg.exe".into()), &payload);

        assert!(args.contains(&"--write-subs".to_string()));
        assert!(args.contains(&"--write-auto-subs".to_string()));
        assert!(args.contains(&"--sub-langs".to_string()));

        let sub_langs_pos = args.iter().position(|a| a == "--sub-langs").unwrap();
        assert_eq!(args[sub_langs_pos + 1], "orig,.*-orig,default,km.*,en.*");

        let convert_subs_pos = args.iter().position(|a| a == "--convert-subs").unwrap();
        assert_eq!(args[convert_subs_pos + 1], "srt");

        assert!(args.contains(&"--compat-options".to_string()));
        let compat_pos = args.iter().position(|a| a == "--compat-options").unwrap();
        assert_eq!(args[compat_pos + 1], "no-keep-subs");

        assert!(args.contains(&"--write-thumbnail".to_string()));
        let convert_thumb_pos = args.iter().position(|a| a == "--convert-thumbnails").unwrap();
        assert_eq!(args[convert_thumb_pos + 1], "jpg");

        assert!(args.contains(&"--sleep-subtitles".to_string()));
        let sleep_pos = args.iter().position(|a| a == "--sleep-subtitles").unwrap();
        assert_eq!(args[sleep_pos + 1], "2");

        assert!(args.contains(&"--no-abort-on-error".to_string()));
        assert!(args.contains(&"--ignore-no-formats-error".to_string()));

        assert!(args.contains(&"--extractor-retries".to_string()));
        let retries_pos = args.iter().position(|a| a == "--extractor-retries").unwrap();
        assert_eq!(args[retries_pos + 1], "3");

        assert!(args.contains(&"--retry-sleep".to_string()));
        let retry_sleep_pos = args.iter().position(|a| a == "--retry-sleep").unwrap();
        assert_eq!(args[retry_sleep_pos + 1], "extractor:3");
    }

    #[test]
    fn test_build_arguments_default_subtitle_lang() {
        let payload = DownloadPayload {
            task_id: None,
            url: "https://www.youtube.com/watch?v=subtest2".into(),
            format_type: "audio".into(),
            quality: "best".into(),
            is_playlist: false,
            use_gpu: false,
            save_path: None,
            download_subtitle: true,
            subtitle_lang: None,
            download_thumbnail: false,
            download_metadata: false,
            browser_cookies: None,
        };

        let args = build_arguments_internal(None, &payload);

        assert!(args.contains(&"--write-subs".to_string()));
        assert!(args.contains(&"--write-auto-subs".to_string()));
        let sub_langs_pos = args.iter().position(|a| a == "--sub-langs").unwrap();
        assert_eq!(args[sub_langs_pos + 1], "orig,.*-orig,default,km.*,en.*");
        let convert_subs_pos = args.iter().position(|a| a == "--convert-subs").unwrap();
        assert_eq!(args[convert_subs_pos + 1], "srt");
        assert!(args.contains(&"--compat-options".to_string()));
        assert!(!args.contains(&"--write-thumbnail".to_string()));
    }

    #[test]
    fn test_build_arguments_expanded_video_formats() {
        for ext in &["mp4", "mkv", "webm", "mov", "avi"] {
            let payload = DownloadPayload {
                task_id: None,
                url: "https://www.youtube.com/watch?v=vidtest".into(),
                format_type: ext.to_string(),
                quality: "best".into(),
                is_playlist: false,
                use_gpu: false,
                save_path: None,
                download_subtitle: false,
                subtitle_lang: None,
                download_thumbnail: false,
                download_metadata: false,
                browser_cookies: None,
            };

            let args = build_arguments_internal(None, &payload);

            let f_pos = args.iter().position(|a| a == "-f").unwrap();
            assert_eq!(args[f_pos + 1], "bestvideo+bestaudio/best");

            match *ext {
                "mkv" => {
                    assert!(args.contains(&"--merge-output-format".to_string()));
                    let merge_pos = args.iter().position(|a| a == "--merge-output-format").unwrap();
                    assert_eq!(args[merge_pos + 1], "mkv");
                }
                "webm" => {
                    assert!(args.contains(&"--merge-output-format".to_string()));
                    let merge_pos = args.iter().position(|a| a == "--merge-output-format").unwrap();
                    assert_eq!(args[merge_pos + 1], "webm");
                }
                "mp4" => {
                    assert!(args.contains(&"--merge-output-format".to_string()));
                    let merge_pos = args.iter().position(|a| a == "--merge-output-format").unwrap();
                    assert_eq!(args[merge_pos + 1], "mp4");
                    assert!(args.contains(&"Merger:-c:a aac -b:a 192k".to_string()));
                }
                "mov" | "avi" => {
                    assert!(args.contains(&"--recode-video".to_string()));
                    let recode_pos = args.iter().position(|a| a == "--recode-video").unwrap();
                    assert_eq!(args[recode_pos + 1], *ext);
                }
                _ => {}
            }
        }
    }

    #[test]
    fn test_build_arguments_expanded_audio_formats() {
        for ext in &["mp3", "m4a", "wav", "flac", "aac", "opus"] {
            let payload = DownloadPayload {
                task_id: None,
                url: "https://www.youtube.com/watch?v=audiotest".into(),
                format_type: ext.to_string(),
                quality: "best".into(),
                is_playlist: false,
                use_gpu: false,
                save_path: None,
                download_subtitle: false,
                subtitle_lang: None,
                download_thumbnail: false,
                download_metadata: false,
                browser_cookies: None,
            };

            let args = build_arguments_internal(None, &payload);
            assert!(args.contains(&"-x".to_string()));
            assert!(args.contains(&"--audio-format".to_string()));
            let format_pos = args.iter().position(|a| a == "--audio-format").unwrap();
            assert_eq!(args[format_pos + 1], *ext);

            assert!(args.contains(&"--audio-quality".to_string()));
            let qual_pos = args.iter().position(|a| a == "--audio-quality").unwrap();
            assert_eq!(args[qual_pos + 1], "0");

            // Clean AAC: Ensure non-existent postprocessor ExtractAudio flag is NOT injected
            assert!(!args.contains(&"ExtractAudio:-c:a aac".to_string()));
        }
    }

    #[test]
    fn test_dynamic_quality_format_selector() {
        let test_cases = [
            ("4k", "bestvideo[height<=2160]+bestaudio/best[height<=2160]/best"),
            ("2160p", "bestvideo[height<=2160]+bestaudio/best[height<=2160]/best"),
            ("2160", "bestvideo[height<=2160]+bestaudio/best[height<=2160]/best"),
            ("1080p", "bestvideo[height<=1080]+bestaudio/best[height<=1080]/best"),
            ("1080", "bestvideo[height<=1080]+bestaudio/best[height<=1080]/best"),
            ("720p", "bestvideo[height<=720]+bestaudio/best[height<=720]/best"),
            ("720", "bestvideo[height<=720]+bestaudio/best[height<=720]/best"),
            ("best", "bestvideo+bestaudio/best"),
            ("unknown", "bestvideo+bestaudio/best"),
        ];

        for (q, expected_format) in test_cases {
            let payload = DownloadPayload {
                task_id: None,
                url: "https://www.youtube.com/watch?v=qualtest".into(),
                format_type: "mp4".into(),
                quality: q.into(),
                is_playlist: false,
                use_gpu: false,
                save_path: None,
                download_subtitle: false,
                subtitle_lang: None,
                download_thumbnail: false,
                download_metadata: false,
                browser_cookies: None,
            };

            let args = build_arguments_internal(None, &payload);
            let f_pos = args.iter().position(|a| a == "-f").unwrap();
            assert_eq!(args[f_pos + 1], expected_format, "Failed for quality: {}", q);
        }
    }

    #[test]
    fn test_build_arguments_with_metadata_export() {
        let payload = DownloadPayload {
            task_id: None,
            url: "https://www.youtube.com/watch?v=metatest".into(),
            format_type: "mp4".into(),
            quality: "1080p".into(),
            is_playlist: false,
            use_gpu: false,
            save_path: None,
            download_subtitle: false,
            subtitle_lang: None,
            download_thumbnail: false,
            download_metadata: true,
            browser_cookies: None,
        };

        let args = build_arguments_internal(None, &payload);
        assert!(args.contains(&"--write-info-json".to_string()));

        let mut payload_no_meta = payload.clone();
        payload_no_meta.download_metadata = false;
        let args2 = build_arguments_internal(None, &payload_no_meta);
        assert!(!args2.contains(&"--write-info-json".to_string()));
    }

    #[test]
    fn test_build_arguments_with_browser_cookies() {
        let mut payload = DownloadPayload {
            task_id: None,
            url: "https://www.youtube.com/watch?v=cookietest".into(),
            format_type: "mp4".into(),
            quality: "1080p".into(),
            is_playlist: false,
            use_gpu: false,
            save_path: None,
            download_subtitle: false,
            subtitle_lang: None,
            download_thumbnail: false,
            download_metadata: false,
            browser_cookies: Some("brave".into()),
        };

        // Test Brave cookies
        let args_brave = build_arguments_internal(None, &payload);
        assert!(args_brave.contains(&"--cookies-from-browser".to_string()));
        let cookie_pos = args_brave.iter().position(|a| a == "--cookies-from-browser").unwrap();
        assert_eq!(args_brave[cookie_pos + 1], "brave");
        assert!(args_brave.contains(&"--compat-options".to_string()));
        let compat_pos = args_brave.iter().position(|a| a == "--compat-options").unwrap();
        assert_eq!(args_brave[compat_pos + 1], "no-keep-subs");

        // Test Firefox cookies
        payload.browser_cookies = Some("firefox".into());
        let args_firefox = build_arguments_internal(None, &payload);
        assert!(args_firefox.contains(&"--cookies-from-browser".to_string()));
        let cookie_pos_ff = args_firefox.iter().position(|a| a == "--cookies-from-browser").unwrap();
        assert_eq!(args_firefox[cookie_pos_ff + 1], "firefox");

        // When empty or whitespace, cookie flags should NOT be added
        payload.browser_cookies = Some("   ".into());
        let args_empty = build_arguments_internal(None, &payload);
        assert!(!args_empty.contains(&"--cookies-from-browser".to_string()));

        payload.browser_cookies = None;
        let args_none = build_arguments_internal(None, &payload);
        assert!(!args_none.contains(&"--cookies-from-browser".to_string()));
    }
}
