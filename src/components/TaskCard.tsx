import React, { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Video,
  Music,
  Layers,
  RefreshCw,
  Sparkles,
  CheckCircle2,
  AlertCircle,
  XCircle,
  Trash2,
  Gauge,
  Clock,
  FileText,
  Play,
  FolderOpen,
  ListMusic,
} from "lucide-react";
import { DownloadTask } from "../types/download";
import { ErrorLogModal } from "./ErrorLogModal";
import { PlaylistModal } from "./PlaylistModal";

interface TaskCardProps {
  task: DownloadTask;
  onCancel: (taskId: string) => void;
  onRemove: (taskId: string) => void;
}

function getPlaylistDirectory(filePath?: string): string {
  if (!filePath) return "";
  const normalized = filePath.replace(/\\/g, "/");
  // If the path ends with a media file extension, extract the parent folder path
  if (/\.(mp4|mp3|m4a|webm|mkv|wav)$/i.test(normalized)) {
    const lastSlash = normalized.lastIndexOf("/");
    return lastSlash > 0 ? normalized.substring(0, lastSlash) : normalized;
  }
  return normalized;
}

export const TaskCard: React.FC<TaskCardProps> = ({
  task,
  onCancel,
  onRemove,
}) => {
  const [showErrorModal, setShowErrorModal] = useState(false);
  const [showPlaylistModal, setShowPlaylistModal] = useState(false);
  const isRunning =
    task.status === "Downloading" || task.status === "Processing/GPU";
  const isPlaylist = Boolean(
    task.isPlaylist || (task.playlistTotal !== undefined && task.playlistTotal > 0)
  );

  const handleOpenFile = async () => {
    if (!task.filePath) return;
    try {
      await invoke("open_media_file", {
        filePath: task.filePath,
        file_path: task.filePath,
      });
    } catch (err) {
      console.error("Failed to open media file:", err);
    }
  };

  const handleShowInFolder = async () => {
    if (!task.filePath) return;
    try {
      await invoke("show_in_folder", {
        filePath: task.filePath,
        file_path: task.filePath,
      });
    } catch (err) {
      console.error("Failed to show in folder:", err);
    }
  };

  return (
    <>
      <div className="group relative bg-neutral-950/70 border border-neutral-800/80 hover:border-neutral-700/80 rounded-xl p-4 transition-all shadow-sm flex flex-col gap-3">
        {/* Top Row: Icon, Title, Status & Actions */}
        <div className="flex items-center justify-between gap-4">
          <div className="flex items-center gap-3 min-w-0">
            <div className="flex items-center justify-center w-8 h-8 rounded-lg bg-neutral-900 border border-neutral-800 text-neutral-300 shrink-0">
              {["mp3", "m4a", "wav", "flac", "aac", "opus", "audio"].includes(
                task.formatType.toLowerCase()
              ) ? (
                <Music className="w-4 h-4 text-purple-400" />
              ) : (
                <Video className="w-4 h-4 text-cyan-400" />
              )}
            </div>
            <div className="min-w-0">
              <div className="flex items-center gap-2 flex-wrap">
                <p
                  className="text-sm font-medium text-neutral-200 truncate"
                  title={task.filename}
                >
                  {task.filename}
                </p>
                {task.playlistTotal !== undefined && task.playlistTotal > 0 && (
                  <span className="inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-[11px] font-semibold bg-gradient-to-r from-indigo-500/20 to-purple-500/20 text-indigo-300 border border-indigo-500/40 shadow-sm shrink-0">
                    <Layers className="w-3 h-3 text-indigo-400" />
                    Video {task.playlistIndex ?? 1} of {task.playlistTotal}
                  </span>
                )}
              </div>
              <p
                className="text-xs text-neutral-500 truncate max-w-sm"
                title={task.url}
              >
                {task.url}
              </p>
            </div>
          </div>

          {/* Status Pill & Action Buttons */}
          <div className="flex items-center gap-2 shrink-0">
            {/* Visual Status Pill */}
            {task.status === "Downloading" && (
              <span className="flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium bg-cyan-500/10 text-cyan-400 border border-cyan-500/20">
                <RefreshCw className="w-3 h-3 animate-spin" />
                {task.playlistTotal !== undefined && task.playlistTotal > 0
                  ? `Downloading (${task.playlistIndex ?? 1}/${task.playlistTotal})`
                  : "Downloading"}
              </span>
            )}
            {task.status === "Processing/GPU" && (
              <span className="flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium bg-purple-500/10 text-purple-400 border border-purple-500/20">
                <Sparkles className="w-3 h-3 animate-pulse" />
                {task.playlistTotal !== undefined && task.playlistTotal > 0
                  ? `Muxing (${task.playlistIndex ?? 1}/${task.playlistTotal})`
                  : "Muxing with GPU"}
              </span>
            )}
            {task.status === "Completed" && (
              <span className="flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium bg-emerald-500/10 text-emerald-400 border border-emerald-500/20">
                <CheckCircle2 className="w-3 h-3" />
                {task.playlistTotal !== undefined && task.playlistTotal > 0
                  ? `Completed (${task.playlistTotal}/${task.playlistTotal})`
                  : "Completed"}
              </span>
            )}
            {task.status === "Error" && (
              <span className="flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium bg-rose-500/10 text-rose-400 border border-rose-500/20">
                <AlertCircle className="w-3 h-3" />
                Failed
              </span>
            )}
            {task.status === "Cancelled" && (
              <span className="flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium bg-neutral-800 text-neutral-400 border border-neutral-700">
                <XCircle className="w-3 h-3" />
                Cancelled
              </span>
            )}

            {/* Completed Actions: View Playlist / Open File & Show in Folder */}
            {task.status === "Completed" && task.filePath && (
              <div className="flex items-center gap-1.5">
                {isPlaylist ? (
                  <button
                    type="button"
                    onClick={() => setShowPlaylistModal(true)}
                    className="flex items-center gap-1.5 px-3 py-1 rounded-full text-xs font-medium bg-gradient-to-r from-indigo-500/20 to-purple-500/20 hover:from-indigo-500/30 hover:to-purple-500/30 text-indigo-300 border border-indigo-500/40 transition-all cursor-pointer shadow-sm"
                    title={`View Playlist Tracks (${task.playlistTotal ?? "All"} tracks)`}
                  >
                    <ListMusic className="w-3.5 h-3.5 text-indigo-400" />
                    <span>View Playlist</span>
                  </button>
                ) : (
                  <button
                    type="button"
                    onClick={handleOpenFile}
                    className="flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium bg-emerald-500/15 hover:bg-emerald-500/25 text-emerald-400 border border-emerald-500/30 transition-colors cursor-pointer"
                    title={`Open media file:\n${task.filePath}`}
                  >
                    <Play className="w-3 h-3 fill-emerald-400/20" />
                    <span>Play</span>
                  </button>
                )}

                <button
                  type="button"
                  onClick={handleShowInFolder}
                  className="flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium bg-neutral-800 hover:bg-neutral-700 text-neutral-300 hover:text-neutral-100 border border-neutral-700/60 transition-colors cursor-pointer"
                  title={`Show in folder:\n${task.filePath}`}
                >
                  <FolderOpen className="w-3 h-3 text-neutral-400" />
                  <span>Folder</span>
                </button>
              </div>
            )}

            {/* View Error Log Button */}
            {(task.status === "Error" || Boolean(task.errorLog)) && (
              <button
                type="button"
                onClick={() => setShowErrorModal(true)}
                className="flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium bg-rose-500/10 hover:bg-rose-500/20 text-rose-300 border border-rose-500/30 transition-colors cursor-pointer"
                title="View Error Logs"
              >
                <FileText className="w-3 h-3" />
                <span>View Log</span>
              </button>
            )}

            {/* Cancel / Dismiss Buttons */}
            {isRunning ? (
              <button
                type="button"
                onClick={() => onCancel(task.taskId)}
                className="p-1.5 text-neutral-400 hover:text-rose-400 hover:bg-rose-500/10 rounded-lg transition-colors cursor-pointer"
                title="Cancel Download"
              >
                <XCircle className="w-4 h-4" />
              </button>
            ) : (
              <button
                type="button"
                onClick={() => onRemove(task.taskId)}
                className="p-1.5 text-neutral-500 hover:text-neutral-300 hover:bg-neutral-800 rounded-lg transition-colors cursor-pointer"
                title="Dismiss Task"
              >
                <Trash2 className="w-4 h-4" />
              </button>
            )}
          </div>
        </div>

        {/* Middle Row: Glowing Progress Bar */}
        <div className="w-full bg-neutral-900 rounded-full h-2 overflow-hidden border border-neutral-800/80">
          <div
            className={`h-full transition-all duration-300 ease-out ${
              task.status === "Completed"
                ? "bg-emerald-500"
                : task.status === "Error"
                ? "bg-rose-500"
                : task.status === "Cancelled"
                ? "bg-neutral-600"
                : "bg-gradient-to-r from-cyan-500 via-indigo-500 to-purple-500 shadow-sm shadow-indigo-500/50"
            }`}
            style={{ width: `${Math.min(Math.max(task.percent, 0), 100)}%` }}
          />
        </div>

        {/* Bottom Row: Metrics & Details */}
        <div className="flex items-center justify-between text-xs text-neutral-400 font-mono">
          <div className="flex items-center gap-4">
            <span className="flex items-center gap-1 font-semibold text-neutral-200">
              {task.percent.toFixed(1)}%
            </span>
            {task.status === "Downloading" && (
              <>
                <span className="flex items-center gap-1">
                  <Gauge className="w-3 h-3 text-neutral-500" />
                  {task.speed}
                </span>
                <span className="flex items-center gap-1">
                  <Clock className="w-3 h-3 text-neutral-500" />
                  ETA: {task.eta}
                </span>
              </>
            )}
          </div>

          <div className="flex items-center gap-2 text-neutral-500 font-sans text-[11px]">
            {task.playlistTotal !== undefined && task.playlistTotal > 0 && (
              <>
                <span className="text-indigo-400 font-medium font-mono">
                  Item {task.playlistIndex ?? 1}/{task.playlistTotal}
                </span>
                <span>•</span>
              </>
            )}
            <span>{task.formatType.toUpperCase()}</span>
            <span>•</span>
            <span>{task.quality.toUpperCase()}</span>
            <span>•</span>
            <span>{task.createdAt}</span>
          </div>
        </div>
      </div>

      {/* Error Log Modal */}
      {showErrorModal && (
        <ErrorLogModal
          isOpen={showErrorModal}
          onClose={() => setShowErrorModal(false)}
          title={task.filename}
          url={task.url}
          errorLog={task.errorLog}
        />
      )}

      {/* Playlist Tracks Modal */}
      {showPlaylistModal && task.filePath && (
        <PlaylistModal
          isOpen={showPlaylistModal}
          onClose={() => setShowPlaylistModal(false)}
          playlistPath={getPlaylistDirectory(task.filePath)}
          playlistTitle={task.filename}
        />
      )}
    </>
  );
};
