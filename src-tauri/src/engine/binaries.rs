use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tauri::{AppHandle, Manager};
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Strips Windows extended-length and UNC path prefixes (e.g. `\\?\` and `\\?\UNC\`)
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

/// Discovers the FFmpeg executable from known bundled paths, resource directories, CWD, or system PATH
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

/// Discovers yt-dlp binary across execution paths, ancestors, resource dir, and CWD
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path_buf() {
        let temp = std::env::temp_dir();
        let normalized = normalize_path_buf(&temp);
        assert!(!normalized.starts_with(r"\\?\"));

        let non_existent = Path::new("non_existent_folder_xyz_123");
        assert_eq!(normalize_path_buf(non_existent), "non_existent_folder_xyz_123");
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
}
