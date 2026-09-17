use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tauri::{AppHandle, Manager};
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::models::{DownloadPayload, MediaMetadata, PlaylistItem};
use serde_json::Value;

// ---------------------------------------------------------------------------
// 1. Path Normalization & Binary Discovery
// ---------------------------------------------------------------------------

pub fn normalize_path_buf(path: &Path) -> String {
    if let Ok(canonical) = std::fs::canonicalize(path) {
        let s = canonical.to_string_lossy().to_string();
        if let Some(stripped) = s.strip_prefix(r"\\?\UNC\") {
            format!(r"\\{}", stripped)
        } else if let Some(stripped) = s.strip_prefix(r"\\?\") {
            stripped.to_string()
        } else {
            s
        }
    } else {
        path.to_string_lossy().to_string()
    }
}

pub fn resolve_ffmpeg_path(app: Option<&AppHandle>) -> Option<String> {
    const CANDIDATES: &[&str] = &[
        "ffmpeg.exe",
        "ffmpeg-x86_64-pc-windows-msvc.exe",
        "binaries/ffmpeg.exe",
        "binaries/ffmpeg-x86_64-pc-windows-msvc.exe",
        "src-tauri/binaries/ffmpeg.exe",
        "src-tauri/binaries/ffmpeg-x86_64-pc-windows-msvc.exe",
    ];

    // 1. Check next to running executable and its ancestors (production install & target/debug)
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(exe_dir) = current_exe.parent() {
            for name in CANDIDATES {
                let candidate = exe_dir.join(name);
                if candidate.is_file() {
                    let path_str = normalize_path_buf(&candidate);
                    println!("[ffmpeg] Resolved FFmpeg next to executable: {}", path_str);
                    return Some(path_str);
                }
            }

            // Up to 5 levels of ancestor directories (e.g. workspace root during dev)
            let mut cur = exe_dir;
            for _ in 0..5 {
                if let Some(parent) = cur.parent() {
                    for name in CANDIDATES {
                        let candidate = parent.join(name);
                        if candidate.is_file() {
                            let path_str = normalize_path_buf(&candidate);
                            println!("[ffmpeg] Resolved FFmpeg in ancestor directory: {}", path_str);
                            return Some(path_str);
                        }
                    }
                    cur = parent;
                } else {
                    break;
                }
            }
        }
    }

    // 2. Check resource directory if AppHandle is provided
    if let Some(app_handle) = app {
        if let Ok(resource_dir) = app_handle.path().resource_dir() {
            for name in CANDIDATES {
                let candidate = resource_dir.join(name);
                if candidate.is_file() {
                    let path_str = normalize_path_buf(&candidate);
                    println!("[ffmpeg] Resolved FFmpeg in resource directory: {}", path_str);
                    return Some(path_str);
                }
            }
        }
    }

    // 3. Check CWD (Current Working Directory)
    if let Ok(cwd) = std::env::current_dir() {
        for name in CANDIDATES {
            let candidate = cwd.join(name);
            if candidate.is_file() {
                let path_str = normalize_path_buf(&candidate);
                println!("[ffmpeg] Resolved FFmpeg in CWD: {}", path_str);
                return Some(path_str);
            }
        }
    }

    // 4. System PATH check
    #[cfg(windows)]
    {
        if let Ok(output) = std::process::Command::new("where.exe")
            .arg("ffmpeg")
            .output()
        {
            if output.status.success() {
                let out_str = String::from_utf8_lossy(&output.stdout);
                if let Some(first_line) = out_str.lines().next() {
                    let trimmed = first_line.trim();
                    if !trimmed.is_empty() {
                        let candidate = Path::new(trimmed);
                        if candidate.is_file() {
                            let path_str = normalize_path_buf(candidate);
                            println!("[ffmpeg] Resolved FFmpeg from system PATH: {}", path_str);
                            return Some(path_str);
                        }
                    }
                }
            }
        }
    }

    #[cfg(not(windows))]
    {
        if let Ok(output) = std::process::Command::new("which")
            .arg("ffmpeg")
            .output()
        {
            if output.status.success() {
                let out_str = String::from_utf8_lossy(&output.stdout);
                if let Some(first_line) = out_str.lines().next() {
                    let trimmed = first_line.trim();
                    if !trimmed.is_empty() {
                        let candidate = Path::new(trimmed);
                        if candidate.is_file() {
                            let path_str = normalize_path_buf(candidate);
                            println!("[ffmpeg] Resolved FFmpeg from system PATH: {}", path_str);
                            return Some(path_str);
                        }
                    }
                }
            }
        }
    }

    println!("[ffmpeg] WARNING: No bundled or system FFmpeg binary found; yt-dlp will rely on internal search.");
    None
}

/// Probes whether the resolved FFmpeg binary has functional NVENC hardware acceleration support
pub fn is_nvenc_functional(ffmpeg_path: &str) -> bool {
    static NVENC_CACHE: OnceLock<bool> = OnceLock::new();

    *NVENC_CACHE.get_or_init(|| {
        let mut cmd = std::process::Command::new(ffmpeg_path);

        #[cfg(windows)]
        {
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        cmd.args([
            "-f", "lavfi",
            "-i", "color=s=64x64:d=0.1",
            "-c:v", "h264_nvenc",
            "-f", "null",
            "-",
        ]);

        match cmd.output() {
            Ok(output) => {
                let supported = output.status.success();
                if supported {
                    println!("[ffmpeg] Hardware acceleration (h264_nvenc) verified and active.");
                } else {
                    let stderr_sample = String::from_utf8_lossy(&output.stderr);
                    let reason = stderr_sample
                        .lines()
                        .find(|l| l.contains("Required:") || l.contains("driver") || l.contains("Error"))
                        .unwrap_or("Hardware encoder unavailable");
                    println!(
                        "[ffmpeg] NVENC probe failed ({}); safely falling back to CPU libx264.",
                        reason.trim()
                    );
                }
                supported
            }
            Err(e) => {
                println!("[ffmpeg] Failed to execute NVENC probe: {e}; falling back to CPU libx264.");
                false
            }
        }
    })
}

pub fn find_ytdlp_executable(app: Option<&AppHandle>) -> Option<PathBuf> {
    const CANDIDATES: &[&str] = &[
        "yt-dlp.exe",
        "yt-dlp-x86_64-pc-windows-msvc.exe",
        "binaries/yt-dlp.exe",
        "binaries/yt-dlp-x86_64-pc-windows-msvc.exe",
        "src-tauri/binaries/yt-dlp.exe",
        "src-tauri/binaries/yt-dlp-x86_64-pc-windows-msvc.exe",
    ];

    // 1. Check next to running executable and its ancestors
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(exe_dir) = current_exe.parent() {
            for name in CANDIDATES {
                let candidate = exe_dir.join(name);
                if candidate.is_file() {
                    return Some(PathBuf::from(normalize_path_buf(&candidate)));
                }
            }

            let mut cur = exe_dir;
            for _ in 0..5 {
                if let Some(parent) = cur.parent() {
                    for name in CANDIDATES {
                        let candidate = parent.join(name);
                        if candidate.is_file() {
                            return Some(PathBuf::from(normalize_path_buf(&candidate)));
                        }
                    }
                    cur = parent;
                } else {
                    break;
                }
            }
        }
    }

    // 2. Check resource directory if AppHandle provided
    if let Some(app_handle) = app {
        if let Ok(resource_dir) = app_handle.path().resource_dir() {
            for name in CANDIDATES {
                let candidate = resource_dir.join(name);
                if candidate.is_file() {
                    return Some(PathBuf::from(normalize_path_buf(&candidate)));
                }
            }
        }
    }

    // 3. Check development binaries directory relative to CWD
    if let Ok(cwd) = std::env::current_dir() {
        for name in CANDIDATES {
            let candidate = cwd.join(name);
            if candidate.is_file() {
                return Some(PathBuf::from(normalize_path_buf(&candidate)));
            }
        }
    }

    None
}

/// Spawns yt-dlp process with prioritized sidecar resolution and fallback strategies
pub fn spawn_ytdlp_process(
    app: &AppHandle,
    args: Vec<String>,
) -> Result<(tauri::async_runtime::Receiver<CommandEvent>, CommandChild), String> {
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(exe_dir) = current_exe.parent() {
            let standard_path = exe_dir.join("yt-dlp.exe");
            let subfolder_path = exe_dir.join("binaries").join("yt-dlp.exe");
            println!("[yt-dlp] Base executable directory: {}", exe_dir.display());
            println!(
                "[yt-dlp] Standard sidecar path exists ({}): {}",
                standard_path.display(),
                standard_path.is_file()
            );
            println!(
                "[yt-dlp] Subfolder sidecar path exists ({}): {}",
                subfolder_path.display(),
                subfolder_path.is_file()
            );
        }
    }

    // Attempt 1: Standard Tauri sidecar identifier "yt-dlp"
    println!("[yt-dlp] Attempting spawn via sidecar('yt-dlp')...");
    match app.shell().sidecar("yt-dlp") {
        Ok(cmd) => match cmd.args(&args).spawn() {
            Ok((rx, child)) => {
                println!("[yt-dlp] Successfully spawned yt-dlp via sidecar('yt-dlp')");
                return Ok((rx, child));
            }
            Err(e) => {
                eprintln!("[yt-dlp] sidecar('yt-dlp').spawn() failed: {e}");
            }
        },
        Err(e) => {
            eprintln!("[yt-dlp] app.shell().sidecar('yt-dlp') configuration error: {e}");
        }
    }

    // Attempt 2: Path identifier "binaries/yt-dlp"
    println!("[yt-dlp] Attempting spawn via sidecar('binaries/yt-dlp')...");
    match app.shell().sidecar("binaries/yt-dlp") {
        Ok(cmd) => match cmd.args(&args).spawn() {
            Ok((rx, child)) => {
                println!("[yt-dlp] Successfully spawned yt-dlp via sidecar('binaries/yt-dlp')");
                return Ok((rx, child));
            }
            Err(e) => {
                eprintln!("[yt-dlp] sidecar('binaries/yt-dlp').spawn() failed: {e}");
            }
        },
        Err(e) => {
            eprintln!("[yt-dlp] app.shell().sidecar('binaries/yt-dlp') configuration error: {e}");
        }
    }

    // Attempt 3: Direct executable search fallback
    if let Some(direct_path) = find_ytdlp_executable(Some(app)) {
        let path_str = direct_path.to_string_lossy().to_string();
        println!("[yt-dlp] Attempting direct spawn from fallback executable: {}", path_str);
        let cmd = app.shell().command(path_str);
        match cmd.args(&args).spawn() {
            Ok((rx, child)) => {
                println!("[yt-dlp] Successfully spawned yt-dlp via direct fallback executable");
                return Ok((rx, child));
            }
            Err(e) => {
                eprintln!("[yt-dlp] Direct executable spawn failed: {e}");
            }
        }
    }

    // Attempt 4: System PATH fallback
    println!("[yt-dlp] Attempting spawn from system PATH 'yt-dlp'...");
    let cmd = app.shell().command("yt-dlp");
    match cmd.args(&args).spawn() {
        Ok((rx, child)) => {
            println!("[yt-dlp] Successfully spawned yt-dlp from system PATH");
            return Ok((rx, child));
        }
        Err(e) => {
            eprintln!("[yt-dlp] System PATH spawn failed: {e}");
        }
    }

    Err("Failed to spawn yt-dlp: sidecar, fallback executables, and system PATH all failed. Please check that yt-dlp is installed.".to_string())
}

// ---------------------------------------------------------------------------
// 2. Output Formatting & Filename Cleaners
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// 3. Regex Parsing Utilities
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// 4. Safe Path & Argument Construction
// ---------------------------------------------------------------------------

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

    // Browser Cookies handling for age-restricted / login-gated videos
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
        args.push(audio_fmt.clone());
        args.push("--audio-quality".into());
        args.push("0".into());

        // Ensure FFmpeg encodes with the native AAC codec cleanly
        if audio_fmt == "aac" {
            args.push("--postprocessor-args".into());
            args.push("ExtractAudio:-c:a aac".into());
        }

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

// ---------------------------------------------------------------------------
// 5. Playlist Scanning & Natural Sort
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// 5b. Media Preview & Metadata Fetching
// ---------------------------------------------------------------------------

/// Helper to extract non-empty trimmed string from JSON matching any of candidate keys
fn extract_json_string(val: &Value, keys: &[&str]) -> Option<String> {
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
fn extract_json_thumbnail(val: &Value) -> Option<String> {
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
fn extract_json_duration(val: &Value) -> Option<u64> {
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


// ---------------------------------------------------------------------------
// 6. Unit Tests
// ---------------------------------------------------------------------------


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
    fn test_path_normalization_slashes() {
        let test_cases = vec![
            (r"C:\Users\User\Downloads", "C:/Users/User/Downloads"),
            (r"C:\Users\User\Downloads\", "C:/Users/User/Downloads"),
            ("C:/Users/User/Downloads/", "C:/Users/User/Downloads"),
            (r"D:\Downloads///", "D:/Downloads"),
        ];

        for (input, expected) in test_cases {
            let normalized = input.replace('\\', "/");
            let clean = normalized.trim_end_matches('/').to_string();
            assert_eq!(clean, expected);
        }
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
        let temp_dir = std::env::temp_dir().join(format!("test_playlist_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()));
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

            if *ext == "aac" {
                assert!(args.contains(&"--postprocessor-args".to_string()));
                assert!(args.contains(&"ExtractAudio:-c:a aac".to_string()));
            }
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
            browser_cookies: Some("firefox".into()),
        };

        let args = build_arguments_internal(None, &payload);
        assert!(args.contains(&"--cookies-from-browser".to_string()));
        let cookie_pos = args.iter().position(|a| a == "--cookies-from-browser").unwrap();
        assert_eq!(args[cookie_pos + 1], "firefox");
        assert!(args.contains(&"--compat-options".to_string()));
        let compat_pos = args.iter().position(|a| a == "--compat-options").unwrap();
        assert_eq!(args[compat_pos + 1], "no-keep-subs");

        // When empty or None, cookie flags should NOT be added
        payload.browser_cookies = Some("   ".into());
        let args_empty = build_arguments_internal(None, &payload);
        assert!(!args_empty.contains(&"--cookies-from-browser".to_string()));

        payload.browser_cookies = None;
        let args_none = build_arguments_internal(None, &payload);
        assert!(!args_none.contains(&"--cookies-from-browser".to_string()));
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


