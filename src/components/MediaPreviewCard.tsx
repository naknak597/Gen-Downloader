import React, { useState } from "react";
import { Video, Clock, User, Globe, X } from "lucide-react";
import { MediaMetadata } from "../types/download";

interface MediaPreviewCardProps {
  metadata: MediaMetadata;
  onClear: () => void;
}

function formatDuration(seconds?: number): string | null {
  if (seconds === undefined || seconds === null || isNaN(seconds) || seconds < 0) {
    return null;
  }
  const totalSecs = Math.floor(seconds);
  const hrs = Math.floor(totalSecs / 3600);
  const mins = Math.floor((totalSecs % 3600) / 60);
  const secs = totalSecs % 60;

  if (hrs > 0) {
    return `${hrs}:${mins.toString().padStart(2, "0")}:${secs.toString().padStart(2, "0")}`;
  }
  return `${mins}:${secs.toString().padStart(2, "0")}`;
}

function extractDomain(url: string): string | null {
  try {
    const parsed = new URL(url);
    return parsed.hostname.replace(/^www\./, "");
  } catch {
    return null;
  }
}

export const MediaPreviewCard: React.FC<MediaPreviewCardProps> = ({
  metadata,
  onClear,
}) => {
  const [imageError, setImageError] = useState(false);
  const formattedDuration = formatDuration(metadata.duration);
  const domain = extractDomain(metadata.webpageUrl);

  return (
    <div className="relative group flex flex-col sm:flex-row items-start sm:items-center gap-4 p-3.5 rounded-xl bg-neutral-950/80 border border-neutral-800/90 backdrop-blur-md shadow-xl transition-all hover:border-neutral-700/80">
      {/* Left: 16:9 responsive thumbnail */}
      <div className="relative aspect-video w-full sm:w-44 shrink-0 rounded-lg overflow-hidden bg-neutral-900 border border-neutral-800/80 flex items-center justify-center">
        {metadata.thumbnail && !imageError ? (
          <img
            src={metadata.thumbnail}
            alt={metadata.title}
            onError={() => setImageError(true)}
            className="w-full h-full object-cover transition-transform duration-300 group-hover:scale-105"
            loading="lazy"
          />
        ) : (
          <div className="flex flex-col items-center justify-center gap-1.5 text-neutral-500">
            <Video className="w-8 h-8 text-neutral-600" />
            <span className="text-[10px] uppercase font-semibold tracking-wider text-neutral-600">
              Preview
            </span>
          </div>
        )}
      </div>

      {/* Right: Media Information */}
      <div className="flex-1 min-w-0 pr-8">
        <h4
          className="text-sm font-semibold text-neutral-100 line-clamp-2 leading-snug group-hover:text-cyan-300 transition-colors"
          title={metadata.title}
        >
          {metadata.title}
        </h4>

        <div className="flex flex-wrap items-center gap-2 mt-2.5">
          {metadata.uploader && (
            <div className="flex items-center gap-1.5 px-2.5 py-1 rounded-md bg-neutral-900/90 border border-neutral-800 text-xs font-medium text-cyan-400">
              <User className="w-3.5 h-3.5 shrink-0" />
              <span className="max-w-[180px] truncate text-neutral-200">
                {metadata.uploader}
              </span>
            </div>
          )}

          {formattedDuration && (
            <div className="flex items-center gap-1.5 px-2.5 py-1 rounded-md bg-neutral-900/90 border border-neutral-800 text-xs font-medium text-purple-400">
              <Clock className="w-3.5 h-3.5 shrink-0" />
              <span className="text-neutral-200">{formattedDuration}</span>
            </div>
          )}

          {domain && (
            <div className="flex items-center gap-1.5 px-2.5 py-1 rounded-md bg-neutral-900/90 border border-neutral-800 text-xs font-medium text-neutral-400">
              <Globe className="w-3.5 h-3.5 shrink-0 text-neutral-500" />
              <span className="text-neutral-300 max-w-[140px] truncate">{domain}</span>
            </div>
          )}
        </div>
      </div>

      {/* Top-Right: Dismiss Close Button */}
      <button
        type="button"
        onClick={onClear}
        className="absolute top-2.5 right-2.5 p-1 rounded-lg text-neutral-400 hover:text-neutral-100 hover:bg-neutral-800/80 transition-colors cursor-pointer"
        title="Dismiss preview"
      >
        <X className="w-4 h-4" />
      </button>
    </div>
  );
};
