export type TaskStatus =
  | "Downloading"
  | "Processing/GPU"
  | "Completed"
  | "Error"
  | "Cancelled";

export type VideoFormat = "mp4" | "mkv" | "webm" | "mov" | "avi";
export type AudioFormat = "mp3" | "m4a" | "wav" | "flac" | "aac" | "opus";
export type MediaFormat = VideoFormat | AudioFormat;
export type FormatType = "video" | "audio" | MediaFormat;

export type TaskFilter = "all" | "active" | "completed" | "failed";
export type FilterTab = TaskFilter;

export interface DownloadProgressEvent {
  task_id: string;
  status: TaskStatus;
  percent: number;
  speed: string;
  eta: string;
  filename: string;
  playlist_index?: number | null;
  playlist_total?: number | null;
  error_log?: string | null;
  file_path?: string | null;
}

export interface PlaylistItem {
  name: string;
  filePath: string;
  fileSize: number;
  extension: string;
}

export interface DownloadTask {
  taskId: string;
  url: string;
  formatType: FormatType;
  quality: string;
  filename: string;
  status: TaskStatus;
  percent: number;
  speed: string;
  eta: string;
  createdAt: string;
  isPlaylist?: boolean;
  playlistIndex?: number;
  playlistTotal?: number;
  errorLog?: string;
  filePath?: string;
}

export interface FormOptions {
  url: string;
  formatType: FormatType;
  quality: string;
  isPlaylist: boolean;
  useGpu: boolean;
  savePath: string;
  downloadSubtitle?: boolean;
  subtitleLang?: string;
  downloadThumbnail?: boolean;
  downloadMetadata?: boolean;
  browserCookies?: string;
}

export interface DownloadPayload {
  task_id?: string | null;
  url: string;
  format_type: FormatType;
  quality: string;
  is_playlist: boolean;
  use_gpu: boolean;
  save_path?: string | null;
  download_subtitle?: boolean;
  subtitle_lang?: string | null;
  download_thumbnail?: boolean;
  download_metadata?: boolean;
  downloadMetadata?: boolean;
  browser_cookies?: string | null;
  browserCookies?: string;
}

export interface MediaMetadata {
  title: string;
  thumbnail?: string;
  duration?: number;
  uploader?: string;
  webpageUrl: string;
  availableSubtitles?: string[];
  description?: string;
  tags?: string[];
}

export interface CoreVersions {
  ytdlpVersion: string;
  ffmpegVersion: string;
}


