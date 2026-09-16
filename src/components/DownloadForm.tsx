import React, { useState, useRef, useCallback, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Download,
  Folder,
  Clipboard,
  Video,
  Music,
  Sparkles,
  Layers,
  RefreshCw,
  FolderCheck,
} from "lucide-react";
import { FormOptions, FormatType, MediaMetadata } from "../types/download";
import { MediaPreviewCard } from "./MediaPreviewCard";
import { useClipboardWatcher } from "../hooks/useClipboardWatcher";

interface DownloadFormProps {
  onSubmit: (options: FormOptions) => void;
  isSubmitting: boolean;
}

export const DownloadForm: React.FC<DownloadFormProps> = ({
  onSubmit,
  isSubmitting,
}) => {
  const [url, setUrl] = useState("");
  const [formatType, setFormatType] = useState<FormatType>("video");
  const [quality, setQuality] = useState("1080p");
  const [isPlaylist, setIsPlaylist] = useState(false);
  const [useGpu, setUseGpu] = useState(true);
  const [savePath, setSavePath] = useState("");
  const [autoDetectClipboard, setAutoDetectClipboard] = useState(true);
  const [showClipboardToast, setShowClipboardToast] = useState(false);

  const inputRef = useRef<HTMLInputElement>(null);
  const clipboardToastTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Media preview metadata states
  const [preview, setPreview] = useState<MediaMetadata | null>(null);
  const [isLoadingPreview, setIsLoadingPreview] = useState(false);
  const lastFetchedUrlRef = useRef<string>("");
  const currentRequestIdRef = useRef<number>(0);

  const fetchPreview = useCallback(async (targetUrl: string) => {
    const trimmed = targetUrl.trim();
    if (
      trimmed.length <= 10 ||
      !/^https?:\/\//i.test(trimmed) ||
      trimmed === lastFetchedUrlRef.current
    ) {
      return;
    }

    lastFetchedUrlRef.current = trimmed;
    const requestId = ++currentRequestIdRef.current;
    setIsLoadingPreview(true);

    try {
      const data = await invoke<MediaMetadata>("get_media_info", {
        url: trimmed,
      });
      if (requestId === currentRequestIdRef.current) {
        setPreview(data);
      }
    } catch (err) {
      if (requestId === currentRequestIdRef.current) {
        console.warn("Failed to fetch media metadata:", err);
        setPreview(null);
      }
    } finally {
      if (requestId === currentRequestIdRef.current) {
        setIsLoadingPreview(false);
      }
    }
  }, []);

  // Handle automatic detection of copied media links
  const handleAutoDetectedUrl = useCallback(
    (detectedUrl: string) => {
      setUrl(detectedUrl);
      setShowClipboardToast(true);

      if (clipboardToastTimeoutRef.current) {
        clearTimeout(clipboardToastTimeoutRef.current);
      }
      clipboardToastTimeoutRef.current = setTimeout(() => {
        setShowClipboardToast(false);
      }, 4000);

      fetchPreview(detectedUrl);
    },
    [fetchPreview]
  );

  // Active clipboard watcher on window focus
  useClipboardWatcher(handleAutoDetectedUrl, autoDetectClipboard, inputRef);

  useEffect(() => {
    return () => {
      if (clipboardToastTimeoutRef.current) {
        clearTimeout(clipboardToastTimeoutRef.current);
      }
    };
  }, []);

  const handlePasteClipboard = async () => {
    try {
      const text = await navigator.clipboard.readText();
      if (text && (text.startsWith("http://") || text.startsWith("https://"))) {
        const trimmed = text.trim();
        setUrl(trimmed);
        fetchPreview(trimmed);
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

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!url.trim() || isSubmitting) return;

    onSubmit({
      url: url.trim(),
      formatType,
      quality,
      isPlaylist,
      useGpu,
      savePath,
    });

    setUrl("");
    setPreview(null);
    lastFetchedUrlRef.current = "";
    setShowClipboardToast(false);
  };

  const handleClearPreview = () => {
    setPreview(null);
  };

  const handleInputKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      if (url.trim().length > 10 && !preview && !isLoadingPreview) {
        e.preventDefault();
        fetchPreview(url);
      }
    }
  };

  return (
    <section className="bg-neutral-900/60 border border-neutral-800/80 rounded-2xl p-5 shadow-xl backdrop-blur-sm shrink-0 flex flex-col gap-4">
      <form onSubmit={handleSubmit} className="flex gap-2">
        <div className="relative flex-1">
          {/* Subtle auto-detected notification badge */}
          {showClipboardToast && (
            <div className="absolute -top-3 left-4 px-2.5 py-0.5 rounded-full bg-indigo-500/20 border border-indigo-500/40 text-indigo-300 text-[11px] font-medium backdrop-blur-md shadow-md shadow-indigo-950/40 flex items-center gap-1.5 animate-in fade-in slide-in-from-bottom-1 duration-200 z-10">
              <Sparkles className="w-3 h-3 text-cyan-300 animate-pulse" />
              <span>Link detected from clipboard</span>
            </div>
          )}

          <input
            ref={inputRef}
            type="text"
            placeholder="Paste media link here (YouTube, Rumble, X, Twitch, Vimeo...)"
            value={url}
            onChange={(e) => {
              const val = e.target.value;
              setUrl(val);
              if (showClipboardToast) {
                setShowClipboardToast(false);
              }
              if (!val.trim()) {
                setPreview(null);
                lastFetchedUrlRef.current = "";
              }
            }}
            onBlur={() => {
              if (url.trim().length > 10) {
                fetchPreview(url);
              }
            }}
            onKeyDown={handleInputKeyDown}
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

      {/* Loading Skeleton */}
      {isLoadingPreview && (
        <div className="relative flex flex-col sm:flex-row items-center gap-4 p-3.5 rounded-xl bg-neutral-950/70 border border-neutral-800/80 backdrop-blur-md animate-pulse">
          <div className="aspect-video w-full sm:w-44 rounded-lg bg-neutral-900 border border-neutral-800/80 flex items-center justify-center shrink-0">
            <RefreshCw className="w-5 h-5 text-neutral-500 animate-spin" />
          </div>
          <div className="flex-1 w-full space-y-2.5">
            <div className="h-4 bg-neutral-800/70 rounded-md w-3/4" />
            <div className="h-3 bg-neutral-800/50 rounded-md w-1/3" />
            <div className="flex items-center gap-2 pt-1">
              <div className="h-6 bg-neutral-800/60 rounded-md w-24" />
              <div className="h-6 bg-neutral-800/60 rounded-md w-16" />
            </div>
          </div>
        </div>
      )}

      {/* Media Preview Card */}
      {!isLoadingPreview && preview && (
        <MediaPreviewCard metadata={preview} onClear={handleClearPreview} />
      )}

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

        {/* Options, Clipboard Auto-Detect & Destination Folder */}
        <div className="flex items-center gap-4">
          {/* Clipboard Auto-Detect Switch */}
          <label
            className={`flex items-center gap-1.5 cursor-pointer transition-colors ${
              autoDetectClipboard
                ? "text-neutral-300 hover:text-neutral-100"
                : "text-neutral-500 hover:text-neutral-400"
            }`}
            title="Automatically populate link and preview when you copy a media URL and switch to Gen Downloader"
          >
            <input
              type="checkbox"
              checked={autoDetectClipboard}
              onChange={(e) => setAutoDetectClipboard(e.target.checked)}
              className="rounded border-neutral-700 bg-neutral-950 text-indigo-600 focus:ring-indigo-500 cursor-pointer"
            />
            <Clipboard
              className={`w-3.5 h-3.5 ${
                autoDetectClipboard ? "text-cyan-400" : "text-neutral-500"
              }`}
            />
            <span>Auto-Detect</span>
          </label>

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
  );
};
