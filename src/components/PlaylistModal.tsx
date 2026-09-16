import React, { useState, useEffect, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  ListMusic,
  Music,
  Video,
  Play,
  Search,
  X,
  FolderOpen,
  AlertCircle,
  Loader2,
  HardDrive,
  CheckCircle2,
} from "lucide-react";
import { PlaylistItem } from "../types/download";

interface PlaylistModalProps {
  isOpen: boolean;
  playlistPath: string;
  playlistTitle: string;
  onClose: () => void;
}

function formatFileSize(bytes: number): string {
  if (!bytes || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  return `${(bytes / Math.pow(1024, i)).toFixed(1)} ${units[i]}`;
}

export const PlaylistModal: React.FC<PlaylistModalProps> = ({
  isOpen,
  playlistPath,
  playlistTitle,
  onClose,
}) => {
  const [items, setItems] = useState<PlaylistItem[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [activePlayingPath, setActivePlayingPath] = useState<string | null>(null);

  // Fetch playlist tracks when modal is opened
  useEffect(() => {
    if (!isOpen || !playlistPath) {
      // Clean up modal state when closed
      setItems([]);
      setSearchQuery("");
      setError(null);
      setActivePlayingPath(null);
      return;
    }

    let isMounted = true;
    setIsLoading(true);
    setError(null);
    setSearchQuery("");

    async function loadTracks() {
      try {
        const fetchedItems = await invoke<PlaylistItem[]>("get_playlist_items", {
          dirPath: playlistPath,
          dir_path: playlistPath,
        });

        if (isMounted) {
          setItems(fetchedItems || []);
        }
      } catch (err) {
        console.error("Failed to load playlist items:", err);
        if (isMounted) {
          setError(String(err));
        }
      } finally {
        if (isMounted) {
          setIsLoading(false);
        }
      }
    }

    loadTracks();

    return () => {
      isMounted = false;
    };
  }, [isOpen, playlistPath]);

  // Handle Escape key to close modal
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape" && isOpen) {
        onClose();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, onClose]);

  // Filter tracks efficiently using useMemo
  const filteredItems = useMemo(() => {
    const query = searchQuery.trim().toLowerCase();
    if (!query) return items;

    return items.filter(
      (item) =>
        item.name.toLowerCase().includes(query) ||
        item.extension.toLowerCase().includes(query)
    );
  }, [items, searchQuery]);

  // Calculate total playlist file size
  const totalPlaylistSize = useMemo(() => {
    return items.reduce((acc, cur) => acc + (cur.fileSize || 0), 0);
  }, [items]);

  if (!isOpen) return null;

  const handlePlayTrack = async (item: PlaylistItem) => {
    try {
      setActivePlayingPath(item.filePath);
      await invoke("open_media_file", {
        filePath: item.filePath,
        file_path: item.filePath,
      });
      // Clear highlight after 3 seconds
      setTimeout(() => {
        setActivePlayingPath((prev) => (prev === item.filePath ? null : prev));
      }, 3000);
    } catch (err) {
      console.error("Failed to play track:", err);
    }
  };

  const handleOpenFolder = async () => {
    if (!playlistPath) return;
    try {
      await invoke("show_in_folder", {
        filePath: playlistPath,
        file_path: playlistPath,
      });
    } catch (err) {
      console.error("Failed to open folder:", err);
    }
  };

  const isAudioExt = (ext: string) =>
    ["mp3", "m4a", "wav", "flac", "aac", "ogg"].includes(ext.toLowerCase());

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/80 backdrop-blur-sm animate-in fade-in duration-200"
      onClick={onClose}
    >
      <div
        className="relative w-full max-w-3xl bg-neutral-900 border border-neutral-800 rounded-2xl shadow-2xl overflow-hidden flex flex-col max-h-[85vh] animate-in zoom-in-95 duration-200"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Modal Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-neutral-800/80 bg-neutral-900/95 shrink-0">
          <div className="flex items-center gap-3 min-w-0">
            <div className="flex items-center justify-center w-9 h-9 rounded-xl bg-gradient-to-br from-indigo-500/20 to-purple-500/20 border border-indigo-500/30 text-indigo-400 shrink-0 shadow-sm">
              <ListMusic className="w-5 h-5" />
            </div>
            <div className="min-w-0">
              <div className="flex items-center gap-2">
                <h2 className="text-base font-semibold text-neutral-100 truncate">
                  {playlistTitle || "Playlist Tracks"}
                </h2>
                {!isLoading && items.length > 0 && (
                  <span className="px-2 py-0.5 rounded-full text-[11px] font-semibold bg-indigo-500/15 text-indigo-300 border border-indigo-500/30 shrink-0">
                    {items.length} {items.length === 1 ? "track" : "tracks"}
                  </span>
                )}
              </div>
              <p
                className="text-xs text-neutral-400 truncate max-w-lg mt-0.5"
                title={playlistPath}
              >
                {playlistPath}
              </p>
            </div>
          </div>

          <div className="flex items-center gap-2 shrink-0">
            <button
              type="button"
              onClick={handleOpenFolder}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-medium bg-neutral-800 hover:bg-neutral-700 text-neutral-300 hover:text-neutral-100 border border-neutral-700/70 transition-colors cursor-pointer"
              title={`Open playlist folder in Explorer:\n${playlistPath}`}
            >
              <FolderOpen className="w-3.5 h-3.5 text-neutral-400" />
              <span>Folder</span>
            </button>

            <button
              type="button"
              onClick={onClose}
              className="p-1.5 text-neutral-400 hover:text-neutral-200 hover:bg-neutral-800 rounded-lg transition-colors cursor-pointer"
              title="Close dialog (Esc)"
            >
              <X className="w-5 h-5" />
            </button>
          </div>
        </div>

        {/* Toolbar & Search Bar */}
        <div className="px-6 py-3 bg-neutral-950/70 border-b border-neutral-800/60 flex items-center justify-between gap-4 shrink-0">
          <div className="relative flex-1 max-w-md">
            <Search className="w-4 h-4 text-neutral-500 absolute left-3 top-1/2 -translate-y-1/2" />
            <input
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="Filter tracks by name or format..."
              className="w-full pl-9 pr-8 py-1.5 bg-neutral-900 border border-neutral-800 rounded-xl text-xs text-neutral-200 placeholder-neutral-500 focus:outline-none focus:border-indigo-500/60 focus:ring-1 focus:ring-indigo-500/30 transition-colors"
            />
            {searchQuery && (
              <button
                type="button"
                onClick={() => setSearchQuery("")}
                className="absolute right-2.5 top-1/2 -translate-y-1/2 text-neutral-500 hover:text-neutral-300 p-0.5 cursor-pointer"
                title="Clear filter"
              >
                <X className="w-3.5 h-3.5" />
              </button>
            )}
          </div>

          <div className="flex items-center gap-3 text-xs text-neutral-400 font-mono shrink-0">
            {totalPlaylistSize > 0 && (
              <span className="flex items-center gap-1.5">
                <HardDrive className="w-3.5 h-3.5 text-neutral-500" />
                <span>{formatFileSize(totalPlaylistSize)}</span>
              </span>
            )}
            {searchQuery && (
              <span className="text-[11px] text-neutral-400 font-sans">
                Showing {filteredItems.length} of {items.length}
              </span>
            )}
          </div>
        </div>

        {/* Track List Body */}
        <div className="flex-1 p-5 overflow-y-auto min-h-0 bg-neutral-950/40 flex flex-col gap-2">
          {/* Loading state */}
          {isLoading && (
            <div className="flex flex-col items-center justify-center py-16 text-neutral-400">
              <Loader2 className="w-8 h-8 animate-spin text-indigo-400 mb-3" />
              <p className="text-sm font-medium">Scanning playlist tracks...</p>
              <p className="text-xs text-neutral-500 mt-1">
                Reading media files and organizing track order
              </p>
            </div>
          )}

          {/* Error state */}
          {!isLoading && error && (
            <div className="flex items-start gap-3 p-4 rounded-xl bg-rose-500/10 border border-rose-500/20 text-rose-300">
              <AlertCircle className="w-5 h-5 shrink-0 mt-0.5" />
              <div className="text-xs">
                <p className="font-semibold text-rose-200">Unable to load playlist</p>
                <p className="mt-1 text-rose-300/90">{error}</p>
              </div>
            </div>
          )}

          {/* Empty Folder state */}
          {!isLoading && !error && items.length === 0 && (
            <div className="flex flex-col items-center justify-center py-16 text-neutral-500">
              <ListMusic className="w-10 h-10 mb-3 stroke-[1.25] text-neutral-600" />
              <p className="text-sm font-medium text-neutral-300">No media tracks found</p>
              <p className="text-xs text-neutral-500 mt-1">
                The folder may still be finalizing or contains non-standard media.
              </p>
            </div>
          )}

          {/* Empty Search Filter state */}
          {!isLoading && !error && items.length > 0 && filteredItems.length === 0 && (
            <div className="flex flex-col items-center justify-center py-12 text-neutral-500">
              <Search className="w-8 h-8 mb-2 stroke-[1.25] text-neutral-600" />
              <p className="text-sm font-medium text-neutral-300">No matching tracks</p>
              <p className="text-xs text-neutral-500 mt-1">
                No files match "{searchQuery}". Try a different search keyword.
              </p>
            </div>
          )}

          {/* Track Items */}
          {!isLoading &&
            !error &&
            filteredItems.map((item, index) => {
              const isAudio = isAudioExt(item.extension);
              const isPlaying = activePlayingPath === item.filePath;

              return (
                <div
                  key={item.filePath}
                  onClick={() => handlePlayTrack(item)}
                  className={`group flex items-center justify-between p-3 rounded-xl border transition-all cursor-pointer select-none ${
                    isPlaying
                      ? "bg-emerald-500/10 border-emerald-500/40 shadow-sm"
                      : "bg-neutral-900/60 hover:bg-neutral-800/70 border-neutral-800/70 hover:border-neutral-700"
                  }`}
                >
                  <div className="flex items-center gap-3.5 min-w-0">
                    {/* Index Badge */}
                    <div className="w-7 text-center font-mono text-xs font-semibold text-neutral-500 group-hover:text-neutral-300 shrink-0">
                      {String(index + 1).padStart(2, "0")}
                    </div>

                    {/* Media Type Icon */}
                    <div
                      className={`flex items-center justify-center w-8 h-8 rounded-lg border shrink-0 transition-colors ${
                        isAudio
                          ? "bg-purple-500/10 border-purple-500/20 text-purple-400 group-hover:bg-purple-500/20"
                          : "bg-cyan-500/10 border-cyan-500/20 text-cyan-400 group-hover:bg-cyan-500/20"
                      }`}
                    >
                      {isAudio ? (
                        <Music className="w-4 h-4" />
                      ) : (
                        <Video className="w-4 h-4" />
                      )}
                    </div>

                    {/* Track Title and Meta */}
                    <div className="min-w-0">
                      <p
                        className={`text-xs font-medium truncate transition-colors ${
                          isPlaying
                            ? "text-emerald-300 font-semibold"
                            : "text-neutral-200 group-hover:text-white"
                        }`}
                        title={item.name}
                      >
                        {item.name}
                      </p>
                      <div className="flex items-center gap-2 mt-0.5 text-[11px] text-neutral-500 font-mono">
                        <span className="uppercase font-semibold text-neutral-400">
                          {item.extension}
                        </span>
                        <span>•</span>
                        <span>{formatFileSize(item.fileSize)}</span>
                      </div>
                    </div>
                  </div>

                  {/* Play Button & Feedback */}
                  <div className="flex items-center gap-2 shrink-0 ml-3">
                    {isPlaying ? (
                      <span className="flex items-center gap-1.5 px-3 py-1 rounded-full text-xs font-medium bg-emerald-500/20 text-emerald-400 border border-emerald-500/30 animate-pulse">
                        <CheckCircle2 className="w-3 h-3" />
                        <span>Playing</span>
                      </span>
                    ) : (
                      <button
                        type="button"
                        onClick={(e) => {
                          e.stopPropagation();
                          handlePlayTrack(item);
                        }}
                        className="flex items-center gap-1.5 px-3 py-1 rounded-full text-xs font-medium bg-neutral-800 group-hover:bg-indigo-600/90 text-neutral-300 group-hover:text-white border border-neutral-700/60 group-hover:border-indigo-500 transition-all cursor-pointer shadow-sm"
                        title="Play in default player"
                      >
                        <Play className="w-3 h-3 fill-current" />
                        <span>Play</span>
                      </button>
                    )}
                  </div>
                </div>
              );
            })}
        </div>

        {/* Modal Footer */}
        <div className="flex items-center justify-between px-6 py-3.5 bg-neutral-900 border-t border-neutral-800 shrink-0">
          <span className="text-xs text-neutral-500">
            Click any track to launch it instantly in your system's default player.
          </span>

          <button
            type="button"
            onClick={onClose}
            className="px-4 py-1.5 rounded-xl text-xs font-medium bg-neutral-800 hover:bg-neutral-700 text-neutral-200 border border-neutral-700 transition-colors cursor-pointer"
          >
            Close
          </button>
        </div>
      </div>
    </div>
  );
};
