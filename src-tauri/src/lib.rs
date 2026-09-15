use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

// ---------------------------------------------------------------------------
// 1. Data Structures
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct DownloadRequest {
    #[serde(default)]
    pub task_id: Option<String>,
    pub url: String,
    pub format_type: String, // "video" | "audio"
    pub quality: String,     // "best" | "4k" | "1080p" | "720p"
    pub is_playlist: bool,
    pub use_gpu: bool,
    pub save_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProgressPayload {
    pub task_id: String,
    pub status: String, // "Downloading", "Processing/GPU", "Completed", "Error", "Cancelled"
    pub percent: f32,
    pub speed: String,
    pub eta: String,
    pub filename: String,
    pub playlist_index: Option<u32>,
    pub playlist_total: Option<u32>,
}

// Thread-safe state for storing active processes to allow cancellation
#[derive(Default, Clone)]
pub struct DownloadManager {
    pub active_tasks: Arc<Mutex<HashMap<String, CommandChild>>>,
    pub cancelled_tasks: Arc<Mutex<HashSet<String>>>,
}

// ---------------------------------------------------------------------------
// 2. Helpers: Locate Bundled Binaries (FFmpeg & yt-dlp)
// ---------------------------------------------------------------------------

fn normalize_path_buf(path: &Path) -> String {
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

fn resolve_ffmpeg_path(app: Option<&AppHandle>) -> Option<String> {
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
            // Direct parent
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
fn is_nvenc_functional(ffmpeg_path: &str) -> bool {
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

fn find_ytdlp_executable(app: Option<&AppHandle>) -> Option<PathBuf> {
    const CANDIDATES: &[&str] = &[
        "yt-dlp.exe",
        "yt-dlp-x86_64-pc-windows-msvc.exe",
        "binaries/yt-dlp.exe",
        "binaries/yt-dlp-x86_64-pc-windows-msvc.exe",
        "src-tauri/binaries/yt-dlp.exe",
        "src-tauri/binaries/yt-dlp-x86_64-pc-windows-msvc.exe",
    ];

    // 1. Check next to running executable and its ancestors (production install or target/debug)
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
fn spawn_ytdlp_process(
    app: &AppHandle,
    args: Vec<String>,
) -> Result<(tauri::async_runtime::Receiver<CommandEvent>, CommandChild), String> {
    // Diagnostic log for current exe and expected sidecar paths
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
// 3. Dynamic Arguments Construction
// ---------------------------------------------------------------------------

/// Strips intermediate stream identifiers and temporary extensions
/// e.g. "Video Title.f298.mp4" -> "Video Title.mp4"
///      "Video Title.f140.m4a" -> "Video Title.mp4"
///      "Video Title.temp.mp4" -> "Video Title.mp4"
///      "Video Title.mp4.part" -> "Video Title.mp4"
fn clean_filename(raw: &str) -> String {
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

/// Extracts clean target filename from yt-dlp log lines (Merger, Destination, etc.)
fn extract_destination_filename(line: &str) -> Option<String> {
    if line.contains("[Merger] Merging formats into \"") {
        let prefix = "[Merger] Merging formats into \"";
        if let Some(start) = line.find(prefix) {
            let rest = &line[start + prefix.len()..];
            if let Some(end) = rest.rfind('"') {
                let path_str = &rest[..end];
                let fname = Path::new(path_str).file_name()?.to_string_lossy().to_string();
                return Some(clean_filename(&fname));
            }
        }
    } else if line.contains("Destination: ") {
        if let Some(start) = line.find("Destination: ") {
            let rest = line[start + "Destination: ".len()..].trim().trim_matches('"');
            let fname = Path::new(rest).file_name()?.to_string_lossy().to_string();
            let cleaned = clean_filename(&fname);
            if !cleaned.is_empty() {
                return Some(cleaned);
            }
        }
    }
    None
}

fn get_playlist_regex() -> &'static regex::Regex {
    static PLAYLIST_REGEX: OnceLock<regex::Regex> = OnceLock::new();
    PLAYLIST_REGEX.get_or_init(|| {
        regex::Regex::new(r"Downloading (?:video|item) (\d+) of (\d+)").unwrap()
    })
}

/// Parses playlist progress from yt-dlp log lines.
/// Matches lines like:
///   "[download] Downloading video 1 of 15"
///   "[download] Downloading item 3 of 10"
fn parse_playlist_progress(line: &str) -> Option<(u32, u32)> {
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

/// Resolves the base save directory, ensuring it exists on disk,
/// and returns a normalized cross-platform path string (using forward slashes).
fn resolve_safe_save_path(app: &AppHandle, save_path: Option<&str>) -> (PathBuf, String) {
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
fn build_output_template(clean_save_path: &str, is_playlist: bool) -> String {
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

fn build_ytdlp_args(app: &AppHandle, request: &DownloadRequest) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "--newline".into(),
        "--no-colors".into(),
        "--progress".into(),
        // Structured progress template for easy line parsing
        "--progress-template".into(),
        "download-progress:%(progress._percent_str)s|%(progress._speed_str)s|%(progress._eta_str)s|%(progress.filename)s".into(),
        // Strictly ensure intermediate stream files are deleted after muxing
        "--no-keep-video".into(),
    ];

    // Playlist handling
    if request.is_playlist {
        args.push("--yes-playlist".into());
    } else {
        args.push("--no-playlist".into());
    }

    // Pass resolved absolute FFmpeg location
    let ffmpeg_path_opt = resolve_ffmpeg_path(Some(app));
    if let Some(ref ffmpeg_path) = ffmpeg_path_opt {
        args.push("--ffmpeg-location".into());
        args.push(ffmpeg_path.clone());
    }

    // Format & Transcoding Flags
    if request.format_type.to_lowercase() == "audio" {
        // High quality MP3 extraction cleanly removing source container
        args.push("-x".into());
        args.push("--audio-format".into());
        args.push("mp3".into());
        args.push("--audio-quality".into());
        args.push("0".into());
        // Safe non-interactive FFmpeg parameters preventing stdin hangs
        args.push("--postprocessor-args".into());
        args.push("ffmpeg:-nostdin -y".into());
    } else {
        // Video (MP4) format selector: Prioritize native compatible H.264 (avc1) + AAC (mp4a)
        // streams, gracefully falling back to highest available resolution
        let format_selector = match request.quality.to_lowercase().as_str() {
            "4k" => "bestvideo[height<=2160][vcodec^=avc1]+bestaudio[acodec^=mp4a]/bestvideo[height<=2160]+bestaudio/best[height<=2160]",
            "1080p" => "bestvideo[height<=1080][vcodec^=avc1]+bestaudio[acodec^=mp4a]/bestvideo[height<=1080]+bestaudio/best[height<=1080]",
            "720p" => "bestvideo[height<=720][vcodec^=avc1]+bestaudio[acodec^=mp4a]/bestvideo[height<=720]+bestaudio/best[height<=720]",
            _ => "bestvideo[vcodec^=avc1]+bestaudio[acodec^=mp4a]/bestvideo+bestaudio/best",
        };
        args.push("-f".into());
        args.push(format_selector.into());

        // Fast direct container remux into mp4 (stream copy without slow re-encoding)
        args.push("--merge-output-format".into());
        args.push("mp4".into());

        // Safe non-interactive FFmpeg postprocessor parameters preventing stdin hangs & overwrite prompts
        args.push("--postprocessor-args".into());
        args.push("ffmpeg:-nostdin -y".into());

        // If GPU acceleration is requested for non-MP4 formats (WebM/VP9), pass NVENC to VideoConvertor only
        let can_use_nvenc = request.use_gpu
            && ffmpeg_path_opt
                .as_ref()
                .map(|p| is_nvenc_functional(p))
                .unwrap_or(false);

        if can_use_nvenc {
            args.push("--postprocessor-args".into());
            args.push("VideoConvertor:-nostdin -y -c:v h264_nvenc -preset p4 -cq 23 -c:a aac -b:a 192k".into());
        }
    }

    // Save path & Output filename template:
    // When is_playlist is true: <SAVE_PATH>/%(playlist_title)s/%(playlist_index)02d - %(title)s.%(ext)s
    // When is_playlist is false: <SAVE_PATH>/%(title)s.%(ext)s
    // The playlist subfolder is automatically created inside the destination directory by yt-dlp,
    // and resolve_safe_save_path ensures the destination directory exists on disk.
    let (_, clean_save_path) = resolve_safe_save_path(app, request.save_path.as_deref());
    let output_template = build_output_template(&clean_save_path, request.is_playlist);

    args.push("-o".into());
    args.push(output_template);

    // Target URL
    args.push(request.url.clone());

    args
}

// ---------------------------------------------------------------------------
// 4. Tauri Commands
// ---------------------------------------------------------------------------

#[tauri::command]
async fn start_download(
    app: AppHandle,
    state: State<'_, DownloadManager>,
    payload: DownloadRequest,
) -> Result<String, String> {
    let task_id = payload.task_id.clone().unwrap_or_else(|| {
        format!(
            "task_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        )
    });

    // Ensure user-specified destination folder exists on disk
    if let Some(ref path_str) = payload.save_path {
        let trimmed = path_str.trim();
        if !trimmed.is_empty() {
            let path_buf = PathBuf::from(trimmed);
            if let Err(e) = std::fs::create_dir_all(&path_buf) {
                eprintln!(
                    "[download] Warning: unable to pre-create destination directory {}: {e}",
                    path_buf.display()
                );
            }
        }
    }

    let args = build_ytdlp_args(&app, &payload);

    // Spawn yt-dlp process using robust sidecar resolution & multi-tier fallback
    let (mut rx, child) = spawn_ytdlp_process(&app, args)?;
    let pid = child.pid();
    println!("[yt-dlp] Task {} spawned with PID {}", task_id, pid);

    // Store child handle for cancellation
    {
        let mut tasks = state
            .active_tasks
            .lock()
            .map_err(|e| format!("Mutex poisoned: {e}"))?;
        tasks.insert(task_id.clone(), child);
    }

    let app_handle = app.clone();
    let current_task_id = task_id.clone();
    let download_manager = state.inner().clone();

    // Spawn background task to process stdout/stderr streams concurrently without pipe deadlock
    tauri::async_runtime::spawn(async move {
        let mut current_filename = String::new();
        let mut final_detected_filename = String::new();
        let mut last_error_msg = String::new();
        let mut exit_code: Option<i32> = None;
        let mut current_playlist_index: Option<u32> = None;
        let mut current_playlist_total: Option<u32> = None;

        while let Some(event) = rx.recv().await {
            // Early break if this task was marked as cancelled
            if download_manager
                .cancelled_tasks
                .lock()
                .map(|set| set.contains(&current_task_id))
                .unwrap_or(false)
            {
                println!(
                    "[yt-dlp] Task {} cancelled mid-stream; breaking event loop.",
                    current_task_id
                );
                break;
            }

            match event {
                CommandEvent::Stdout(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    for line in text.lines() {
                        let line = line.trim();
                        if line.is_empty() {
                            continue;
                        }

                        // Check if yt-dlp reported playlist progress
                        if let Some((idx, total)) = parse_playlist_progress(line) {
                            current_playlist_index = Some(idx);
                            current_playlist_total = Some(total);
                        }

                        // Parse structured progress line
                        if line.starts_with("download-progress:") {
                            let raw = &line["download-progress:".len()..];
                            let parts: Vec<&str> = raw.split('|').collect();
                            if parts.len() >= 4 {
                                let percent_str = parts[0].trim().trim_end_matches('%');
                                let percent = percent_str.parse::<f32>().unwrap_or(0.0);
                                let speed = parts[1].trim().to_string();
                                let eta = parts[2].trim().to_string();
                                let filename = parts[3..].join("|").trim().to_string();

                                if !filename.is_empty() {
                                    current_filename = filename.clone();
                                    let clean = clean_filename(&filename);
                                    final_detected_filename = clean;
                                }

                                let display_name = if !final_detected_filename.is_empty() {
                                    final_detected_filename.clone()
                                } else {
                                    clean_filename(&current_filename)
                                };

                                let progress = ProgressPayload {
                                    task_id: current_task_id.clone(),
                                    status: "Downloading".into(),
                                    percent,
                                    speed,
                                    eta,
                                    filename: display_name,
                                    playlist_index: current_playlist_index,
                                    playlist_total: current_playlist_total,
                                };
                                let _ = app_handle.emit("download-progress", &progress);
                            }
                        } else if line.contains("[Merger]")
                            || line.contains("[ExtractAudio]")
                            || line.contains("[VideoConvertor]")
                            || line.contains("[Fixup")
                        {
                            if let Some(clean) = extract_destination_filename(line) {
                                final_detected_filename = clean;
                            }

                            let display_name = if !final_detected_filename.is_empty() {
                                final_detected_filename.clone()
                            } else {
                                clean_filename(&current_filename)
                            };

                            // Post-processing / muxing phase
                            let progress = ProgressPayload {
                                task_id: current_task_id.clone(),
                                status: "Processing/GPU".into(),
                                percent: 100.0,
                                speed: "N/A".into(),
                                eta: "N/A".into(),
                                filename: display_name,
                                playlist_index: current_playlist_index,
                                playlist_total: current_playlist_total,
                            };
                            let _ = app_handle.emit("download-progress", &progress);
                        } else if line.starts_with("[download] Destination:") {
                            if let Some(clean) = extract_destination_filename(line) {
                                if !clean.contains(".f")
                                    && !clean.ends_with(".temp")
                                    && !clean.ends_with(".part")
                                {
                                    final_detected_filename = clean;
                                }
                            }
                        }
                    }
                }
                CommandEvent::Stderr(bytes) => {
                    // Continually drain stderr buffer so child process never blocks
                    let err_text = String::from_utf8_lossy(&bytes);
                    for line in err_text.lines() {
                        let line = line.trim();
                        if line.is_empty() {
                            continue;
                        }

                        if let Some((idx, total)) = parse_playlist_progress(line) {
                            current_playlist_index = Some(idx);
                            current_playlist_total = Some(total);
                        }

                        if line.starts_with("ERROR:") {
                            eprintln!("[yt-dlp:stderr] {}", line);
                            last_error_msg = line.to_string();
                        } else if line.contains("[Merger]") || line.contains("[ExtractAudio]") {
                            if let Some(clean) = extract_destination_filename(line) {
                                final_detected_filename = clean;
                            }
                        }
                    }
                }
                CommandEvent::Error(err) => {
                    eprintln!("[yt-dlp:error] {}", err);
                    last_error_msg = err;
                }
                CommandEvent::Terminated(payload) => {
                    exit_code = payload.code;
                    break;
                }
                _ => {}
            }
        }

        // 1. Remove from active tasks map to prevent memory leaks
        if let Ok(mut tasks) = download_manager.active_tasks.lock() {
            tasks.remove(&current_task_id);
        }

        // 2. Check if this task was marked as cancelled
        let was_cancelled = download_manager
            .cancelled_tasks
            .lock()
            .map(|mut set| set.remove(&current_task_id))
            .unwrap_or(false);

        if was_cancelled {
            println!(
                "[yt-dlp] Task {} was cancelled; suppressing error/completion emission.",
                current_task_id
            );
            return;
        }

        // Process final completion status
        let is_success = exit_code == Some(0);

        let final_display_name = if !final_detected_filename.is_empty() {
            final_detected_filename
        } else if !current_filename.is_empty() {
            clean_filename(&current_filename)
        } else {
            "Download completed".to_string()
        };

        if is_success {
            println!(
                "[yt-dlp] Task {} completed successfully: {}",
                current_task_id, final_display_name
            );
            let progress = ProgressPayload {
                task_id: current_task_id.clone(),
                status: "Completed".into(),
                percent: 100.0,
                speed: "0 KiB/s".into(),
                eta: "00:00".into(),
                filename: final_display_name,
                playlist_index: current_playlist_index,
                playlist_total: current_playlist_total,
            };
            let _ = app_handle.emit("download-progress", &progress);
        } else {
            let error_desc = if !last_error_msg.is_empty() {
                last_error_msg
            } else {
                format!("Process exited with code {:?}", exit_code.unwrap_or(-1))
            };
            eprintln!(
                "[yt-dlp] Task {} terminated with error: {}",
                current_task_id, error_desc
            );
            let progress = ProgressPayload {
                task_id: current_task_id.clone(),
                status: "Error".into(),
                percent: 0.0,
                speed: "0 KiB/s".into(),
                eta: "00:00".into(),
                filename: error_desc,
                playlist_index: current_playlist_index,
                playlist_total: current_playlist_total,
            };
            let _ = app_handle.emit("download-progress", &progress);
        }
    });

    Ok(task_id)
}

#[tauri::command]
async fn cancel_download(
    task_id: String,
    state: State<'_, DownloadManager>,
    app: AppHandle,
) -> Result<(), String> {
    println!("[download] cancel_download requested for task: {task_id}");

    // 1. Mark as cancelled immediately to prevent race conditions with background thread
    if let Ok(mut set) = state.cancelled_tasks.lock() {
        set.insert(task_id.clone());
    }

    // 2. Remove and retrieve child process handle from active tasks map
    let maybe_child = {
        let mut tasks = state
            .active_tasks
            .lock()
            .map_err(|e| format!("Mutex poisoned: {e}"))?;
        tasks.remove(&task_id)
    };

    if let Some(child) = maybe_child {
        let pid = child.pid();
        println!(
            "[download] Found active process for task {} with PID {}",
            task_id, pid
        );

        // Windows process-tree kill: yt-dlp + ffmpeg + any spawned subprocesses
        #[cfg(windows)]
        {
            let mut kill_cmd = std::process::Command::new("taskkill");
            kill_cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
            kill_cmd.args(["/F", "/T", "/PID", &pid.to_string()]);
            match kill_cmd.output() {
                Ok(output) => {
                    println!(
                        "[download] taskkill /F /T /PID {} completed with status: {}",
                        pid, output.status
                    );
                }
                Err(e) => {
                    eprintln!("[download] Failed to execute taskkill for PID {}: {}", pid, e);
                }
            }
        }

        // Also call child.kill() to ensure handle is closed/cleaned up
        let _ = child.kill();
    } else {
        println!(
            "[download] No active process handle found for task {} (may have already terminated)",
            task_id
        );
    }

    // 3. Emit definitive Cancelled event to notify the frontend
    let progress = ProgressPayload {
        task_id: task_id.clone(),
        status: "Cancelled".into(),
        percent: 0.0,
        speed: "0 KiB/s".into(),
        eta: "00:00".into(),
        filename: "Download cancelled by user".into(),
        playlist_index: None,
        playlist_total: None,
    };
    let _ = app.emit("download-progress", &progress);

    Ok(())
}


// ---------------------------------------------------------------------------
// 5. Entry Point
// ---------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .manage(DownloadManager::default())
        .invoke_handler(tauri::generate_handler![start_download, cancel_download])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
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
    fn test_download_manager_cancellation_tracking() {
        let dm = DownloadManager::default();
        assert!(!dm.cancelled_tasks.lock().unwrap().contains("task_test"));

        // Mark task as cancelled
        dm.cancelled_tasks.lock().unwrap().insert("task_test".to_string());
        assert!(dm.cancelled_tasks.lock().unwrap().contains("task_test"));

        // Clean up task cancellation
        assert!(dm.cancelled_tasks.lock().unwrap().remove("task_test"));
        assert!(!dm.cancelled_tasks.lock().unwrap().contains("task_test"));
    }
}
