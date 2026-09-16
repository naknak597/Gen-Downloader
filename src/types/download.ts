export type TaskStatus =
  | "Downloading"
  | "Processing/GPU"
  | "Completed"
  | "Error"
  | "Cancelled";

export type FormatType = "video" | "audio";

export type FilterTab = "all" | "active" | "completed";

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
}

export interface MediaMetadata {
  title: string;
  thumbnail?: string;
  duration?: number;
  uploader?: string;
  webpageUrl: string;
}

