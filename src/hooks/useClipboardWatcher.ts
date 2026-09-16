import { useEffect, useRef, useCallback } from "react";
import { readText } from "@tauri-apps/plugin-clipboard-manager";
import { getCurrentWindow } from "@tauri-apps/api/window";

export const SUPPORTED_MEDIA_DOMAINS = [
  "youtube.com",
  "youtu.be",
  "tiktok.com",
  "facebook.com",
  "fb.watch",
  "x.com",
  "twitter.com",
  "instagram.com",
  "vimeo.com",
  "rumble.com",
  "soundcloud.com",
  "reddit.com",
  "twitch.tv",
  "bilibili.com",
  "dailymotion.com",
];

/**
 * Validates whether a given text string is a valid URL targeting a supported media platform.
 */
export function isValidMediaUrl(text: string): boolean {
  const trimmed = text.trim();
  if (trimmed.length < 11) return false;

  try {
    const parsed = new URL(trimmed);
    if (parsed.protocol !== "http:" && parsed.protocol !== "https:") return false;

    const hostname = parsed.hostname.toLowerCase().replace(/^www\./, "");
    return SUPPORTED_MEDIA_DOMAINS.some(
      (domain) => hostname === domain || hostname.endsWith(`.${domain}`)
    );
  } catch {
    return false;
  }
}

/**
 * Reads text from clipboard using Tauri clipboard-manager with fallback to browser navigator.clipboard.
 * Fails silently without noisy console errors if permission is denied.
 */
async function getClipboardText(): Promise<string | null> {
  try {
    const text = await readText();
    if (text) return text.trim();
  } catch {
    // Fallback to navigator.clipboard if Tauri plugin fails
    try {
      if (typeof navigator !== "undefined" && navigator.clipboard?.readText) {
        const text = await navigator.clipboard.readText();
        if (text) return text.trim();
      }
    } catch {
      // Silently handle clipboard permission or availability rejections
    }
  }
  return null;
}

/**
 * Hook to automatically detect media URLs when the app window gains focus.
 *
 * @param onUrlDetected Callback invoked when a new, valid media URL is detected
 * @param enabled Whether clipboard watching is enabled (defaults to true)
 * @param inputRef Optional ref to the input element to prevent overwriting during active typing
 */
export function useClipboardWatcher(
  onUrlDetected: (url: string) => void,
  enabled: boolean = true,
  inputRef?: React.RefObject<HTMLInputElement | null>
) {
  const lastDetectedUrlRef = useRef<string>("");
  const onUrlDetectedRef = useRef(onUrlDetected);

  useEffect(() => {
    onUrlDetectedRef.current = onUrlDetected;
  }, [onUrlDetected]);

  const checkClipboard = useCallback(async () => {
    if (!enabled) return;

    // Safety: never overwrite if the user is currently typing in the input field
    if (
      inputRef?.current &&
      document.activeElement === inputRef.current
    ) {
      return;
    }

    const text = await getClipboardText();
    if (!text) return;

    if (isValidMediaUrl(text) && text !== lastDetectedUrlRef.current) {
      lastDetectedUrlRef.current = text;
      onUrlDetectedRef.current(text);
    }
  }, [enabled, inputRef]);

  useEffect(() => {
    if (!enabled) return;

    let unlistenTauriFocus: (() => void) | undefined;

    // 1. Browser window focus listener
    window.addEventListener("focus", checkClipboard);

    // 2. Tauri native window focus listener (for reliable Alt+Tab / OS taskbar switches)
    try {
      const appWindow = getCurrentWindow();
      appWindow
        .onFocusChanged(({ payload: focused }) => {
          if (focused) {
            checkClipboard();
          }
        })
        .then((unlisten) => {
          unlistenTauriFocus = unlisten;
        })
        .catch(() => {
          // Fallback gracefully if running in standard browser context
        });
    } catch {
      // Silent catch for test or non-tauri contexts
    }

    return () => {
      window.removeEventListener("focus", checkClipboard);
      if (unlistenTauriFocus) {
        unlistenTauriFocus();
      }
    };
  }, [enabled, checkClipboard]);

  return {
    checkClipboard,
    setLastDetectedUrl: (url: string) => {
      lastDetectedUrlRef.current = url;
    },
  };
}
