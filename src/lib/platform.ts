/**
 * The words that differ between operating systems.
 *
 * The backend reports which platform it runs on through `get_environment`,
 * and every screen that names a system feature ("Recycle Bin", "File
 * Explorer", "Task Manager") takes the name from here instead of spelling it
 * out, so a Linux user is never told to look in a Windows control panel.
 */

import { useStore } from "@/app/store";
import type { Platform } from "@/lib/types";

export interface PlatformWords {
  platform: Platform;
  isWindows: boolean;
  /** "Windows", "Linux", "macOS". */
  os: string;
  /** "Recycle Bin" or "Trash". */
  trash: string;
  /** "File Explorer", "the file manager", "Finder". */
  fileManager: string;
  /** Label for the button that reveals a file: "Show in Explorer". */
  showInFolder: string;
  /** "Task Manager", "System Monitor", "Activity Monitor". */
  taskManager: string;
  /** Where the OS keeps settings for sign-in programs. */
  startupPlaces: string;
  /** Where installed applications are listed by the OS. */
  appsSource: string;
  /** "Administrator" or "root". */
  admin: string;
  /** Whether AllInsight can restart itself with more rights on this OS. */
  canElevate: boolean;
}

/** A guess for the first frame, before the backend has answered. */
function guessPlatform(): Platform {
  const ua = typeof navigator === "undefined" ? "" : navigator.userAgent;
  if (/Windows/i.test(ua)) return "windows";
  if (/Mac OS X|Macintosh/i.test(ua)) return "macos";
  if (/Linux|X11/i.test(ua)) return "linux";
  return "windows";
}

export function wordsFor(platform: Platform): PlatformWords {
  switch (platform) {
    case "linux":
      return {
        platform,
        isWindows: false,
        os: "Linux",
        trash: "Trash",
        fileManager: "the file manager",
        showInFolder: "Show in folder",
        taskManager: "System Monitor",
        startupPlaces: "the autostart folders your desktop reads",
        appsSource: "the desktop entries your launcher shows, traced to their package",
        admin: "root",
        canElevate: false,
      };
    case "macos":
      return {
        platform,
        isWindows: false,
        os: "macOS",
        trash: "Trash",
        fileManager: "Finder",
        showInFolder: "Show in Finder",
        taskManager: "Activity Monitor",
        startupPlaces: "your login items",
        appsSource: "the Applications folder",
        admin: "administrator",
        canElevate: false,
      };
    default:
      return {
        platform: "windows",
        isWindows: true,
        os: "Windows",
        trash: "Recycle Bin",
        fileManager: "File Explorer",
        showInFolder: "Show in Explorer",
        taskManager: "Task Manager",
        startupPlaces: "the Run registry keys and the Startup folders, the same places Windows reads",
        appsSource: "the same registry entries Windows Settings uses",
        admin: "administrator",
        canElevate: true,
      };
  }
}

export function usePlatformWords(): PlatformWords {
  const { environment } = useStore();
  return wordsFor(environment?.platform ?? guessPlatform());
}
