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
    #[serde(default, alias = "downloadSubtitle")]
    pub download_subtitle: bool,
    #[serde(default, alias = "subtitleLang")]
    pub subtitle_lang: Option<String>,
    #[serde(default, alias = "downloadThumbnail")]
    pub download_thumbnail: bool,
    #[serde(default, alias = "downloadMetadata")]
    pub download_metadata: bool,
    #[serde(default, alias = "browserCookies")]
    pub browser_cookies: Option<String>,
}

impl DownloadPayload {
    /// Normalizes and cleans format_type string (trimming whitespace, dots, and converting to lowercase)
    pub fn clean_format(&self) -> String {
        self.format_type.trim().trim_start_matches('.').to_ascii_lowercase()
    }

    /// Returns true if the requested format is an audio container or audio extraction
    pub fn is_audio(&self) -> bool {
        matches!(
            self.clean_format().as_str(),
            "mp3" | "m4a" | "wav" | "flac" | "aac" | "opus" | "audio"
        )
    }

    /// Resolves canonical audio container extension
    pub fn resolved_audio_ext(&self) -> &str {
        match self.clean_format().as_str() {
            "m4a" => "m4a",
            "wav" => "wav",
            "flac" => "flac",
            "aac" => "aac",
            "opus" => "opus",
            _ => "mp3",
        }
    }

    /// Resolves canonical video container extension
    pub fn resolved_video_ext(&self) -> &str {
        match self.clean_format().as_str() {
            "mkv" => "mkv",
            "webm" => "webm",
            "mov" => "mov",
            "avi" => "avi",
            _ => "mp4",
        }
    }
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
    #[serde(default)]
    pub available_subtitles: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
}

/// Core binary versions for yt-dlp and FFmpeg
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CoreVersions {
    pub ytdlp_version: String,
    pub ffmpeg_version: String,
}
