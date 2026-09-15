import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  Download,
  Folder,
  Clipboard,
  CheckCircle2,
  AlertCircle,
  XCircle,
  Video,
  Music,
  Zap,
  Cpu,
  Layers,
  Sparkles,
  Clock,
  Gauge,
  Trash2,
  RefreshCw,
  FolderCheck,
} from "lucide-react";

interface DownloadProgressEvent {
  task_id: string;
  status: "Downloading" | "Processing/GPU" | "Completed" | "Error" | "Cancelled";
  percent: number;
  speed: string;
  eta: string;
  filename: string;
  playlist_index?: number | null;
  playlist_total?: number | null;
}

interface TaskItem {
  taskId: string;
  url: string;
  formatType: "video" | "audio";
  quality: string;
  filename: string;
  status: "Downloading" | "Processing/GPU" | "Completed" | "Error" | "Cancelled";
  percent: number;
  speed: string;
  eta: string;
  createdAt: string;
  playlistIndex?: number;
  playlistTotal?: number;
}

export default function App() {
  const [url, setUrl] = useState("");
  const [formatType, setFormatType] = useState<"video" | "audio">("video");
  const [quality, setQuality] = useState("1080p");
  const [isPlaylist, setIsPlaylist] = useState(false);
  const [useGpu, setUseGpu] = useState(true);
  const [savePath, setSavePath] = useState("");
  const [tasks, setTasks] = useState<TaskItem[]>([]);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [activeTab, setActiveTab] = useState<"all" | "active" | "completed">("all");

  // ---------------------------------------------------------------------------
  // 1. Tauri Real-Time Progress Event Listener
  // ---------------------------------------------------------------------------
  useEffect(() => {
    let unlistenFn: (() => void) | undefined;

    async function setupListener() {
      try {
        unlistenFn = await listen<DownloadProgressEvent>(
          "download-progress",
          (event) => {
            const data = event.payload;

            setTasks((prevTasks) =>
              prevTasks.map((task) => {
                if (task.taskId === data.task_id) {
                  // If the task has already been cancelled in UI, do not let late progress/error events override it
                  if (task.status === "Cancelled" && data.status !== "Cancelled") {
                    return task;
                  }

                  return {
                    ...task,
                    status: data.status,
                    percent: data.status === "Cancelled" ? 0 : data.percent,
                    speed: data.status === "Cancelled" ? "0 KiB/s" : data.speed || task.speed,
                    eta: data.status === "Cancelled" ? "--:--" : data.eta || task.eta,
                    filename: data.filename || task.filename,
                    playlistIndex:
                      typeof data.playlist_index === "number"
                        ? data.playlist_index
                        : task.playlistIndex,
                    playlistTotal:
                      typeof data.playlist_total === "number"
                        ? data.playlist_total
                        : task.playlistTotal,
                  };
                }
                return task;
              })
            );
          }
        );
      } catch (err) {
        console.error("Failed to subscribe to download-progress events:", err);
      }
    }

    setupListener();

    return () => {
      if (unlistenFn) {
        unlistenFn();
      }
    };
  }, []);

  // ---------------------------------------------------------------------------
  // 2. Clipboard & Folder Picker Helpers
  // ---------------------------------------------------------------------------
  const handlePasteClipboard = async () => {
    try {
      const text = await navigator.clipboard.readText();
      if (text && (text.startsWith("http://") || text.startsWith("https://"))) {
        setUrl(text.trim());
      }
    } catch (err) {
      console.warn("Unable to access clipboard:", err);
    }
  };

  const handlePickDirectory = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({
        directory: true,
        multiple: false,
        title: "Select Save Destination Directory",
      });
      if (typeof selected === "string") {
        setSavePath(selected);
      }
    } catch (err) {
      console.warn("Folder picker error or plugin not installed:", err);
    }
  };

  // ---------------------------------------------------------------------------
  // 3. Start & Cancel Handlers
  // ---------------------------------------------------------------------------
  const handleStartDownload = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!url.trim() || isSubmitting) return;

    setIsSubmitting(true);
    const tempTaskId = `task_${Date.now()}`;

    // Optimistically insert task into active queue
    const newTask: TaskItem = {
      taskId: tempTaskId,
      url: url.trim(),
      formatType,
      quality,
      filename: "Fetching stream metadata...",
      status: "Downloading",
      percent: 0,
      speed: "Connecting...",
      eta: "--:--",
      createdAt: new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }),
    };

    setTasks((prev) => [newTask, ...prev]);
    const targetUrl = url.trim();
    setUrl("");

    try {
      const assignedTaskId = await invoke<string>("start_download", {
        payload: {
          task_id: tempTaskId,
          url: targetUrl,
          format_type: formatType,
          quality,
          is_playlist: isPlaylist,
          use_gpu: useGpu,
          save_path: savePath.trim() || null,
        },
      });

      // Synchronize taskId if Rust backend assigned a different one
      if (assignedTaskId && assignedTaskId !== tempTaskId) {
        setTasks((prev) =>
          prev.map((t) => (t.taskId === tempTaskId ? { ...t, taskId: assignedTaskId } : t))
        );
      }
    } catch (err) {
      console.error("Failed to start download:", err);
      setTasks((prev) =>
        prev.map((t) =>
          t.taskId === tempTaskId
            ? { ...t, status: "Error", filename: String(err) }
            : t
        )
      );
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleCancelDownload = async (taskId: string) => {
    // 1. Immediately update UI state to "Cancelled" and stop showing active progress speeds
    setTasks((prevTasks) =>
      prevTasks.map((t) =>
        t.taskId === taskId
          ? {
              ...t,
              status: "Cancelled",
              speed: "0 KiB/s",
              eta: "--:--",
            }
          : t
      )
    );

    // 2. Invoke Rust backend: pass both taskId and task_id for bulletproof parameter casing
    try {
      await invoke("cancel_download", { taskId, task_id: taskId });
    } catch (err) {
      console.error("Failed to cancel download:", err);
    }
  };

  const handleRemoveTask = (taskId: string) => {
    setTasks((prev) => prev.filter((t) => t.taskId !== taskId));
  };

  const handleClearCompleted = () => {
    setTasks((prev) =>
      prev.filter((t) => t.status === "Downloading" || t.status === "Processing/GPU")
    );
  };

  // ---------------------------------------------------------------------------
  // 4. Filter & Derived Counts
  // ---------------------------------------------------------------------------
  const activeCount = tasks.filter(
    (t) => t.status === "Downloading" || t.status === "Processing/GPU"
  ).length;

  const completedCount = tasks.filter((t) => t.status === "Completed").length;

  const filteredTasks = tasks.filter((t) => {
    if (activeTab === "active") {
      return t.status === "Downloading" || t.status === "Processing/GPU";
    }
    if (activeTab === "completed") {
      return t.status === "Completed";
    }
    return true;
  });

  return (
    <div className="flex flex-col h-screen w-screen bg-neutral-950 text-neutral-100 font-sans select-none overflow-hidden antialiased">
      {/* ===================================================================== */}
      {/* HEADER                                                                */}
      {/* ===================================================================== */}
      <header className="flex items-center justify-between px-6 py-3.5 bg-neutral-900/70 border-b border-neutral-800/80 backdrop-blur-md shrink-0">
        <div className="flex items-center gap-3">
          <div className="flex items-center justify-center w-9 h-9 rounded-xl bg-gradient-to-tr from-cyan-500 via-indigo-600 to-purple-600 shadow-lg shadow-indigo-500/20">
            <Zap className="w-5 h-5 text-white fill-white/20" />
          </div>
          <div>
            <div className="flex items-center gap-2">
              <h1 className="text-base font-bold tracking-tight bg-gradient-to-r from-neutral-100 via-neutral-200 to-neutral-400 bg-clip-text text-transparent">
                Gen Downloader
              </h1>
              <span className="text-[10px] uppercase font-semibold px-1.5 py-0.5 rounded bg-neutral-800 text-neutral-400 border border-neutral-700/60">
                v2.0
              </span>
            </div>
            <p className="text-xs text-neutral-400">High-Performance Media Engine</p>
          </div>
        </div>

        {/* Dynamic GPU Status Badge */}
        <div className="flex items-center gap-2 px-3 py-1 rounded-full bg-emerald-500/10 border border-emerald-500/20 text-emerald-400 text-xs font-medium shadow-sm shadow-emerald-950">
          <span className="relative flex h-2 w-2">
            <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75"></span>
            <span className="relative inline-flex rounded-full h-2 w-2 bg-emerald-500"></span>
          </span>
          <Cpu className="w-3.5 h-3.5" />
          <span>GPU Acceleration: NVENC Ready</span>
        </div>
      </header>

      {/* ===================================================================== */}
      {/* MAIN CONTENT AREA                                                     */}
      {/* ===================================================================== */}
      <main className="flex flex-col flex-1 overflow-hidden p-6 gap-5">
        {/* URL Input & Controls Container */}
        <section className="bg-neutral-900/60 border border-neutral-800/80 rounded-2xl p-5 shadow-xl backdrop-blur-sm shrink-0 flex flex-col gap-4">
          <form onSubmit={handleStartDownload} className="flex gap-2">
            <div className="relative flex-1">
              <input
                type="text"
                placeholder="Paste media link here (YouTube, Rumble, X, Twitch, Vimeo...)"
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                className="w-full h-11 pl-4 pr-24 text-sm bg-neutral-950/80 border border-neutral-800 rounded-xl text-neutral-100 placeholder:text-neutral-500 focus:outline-none focus:border-indigo-500 focus:ring-2 focus:ring-indigo-500/20 transition-all"
              />
              <button
                type="button"
                onClick={handlePasteClipboard}
                className="absolute right-2 top-1/2 -translate-y-1/2 px-2.5 py-1.5 text-xs font-medium text-neutral-400 hover:text-neutral-200 bg-neutral-800 hover:bg-neutral-700/80 border border-neutral-700/60 rounded-lg flex items-center gap-1.5 transition-colors cursor-pointer"
                title="Paste from clipboard"
              >
                <Clipboard className="w-3.5 h-3.5" />
                <span>Paste</span>
              </button>
            </div>

            <button
              type="submit"
              disabled={!url.trim() || isSubmitting}
              className="px-6 h-11 rounded-xl font-medium text-sm text-white bg-gradient-to-r from-cyan-500 via-indigo-600 to-purple-600 hover:from-cyan-400 hover:via-indigo-500 hover:to-purple-500 disabled:opacity-50 disabled:cursor-not-allowed shadow-lg shadow-indigo-600/25 hover:shadow-indigo-600/40 flex items-center gap-2 transition-all cursor-pointer"
            >
              {isSubmitting ? (
                <RefreshCw className="w-4 h-4 animate-spin" />
              ) : (
                <Download className="w-4 h-4" />
              )}
              <span>Download</span>
            </button>
          </form>

          {/* Configuration Toolbar */}
          <div className="flex flex-wrap items-center justify-between gap-4 pt-1 border-t border-neutral-800/60 text-xs">
            {/* Format & Quality */}
            <div className="flex items-center gap-3">
              {/* Video / Audio Switcher */}
              <div className="flex p-1 bg-neutral-950/80 border border-neutral-800 rounded-xl">
                <button
                  type="button"
                  onClick={() => setFormatType("video")}
                  className={`px-3 py-1.5 rounded-lg font-medium flex items-center gap-1.5 transition-all cursor-pointer ${
                    formatType === "video"
                      ? "bg-neutral-800 text-cyan-400 shadow-sm"
                      : "text-neutral-400 hover:text-neutral-200"
                  }`}
                >
                  <Video className="w-3.5 h-3.5" />
                  <span>Video (MP4)</span>
                </button>
                <button
                  type="button"
                  onClick={() => setFormatType("audio")}
                  className={`px-3 py-1.5 rounded-lg font-medium flex items-center gap-1.5 transition-all cursor-pointer ${
                    formatType === "audio"
                      ? "bg-neutral-800 text-purple-400 shadow-sm"
                      : "text-neutral-400 hover:text-neutral-200"
                  }`}
                >
                  <Music className="w-3.5 h-3.5" />
                  <span>Audio (MP3)</span>
                </button>
              </div>

              {/* Quality Dropdown */}
              {formatType === "video" && (
                <div className="flex items-center gap-2">
                  <span className="text-neutral-400 font-medium">Quality:</span>
                  <select
                    value={quality}
                    onChange={(e) => setQuality(e.target.value)}
                    className="bg-neutral-950/80 border border-neutral-800 rounded-xl px-3 py-1.5 text-neutral-200 text-xs focus:outline-none focus:border-indigo-500 cursor-pointer"
                  >
                    <option value="4k">4K Ultra HD (2160p)</option>
                    <option value="1080p">Full HD (1080p)</option>
                    <option value="720p">High Definition (720p)</option>
                    <option value="best">Best Available</option>
                  </select>
                </div>
              )}
            </div>

            {/* Options & Destination Folder */}
            <div className="flex items-center gap-4">
              {/* Playlist Toggle */}
              <label className="flex items-center gap-2 text-neutral-300 cursor-pointer hover:text-neutral-100 transition-colors">
                <input
                  type="checkbox"
                  checked={isPlaylist}
                  onChange={(e) => setIsPlaylist(e.target.checked)}
                  className="rounded border-neutral-700 bg-neutral-950 text-indigo-600 focus:ring-indigo-500 cursor-pointer"
                />
                <Layers className="w-3.5 h-3.5 text-neutral-400" />
                <span>Entire Playlist</span>
              </label>

              {/* GPU Hardware Muxing Switch */}
              <label className="flex items-center gap-2 text-neutral-300 cursor-pointer hover:text-neutral-100 transition-colors">
                <input
                  type="checkbox"
                  checked={useGpu}
                  onChange={(e) => setUseGpu(e.target.checked)}
                  className="rounded border-neutral-700 bg-neutral-950 text-indigo-600 focus:ring-indigo-500 cursor-pointer"
                />
                <Sparkles className="w-3.5 h-3.5 text-cyan-400" />
                <span>GPU Muxing</span>
              </label>

              {/* Destination Folder Picker */}
              <button
                type="button"
                onClick={handlePickDirectory}
                className="px-3 py-1.5 bg-neutral-950/80 hover:bg-neutral-800 border border-neutral-800 hover:border-neutral-700 rounded-xl text-neutral-300 hover:text-neutral-100 flex items-center gap-1.5 transition-colors cursor-pointer"
                title={savePath ? `Save Path: ${savePath}` : "Set download folder"}
              >
                {savePath ? (
                  <FolderCheck className="w-3.5 h-3.5 text-emerald-400" />
                ) : (
                  <Folder className="w-3.5 h-3.5 text-neutral-400" />
                )}
                <span className="max-w-[140px] truncate">
                  {savePath ? savePath.split(/[\\/]/).pop() || savePath : "Default Folder"}
                </span>
              </button>
            </div>
          </div>
        </section>

        {/* Task List Section */}
        <section className="flex flex-col flex-1 min-h-0 bg-neutral-900/40 border border-neutral-800/80 rounded-2xl overflow-hidden backdrop-blur-sm">
          {/* List Header & Tabs */}
          <div className="flex items-center justify-between px-5 py-3 border-b border-neutral-800/80 shrink-0 bg-neutral-900/60">
            <div className="flex items-center gap-2">
              <button
                onClick={() => setActiveTab("all")}
                className={`px-3 py-1 rounded-lg text-xs font-medium transition-colors cursor-pointer ${
                  activeTab === "all"
                    ? "bg-neutral-800 text-neutral-100"
                    : "text-neutral-400 hover:text-neutral-200"
                }`}
              >
                All Tasks ({tasks.length})
              </button>
              <button
                onClick={() => setActiveTab("active")}
                className={`px-3 py-1 rounded-lg text-xs font-medium transition-colors cursor-pointer ${
                  activeTab === "active"
                    ? "bg-cyan-500/10 text-cyan-400 border border-cyan-500/20"
                    : "text-neutral-400 hover:text-neutral-200"
                }`}
              >
                Active ({activeCount})
              </button>
              <button
                onClick={() => setActiveTab("completed")}
                className={`px-3 py-1 rounded-lg text-xs font-medium transition-colors cursor-pointer ${
                  activeTab === "completed"
                    ? "bg-emerald-500/10 text-emerald-400 border border-emerald-500/20"
                    : "text-neutral-400 hover:text-neutral-200"
                }`}
              >
                Completed ({completedCount})
              </button>
            </div>

            {completedCount > 0 && (
              <button
                onClick={handleClearCompleted}
                className="text-xs text-neutral-400 hover:text-neutral-200 flex items-center gap-1.5 transition-colors cursor-pointer"
              >
                <Trash2 className="w-3.5 h-3.5" />
                <span>Clear Completed</span>
              </button>
            )}
          </div>

          {/* Scrollable Tasks Container */}
          <div className="flex-1 overflow-y-auto p-4 space-y-3">
            {filteredTasks.length === 0 ? (
              <div className="flex flex-col items-center justify-center h-full text-neutral-500 py-12">
                <Download className="w-10 h-10 mb-3 stroke-[1.25] text-neutral-600" />
                <p className="text-sm font-medium">No downloads in this view</p>
                <p className="text-xs text-neutral-600 mt-1">
                  Paste a link above to start downloading at high speed
                </p>
              </div>
            ) : (
              filteredTasks.map((task) => (
                <div
                  key={task.taskId}
                  className="group relative bg-neutral-950/70 border border-neutral-800/80 hover:border-neutral-700/80 rounded-xl p-4 transition-all shadow-sm flex flex-col gap-3"
                >
                  {/* Top Row: Icon, Title, Status & Actions */}
                  <div className="flex items-center justify-between gap-4">
                    <div className="flex items-center gap-3 min-w-0">
                      <div className="flex items-center justify-center w-8 h-8 rounded-lg bg-neutral-900 border border-neutral-800 text-neutral-300 shrink-0">
                        {task.formatType === "audio" ? (
                          <Music className="w-4 h-4 text-purple-400" />
                        ) : (
                          <Video className="w-4 h-4 text-cyan-400" />
                        )}
                      </div>
                      <div className="min-w-0">
                        <div className="flex items-center gap-2 flex-wrap">
                          <p className="text-sm font-medium text-neutral-200 truncate" title={task.filename}>
                            {task.filename}
                          </p>
                          {task.playlistTotal !== undefined && task.playlistTotal > 0 && (
                            <span className="inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-[11px] font-semibold bg-gradient-to-r from-indigo-500/20 to-purple-500/20 text-indigo-300 border border-indigo-500/40 shadow-sm shrink-0">
                              <Layers className="w-3 h-3 text-indigo-400" />
                              Video {task.playlistIndex ?? 1} of {task.playlistTotal}
                            </span>
                          )}
                        </div>
                        <p className="text-xs text-neutral-500 truncate max-w-sm" title={task.url}>
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

                      {/* Cancel / Dismiss Buttons */}
                      {task.status === "Downloading" || task.status === "Processing/GPU" ? (
                        <button
                          type="button"
                          onClick={() => handleCancelDownload(task.taskId)}
                          className="p-1.5 text-neutral-400 hover:text-rose-400 hover:bg-rose-500/10 rounded-lg transition-colors cursor-pointer"
                          title="Cancel Download"
                        >
                          <XCircle className="w-4 h-4" />
                        </button>
                      ) : (
                        <button
                          type="button"
                          onClick={() => handleRemoveTask(task.taskId)}
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
                      <span>{task.quality.toUpperCase()}</span>
                      <span>•</span>
                      <span>{task.createdAt}</span>
                    </div>
                  </div>
                </div>
              ))
            )}
          </div>
        </section>
      </main>
    </div>
  );
}
