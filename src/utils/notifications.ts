import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import { playSuccessSound, playErrorSound } from "./sound";

/**
 * Ensures native Windows notification permissions are checked and requested if needed.
 */
export async function ensureNotificationPermission(): Promise<boolean> {
  try {
    let granted = await isPermissionGranted();
    if (!granted) {
      const permission = await requestPermission();
      granted = permission === "granted";
    }
    return granted;
  } catch (err) {
    console.warn("Unable to verify notification permissions:", err);
    return false;
  }
}

/**
 * Alerts the user of a completed download via audio chime and native Windows toast notification.
 */
export async function notifyTaskComplete(
  title: string,
  isPlaylist?: boolean
): Promise<void> {
  playSuccessSound();

  try {
    const hasPermission = await ensureNotificationPermission();
    if (hasPermission) {
      sendNotification({
        title: "Download Complete 🎬",
        body: isPlaylist ? "Playlist downloaded successfully" : title,
      });
    }
  } catch (err) {
    console.warn("Failed to dispatch completion notification:", err);
  }
}

/**
 * Alerts the user of a failed download via error sound and native Windows toast notification.
 */
export async function notifyTaskFailed(
  title: string,
  error?: string
): Promise<void> {
  playErrorSound();

  try {
    const hasPermission = await ensureNotificationPermission();
    if (hasPermission) {
      sendNotification({
        title: "Download Failed ⚠️",
        body: error || title,
      });
    }
  } catch (err) {
    console.warn("Failed to dispatch failure notification:", err);
  }
}
