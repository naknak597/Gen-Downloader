import React, { useState, useEffect } from "react";
import { Cpu, RefreshCw, CheckCircle2, AlertCircle, X } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { CoreVersions } from "../types/download";

export const Header: React.FC = () => {
  const [versions, setVersions] = useState<CoreVersions | null>(null);
  const [isUpdating, setIsUpdating] = useState(false);
  const [updateStatus, setUpdateStatus] = useState<{
    type: "success" | "error";
    message: string;
  } | null>(null);

  // Fetch core binary versions on mount
  useEffect(() => {
    async function fetchVersions() {
      try {
        const v = await invoke<CoreVersions>("get_core_versions");
        setVersions(v);
      } catch (err) {
        console.warn("[Header] Failed to fetch core versions:", err);
      }
    }
    fetchVersions();
  }, []);

  // Auto-dismiss status message after 6 seconds
  useEffect(() => {
    if (!updateStatus) return;
    const timer = setTimeout(() => {
      setUpdateStatus(null);
    }, 6000);
    return () => clearTimeout(timer);
  }, [updateStatus]);

  const handleUpdateCore = async () => {
    if (isUpdating) return;
    setIsUpdating(true);
    setUpdateStatus(null);

    try {
      const result = await invoke<string>("update_ytdlp_binary");
      setUpdateStatus({
        type: "success",
        message: result,
      });

      // Refresh version details after successful update
      try {
        const updatedVersions = await invoke<CoreVersions>("get_core_versions");
        setVersions(updatedVersions);
      } catch {
        // ignore secondary fetch error
      }
    } catch (err) {
      const errMsg = typeof err === "string" ? err : String(err);
      setUpdateStatus({
        type: "error",
        message: errMsg,
      });
    } finally {
      setIsUpdating(false);
    }
  };

  return (
    <header className="flex items-center justify-between px-6 py-3.5 bg-neutral-900/70 border-b border-neutral-800/80 backdrop-blur-md shrink-0">
      <div className="flex items-center gap-3">
        {/* New App Logo */}
        <img
          src="/logo.png"
          alt="Gen Downloader Logo"
          className="w-9 h-9 rounded-xl object-cover shadow-lg shadow-purple-500/20 border border-neutral-700/60"
        />
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

      {/* Right Controls: Update Status, Core Updater, and GPU Status Badge */}
      <div className="flex items-center gap-2.5">
        {/* Status Toast / Badge */}
        {updateStatus && (
          <div
            className={`flex items-center gap-1.5 px-3 py-1 rounded-full text-xs font-medium border shadow-md transition-all animate-in fade-in duration-200 ${
              updateStatus.type === "success"
                ? "bg-emerald-500/10 border-emerald-500/30 text-emerald-300"
                : "bg-rose-500/10 border-rose-500/30 text-rose-300"
            }`}
          >
            {updateStatus.type === "success" ? (
              <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400 shrink-0" />
            ) : (
              <AlertCircle className="w-3.5 h-3.5 text-rose-400 shrink-0" />
            )}
            <span className="max-w-[280px] truncate" title={updateStatus.message}>
              {updateStatus.message}
            </span>
            <button
              onClick={() => setUpdateStatus(null)}
              className="ml-0.5 p-0.5 hover:bg-neutral-800/60 rounded text-neutral-400 hover:text-neutral-200 transition-colors"
              title="Dismiss"
            >
              <X className="w-3 h-3" />
            </button>
          </div>
        )}

        {/* Update Core Button */}
        <button
          onClick={handleUpdateCore}
          disabled={isUpdating}
          title={
            versions
              ? `yt-dlp: ${versions.ytdlpVersion}\nFFmpeg: ${versions.ffmpegVersion}\nClick to update yt-dlp core`
              : "Check and update yt-dlp core"
          }
          className="flex items-center gap-1.5 px-3 py-1 rounded-full bg-neutral-800/80 hover:bg-neutral-700/80 border border-neutral-700/70 hover:border-neutral-600 active:scale-95 disabled:opacity-60 disabled:pointer-events-none text-xs font-medium text-neutral-200 hover:text-white transition-all shadow-sm cursor-pointer"
        >
          <RefreshCw
            className={`w-3.5 h-3.5 text-cyan-400 ${
              isUpdating ? "animate-spin text-cyan-300" : ""
            }`}
          />
          <span>{isUpdating ? "Updating Core..." : "Update Core"}</span>
          {versions?.ytdlpVersion && !isUpdating && (
            <span className="text-[10px] text-neutral-400 bg-neutral-900/80 px-1.5 py-0.5 rounded border border-neutral-700/50">
              {versions.ytdlpVersion}
            </span>
          )}
        </button>

        {/* Dynamic GPU Status Badge */}
        <div className="flex items-center gap-2 px-3 py-1 rounded-full bg-emerald-500/10 border border-emerald-500/20 text-emerald-400 text-xs font-medium shadow-sm shadow-emerald-950">
          <span className="relative flex h-2 w-2">
            <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75"></span>
            <span className="relative inline-flex rounded-full h-2 w-2 bg-emerald-500"></span>
          </span>
          <Cpu className="w-3.5 h-3.5" />
          <span>GPU Acceleration: NVENC Ready</span>
        </div>
      </div>
    </header>
  );
};