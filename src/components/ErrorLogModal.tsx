import React, { useState, useEffect } from "react";
import { Terminal, Copy, Check, X, AlertCircle } from "lucide-react";

interface ErrorLogModalProps {
  isOpen: boolean;
  onClose: () => void;
  title: string;
  url: string;
  errorLog?: string;
}

export const ErrorLogModal: React.FC<ErrorLogModalProps> = ({
  isOpen,
  onClose,
  title,
  url,
  errorLog,
}) => {
  const [copied, setCopied] = useState(false);

  // Close on Escape key press
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape" && isOpen) {
        onClose();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, onClose]);

  if (!isOpen) return null;

  const content = errorLog && errorLog.trim().length > 0
    ? errorLog.trim()
    : "No detailed error output was captured by yt-dlp or the system.";

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(content);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch (err) {
      console.error("Failed to copy error log to clipboard:", err);
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/80 backdrop-blur-sm animate-in fade-in duration-200"
      onClick={onClose}
    >
      <div
        className="relative w-full max-w-2xl bg-neutral-900 border border-neutral-800 rounded-2xl shadow-2xl overflow-hidden flex flex-col max-h-[85vh] animate-in zoom-in-95 duration-200"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-neutral-800/80 bg-neutral-900/90 shrink-0">
          <div className="flex items-center gap-3 min-w-0">
            <div className="flex items-center justify-center w-8 h-8 rounded-lg bg-rose-500/10 border border-rose-500/20 text-rose-400 shrink-0">
              <AlertCircle className="w-4 h-4" />
            </div>
            <div className="min-w-0">
              <h2 className="text-sm font-semibold text-neutral-100 truncate">
                Task Error Log
              </h2>
              <p className="text-xs text-neutral-400 truncate max-w-md" title={title}>
                {title || "Download Task"}
              </p>
            </div>
          </div>

          <button
            type="button"
            onClick={onClose}
            className="p-1.5 text-neutral-400 hover:text-neutral-200 hover:bg-neutral-800 rounded-lg transition-colors cursor-pointer"
            title="Close dialog (Esc)"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* URL Meta Bar */}
        {url && (
          <div className="px-6 py-2 bg-neutral-950/60 border-b border-neutral-800/50 flex items-center gap-2 text-xs text-neutral-400">
            <span className="font-semibold text-neutral-500 shrink-0">URL:</span>
            <span className="font-mono text-neutral-300 truncate" title={url}>
              {url}
            </span>
          </div>
        )}

        {/* Terminal Error Log Box */}
        <div className="flex-1 p-5 overflow-hidden flex flex-col bg-neutral-950 min-h-0">
          <div className="flex items-center justify-between pb-2 text-[11px] font-mono text-neutral-500 border-b border-neutral-800/60 shrink-0">
            <div className="flex items-center gap-1.5">
              <Terminal className="w-3.5 h-3.5 text-neutral-400" />
              <span>stderr output trace</span>
            </div>
            <span>UTF-8</span>
          </div>

          <pre className="flex-1 mt-2.5 overflow-y-auto font-mono text-xs text-rose-300/95 leading-relaxed whitespace-pre-wrap break-all p-3.5 bg-neutral-950 border border-neutral-800/80 rounded-xl select-text selection:bg-rose-950 selection:text-rose-100">
            {content}
          </pre>
        </div>

        {/* Footer with Actions */}
        <div className="flex items-center justify-between px-6 py-3.5 bg-neutral-900 border-t border-neutral-800 shrink-0">
          <button
            type="button"
            onClick={handleCopy}
            className="px-3.5 py-1.5 rounded-xl text-xs font-medium bg-neutral-800 hover:bg-neutral-700/80 text-neutral-200 border border-neutral-700/60 flex items-center gap-2 transition-colors cursor-pointer"
          >
            {copied ? (
              <>
                <Check className="w-3.5 h-3.5 text-emerald-400" />
                <span className="text-emerald-400">Copied to Clipboard!</span>
              </>
            ) : (
              <>
                <Copy className="w-3.5 h-3.5" />
                <span>Copy Logs</span>
              </>
            )}
          </button>

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
