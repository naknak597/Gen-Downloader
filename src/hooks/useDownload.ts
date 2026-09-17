import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { DownloadTask, DownloadProgressEvent, FormOptions } from "../types/download";
import {
  ensureNotificationPermission,
  notifyTaskComplete,
  notifyTaskFailed,
} from "../utils/notifications";

export function useDownload() {
  const [tasks, setTasks] = useState<DownloadTask[]>([]);
  const [isSubmitting, setIsSubmitting] = useState(false);

  // Request notification permissions once on component mount
  useEffect(() => {
    ensureNotificationPermission();
  }, []);

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
                  // Do not let late progress events overwrite a user-cancelled task
                  if (task.status === "Cancelled" && data.status !== "Cancelled") {
                    return task;
                  }

                  // Alert user with chime + toast on completion or failure
                  if (data.status === "Completed" && task.status !== "Completed") {
                    notifyTaskComplete(data.filename || task.filename, task.isPlaylist);
                  } else if (data.status === "Error" && task.status !== "Error") {
                    notifyTaskFailed(
                      data.filename || task.filename,
                      data.error_log || undefined
                    );
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
                    errorLog: data.error_log !== undefined && data.error_log !== null ? data.error_log : task.errorLog,
                    filePath: data.file_path !== undefined && data.file_path !== null ? data.file_path : task.filePath,
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
  // 2. Start Download Action
  // ---------------------------------------------------------------------------
  const startDownload = useCallback(async (options: FormOptions) => {
    const targetUrl = options.url.trim();
    if (!targetUrl || isSubmitting) return;

    setIsSubmitting(true);
    const tempTaskId = `task_${Date.now()}`;

    // Optimistically insert task into active queue
    const newTask: DownloadTask = {
      taskId: tempTaskId,
      url: targetUrl,
      formatType: options.formatType,
      quality: options.quality,
      isPlaylist: options.isPlaylist,
      filename: "Fetching stream metadata...",
      status: "Downloading",
      percent: 0,
      speed: "Connecting...",
      eta: "--:--",
      createdAt: new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }),
    };

    setTasks((prev) => [newTask, ...prev]);

    try {
      const assignedTaskId = await invoke<string>("start_download", {
        payload: {
          task_id: tempTaskId,
          url: targetUrl,
          format_type: options.formatType,
          quality: options.quality,
          is_playlist: options.isPlaylist,
          use_gpu: options.useGpu,
          save_path: options.savePath.trim() || null,
          download_subtitle: options.downloadSubtitle ?? false,
          subtitle_lang: options.subtitleLang || null,
          download_thumbnail: options.downloadThumbnail ?? false,
          download_metadata: options.downloadMetadata ?? false,
          browser_cookies: options.browserCookies || null,
          browserCookies: options.browserCookies || undefined,
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
      const errMsg = String(err);
      notifyTaskFailed(targetUrl, errMsg);
      setTasks((prev) =>
        prev.map((t) =>
          t.taskId === tempTaskId
            ? { ...t, status: "Error", filename: errMsg, errorLog: errMsg }
            : t
        )
      );
    } finally {
      setIsSubmitting(false);
    }
  }, [isSubmitting]);

  // ---------------------------------------------------------------------------
  // 3. Cancel Download Action
  // ---------------------------------------------------------------------------
  const cancelDownload = useCallback(async (taskId: string) => {
    // Immediately reflect cancellation in UI
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

    try {
      await invoke("cancel_download", { taskId, task_id: taskId });
    } catch (err) {
      console.error("Failed to cancel download:", err);
    }
  }, []);

  // ---------------------------------------------------------------------------
  // 4. Utility Actions
  // ---------------------------------------------------------------------------
  const removeTask = useCallback((taskId: string) => {
    setTasks((prev) => prev.filter((t) => t.taskId !== taskId));
  }, []);

  const clearCompleted = useCallback(() => {
    setTasks((prev) => prev.filter((t) => t.status !== "Completed"));
  }, []);

  const activeCount = tasks.filter(
    (t) => t.status === "Downloading" || t.status === "Processing/GPU"
  ).length;

  const completedCount = tasks.filter((t) => t.status === "Completed").length;

  return {
    tasks,
    isSubmitting,
    startDownload,
    cancelDownload,
    removeTask,
    clearCompleted,
    activeCount,
    completedCount,
  };
}
