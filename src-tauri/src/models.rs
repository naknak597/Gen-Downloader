use serde::{Deserialize, Serialize};

/// Represents an individual track or media item found within a playlist directory
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistItem {
    pub name: String,
    pub file_path: String,
    pub file_size: u64,
    pub extension: String,
}

/// Payload received from frontend to start a download task
#[derive(Debug, Clone, Deserialize)]
pub struct DownloadPayload {
    #[serde(default)]
    pub task_id: Option<String>,
    pub url: String,
    pub format_type: String, // "video" | "audio"
    pub quality: String,     // "best" | "4k" | "1080p" | "720p"
    pub is_playlist: bool,
    pub use_gpu: bool,
    pub save_path: Option<String>,
}

/// Backwards-compatible alias for DownloadPayload
pub type DownloadRequest = DownloadPayload;

/// Payload emitted to frontend via 'download-progress' event channel
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
    pub error_log: Option<String>,
    pub file_path: Option<String>,
}

/// Response payload containing media metadata for video preview
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MediaMetadata {
    pub title: String,
    pub thumbnail: Option<String>,
    pub duration: Option<u64>,
    pub uploader: Option<String>,
    pub webpage_url: String,
}

