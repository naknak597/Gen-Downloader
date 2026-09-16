use std::path::Path;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_shell::process::CommandEvent;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::engine::{
    build_arguments, clean_filename, clean_filepath, extract_destination_filename,
    extract_destination_filepath, parse_playlist_progress, resolve_safe_save_path,
    scan_playlist_directory, spawn_ytdlp_process,
};
use crate::models::{DownloadPayload, MediaMetadata, PlaylistItem, ProgressPayload};
use crate::state::DownloadManager;

#[tauri::command]
pub async fn start_download(
    app: AppHandle,
    state: State<'_, DownloadManager>,
    payload: DownloadPayload,
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
    let (target_dir, _) = resolve_safe_save_path(&app, payload.save_path.as_deref());
    if let Err(e) = std::fs::create_dir_all(&target_dir) {
        eprintln!(
            "[download] Warning: unable to pre-create destination directory {}: {e}",
            target_dir.display()
        );
    }

    let args = build_arguments(&app, &payload);

    // Spawn yt-dlp process using prioritized sidecar resolution & multi-tier fallback
    let (mut rx, child) = spawn_ytdlp_process(&app, args)?;
    let pid = child.pid();
    println!("[yt-dlp] Task {} spawned with PID {}", task_id, pid);

    // Store child handle in managed state for cancellation
    state.insert_task(task_id.clone(), child)?;

    let app_handle = app.clone();
    let current_task_id = task_id.clone();
    let download_manager = state.inner().clone();
    let is_playlist_download = payload.is_playlist;

    // Spawn background task to process stdout/stderr streams concurrently without pipe deadlock
    tauri::async_runtime::spawn(async move {
        let mut current_filename = String::new();
        let mut final_detected_filename = String::new();
        let mut detected_filepath = String::new();
        let mut last_error_msg = String::new();
        let mut full_error_log = String::new();
        let mut exit_code: Option<i32> = None;
        let mut current_playlist_index: Option<u32> = None;
        let mut current_playlist_total: Option<u32> = None;

        while let Some(event) = rx.recv().await {
            // Early break if this task was marked as cancelled
            if download_manager.is_cancelled(&current_task_id) {
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

                        // Check for destination filepath
                        if let Some(raw_path) = extract_destination_filepath(line) {
                            let clean_p = clean_filepath(&raw_path);
                            if !clean_p.ends_with(".part") && !clean_p.ends_with(".temp") {
                                detected_filepath = clean_p;
                            }
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

                                    let clean_p = clean_filepath(&filename);
                                    if !clean_p.ends_with(".part")
                                        && !clean_p.ends_with(".temp")
                                        && detected_filepath.is_empty()
                                    {
                                        detected_filepath = clean_p;
                                    }
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
                                    error_log: None,
                                    file_path: None,
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
                                error_log: None,
                                file_path: None,
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
                    full_error_log.push_str(&err_text);
                    for line in err_text.lines() {
                        let line = line.trim();
                        if line.is_empty() {
                            continue;
                        }

                        if let Some((idx, total)) = parse_playlist_progress(line) {
                            current_playlist_index = Some(idx);
                            current_playlist_total = Some(total);
                        }

                        if let Some(raw_path) = extract_destination_filepath(line) {
                            let clean_p = clean_filepath(&raw_path);
                            if !clean_p.ends_with(".part") && !clean_p.ends_with(".temp") {
                                detected_filepath = clean_p;
                            }
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
                    full_error_log.push_str(&format!("Process error: {err}\n"));
                    last_error_msg = err;
                }
                CommandEvent::Terminated(payload) => {
                    exit_code = payload.code;
                    break;
                }
                _ => {}
            }
        }

        // 1. Remove from active tasks map to prevent resource leaks
        let _ = download_manager.remove_task(&current_task_id);

        // 2. Check if this task was marked as cancelled
        let was_cancelled = download_manager.clear_cancelled(&current_task_id);

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
            // Resolve the definitive output file path for Open File & Show in Folder:
            // For playlists, always provide the clean directory path containing the downloaded tracks.
            // For single files, provide the specific media file path.
            let final_file_path: Option<String> = if is_playlist_download {
                let playlist_dir: Option<std::path::PathBuf> = {
                    if !detected_filepath.is_empty() {
                        let p = Path::new(&detected_filepath);
                        let full_p = if p.is_absolute() {
                            p.to_path_buf()
                        } else {
                            target_dir.join(p)
                        };

                        if let Some(parent) = full_p.parent() {
                            if parent.exists() && parent.is_dir() && parent != target_dir {
                                Some(parent.to_path_buf())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                };

                let resolved = playlist_dir
                    .or_else(|| {
                        if let Ok(entries) = std::fs::read_dir(&target_dir) {
                            let mut subdirs: Vec<std::path::PathBuf> = entries
                                .flatten()
                                .map(|e| e.path())
                                .filter(|p| p.is_dir())
                                .collect();
                            subdirs.sort_by_key(|p| {
                                p.metadata()
                                    .and_then(|m| m.modified())
                                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                            });
                            subdirs.pop()
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| target_dir.clone());

                Some(resolved.to_string_lossy().replace('/', "\\"))
            } else {
                if !detected_filepath.is_empty() {
                    let p = Path::new(&detected_filepath);
                    if p.is_absolute() && p.exists() {
                        Some(detected_filepath.replace('/', "\\"))
                    } else {
                        let candidate = target_dir.join(&detected_filepath);
                        if candidate.exists() {
                            Some(candidate.to_string_lossy().replace('/', "\\"))
                        } else {
                            let candidate2 = target_dir.join(&final_display_name);
                            if candidate2.exists() {
                                Some(candidate2.to_string_lossy().replace('/', "\\"))
                            } else if p.is_absolute() {
                                Some(detected_filepath.replace('/', "\\"))
                            } else {
                                Some(candidate.to_string_lossy().replace('/', "\\"))
                            }
                        }
                    }
                } else {
                    let candidate = target_dir.join(&final_display_name);
                    if candidate.exists() {
                        Some(candidate.to_string_lossy().replace('/', "\\"))
                    } else {
                        None
                    }
                }
            };

            println!(
                "[yt-dlp] Task {} completed successfully: {} (path: {:?})",
                current_task_id, final_display_name, final_file_path
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
                error_log: if !full_error_log.trim().is_empty() {
                    Some(full_error_log.trim().to_string())
                } else {
                    None
                },
                file_path: final_file_path,
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
            let full_log = if !full_error_log.trim().is_empty() {
                full_error_log.trim().to_string()
            } else {
                error_desc.clone()
            };
            let progress = ProgressPayload {
                task_id: current_task_id.clone(),
                status: "Error".into(),
                percent: 0.0,
                speed: "0 KiB/s".into(),
                eta: "00:00".into(),
                filename: error_desc,
                playlist_index: current_playlist_index,
                playlist_total: current_playlist_total,
                error_log: Some(full_log),
                file_path: None,
            };
            let _ = app_handle.emit("download-progress", &progress);
        }
    });

    Ok(task_id)
}

#[tauri::command]
pub async fn cancel_download(
    task_id: String,
    state: State<'_, DownloadManager>,
    app: AppHandle,
) -> Result<(), String> {
    println!("[download] cancel_download requested for task: {task_id}");

    // 1. Mark as cancelled immediately to prevent race conditions with background thread
    state.mark_cancelled(&task_id)?;

    // 2. Remove and retrieve child process handle from active tasks map
    let maybe_child = state.remove_task(&task_id)?;

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
        error_log: None,
        file_path: None,
    };
    let _ = app.emit("download-progress", &progress);

    Ok(())
}

/// Opens the downloaded media file using the system's default application
#[tauri::command]
pub async fn open_media_file(file_path: String) -> Result<(), String> {
    let normalized = file_path.replace('/', "\\");
    let path = Path::new(&normalized);
    if !path.exists() {
        return Err(format!("File does not exist: {normalized}"));
    }

    #[cfg(windows)]
    {
        let mut cmd = std::process::Command::new("cmd");
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        cmd.args(["/C", "start", "", &normalized]);
        cmd.spawn()
            .map_err(|e| format!("Failed to open file with default player: {e}"))?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&normalized)
            .spawn()
            .map_err(|e| format!("Failed to open file: {e}"))?;
    }

    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(&normalized)
            .spawn()
            .map_err(|e| format!("Failed to open file: {e}"))?;
    }

    Ok(())
}

/// Opens Windows Explorer with the specific file selected/highlighted
#[tauri::command]
pub async fn show_in_folder(file_path: String) -> Result<(), String> {
    let normalized = file_path.replace('/', "\\");
    let path = Path::new(&normalized);

    #[cfg(windows)]
    {
        let mut cmd = std::process::Command::new("explorer");
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW

        if path.is_file() {
            cmd.args(["/select,", &normalized]);
        } else if path.is_dir() {
            cmd.arg(&normalized);
        } else if let Some(parent) = path.parent() {
            if parent.exists() {
                cmd.arg(parent.to_string_lossy().to_string());
            } else {
                cmd.args(["/select,", &normalized]);
            }
        } else {
            cmd.args(["/select,", &normalized]);
        }

        cmd.spawn()
            .map_err(|e| format!("Failed to launch Windows Explorer: {e}"))?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .args(["-R", &normalized])
            .spawn()
            .map_err(|e| format!("Failed to show in folder: {e}"))?;
    }

    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        let target_dir = if path.is_dir() {
            path
        } else {
            path.parent().unwrap_or(path)
        };
        std::process::Command::new("xdg-open")
            .arg(target_dir)
            .spawn()
            .map_err(|e| format!("Failed to show in folder: {e}"))?;
    }

    Ok(())
}

/// Scans the target playlist directory for media files and returns them naturally sorted
#[tauri::command]
pub async fn get_playlist_items(dir_path: String) -> Result<Vec<PlaylistItem>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        scan_playlist_directory(&dir_path)
    })
    .await
    .map_err(|e| format!("Internal error joining scan task: {e}"))?
}

/// Fetches media metadata (title, thumbnail, duration, uploader) for video preview
#[tauri::command]
pub async fn get_media_info(app: tauri::AppHandle, url: String) -> Result<MediaMetadata, String> {
    crate::engine::fetch_media_metadata(&app, &url).await
}

