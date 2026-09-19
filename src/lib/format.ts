/**
 * Formatting helpers.
 *
 * `formatBytes` mirrors the Rust implementation exactly, so a figure in a
 * notification, a log line and the interface always reads the same.
 */

const UNITS = ["B", "KB", "MB", "GB", "TB", "PB"];

export function formatBytes(bytes: number | null | undefined): string {
  if (bytes === null || bytes === undefined || Number.isNaN(bytes)) return "-";
  if (bytes < 1024) return `${Math.round(bytes)} B`;
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  if (value >= 100) return `${value.toFixed(0)} ${UNITS[unit]}`;
  if (value >= 10) return `${value.toFixed(1)} ${UNITS[unit]}`;
  return `${value.toFixed(2)} ${UNITS[unit]}`;
}

/** Compact form for axis labels and dense tables. */
export function formatBytesShort(bytes: number | null | undefined): string {
  if (bytes === null || bytes === undefined) return "-";
  if (bytes < 1024) return `${Math.round(bytes)}B`;
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value >= 10 ? value.toFixed(0) : value.toFixed(1)}${UNITS[unit]}`;
}

export function formatRate(bytesPerSecond: number): string {
  return `${formatBytesShort(bytesPerSecond)}/s`;
}

export function formatPercent(value: number | null | undefined, digits = 0): string {
  if (value === null || value === undefined || Number.isNaN(value)) return "-";
  return `${value.toFixed(digits)}%`;
}

export function formatCount(value: number | null | undefined): string {
  if (value === null || value === undefined) return "-";
  return value.toLocaleString();
}

/** A Unix second timestamp as a short local date. */
export function formatDate(unixSeconds: number | null | undefined): string {
  if (!unixSeconds) return "-";
  return new Date(unixSeconds * 1000).toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

export function formatDateTime(unixSeconds: number | null | undefined): string {
  if (!unixSeconds) return "-";
  return new Date(unixSeconds * 1000).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

/** "3 days ago", for history lists where the exact minute does not matter. */
export function formatRelative(unixSeconds: number | null | undefined): string {
  if (!unixSeconds) return "-";
  const seconds = Math.floor(Date.now() / 1000) - unixSeconds;
  if (seconds < 60) return "just now";
  if (seconds < 3600) return `${Math.floor(seconds / 60)} min ago`;
  if (seconds < 86_400) return `${Math.floor(seconds / 3600)} h ago`;
  if (seconds < 604_800) return `${Math.floor(seconds / 86_400)} d ago`;
  return formatDate(unixSeconds);
}

export function formatDuration(seconds: number | null | undefined): string {
  if (seconds === null || seconds === undefined) return "-";
  if (seconds < 60) return `${Math.round(seconds)}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ${minutes % 60}m`;
  return `${Math.floor(hours / 24)}d ${hours % 24}h`;
}

export function formatMilliseconds(ms: number): string {
  if (ms < 1000) return `${ms} ms`;
  return `${(ms / 1000).toFixed(1)} s`;
}

/** Shorten a path for a narrow column, keeping the ends that identify it. */
export function shortenPath(path: string, maxLength = 52): string {
  if (path.length <= maxLength) return path;
  const parts = path.split("\\");
  if (parts.length <= 2) return `...${path.slice(-(maxLength - 3))}`;
  const head = parts[0];
  const tail = parts.slice(-2).join("\\");
  const shortened = `${head}\\...\\${tail}`;
  return shortened.length <= maxLength
    ? shortened
    : `...${tail.slice(-(maxLength - 3))}`;
}

export function parentDirectory(path: string): string {
  const index = path.lastIndexOf("\\");
  return index > 2 ? path.slice(0, index) : path;
}
