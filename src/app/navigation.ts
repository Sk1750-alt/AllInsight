/**
 * The navigation model.
 *
 * One flat list of routes grouped for display. Insight actions from the
 * backend map onto these ids, which is how a recommendation becomes a button
 * without the backend ever knowing anything about the interface.
 */
import type { InsightAction } from "@/lib/types";

export type RouteId =
  | "overview"
  | "storage-map"
  | "large-files"
  | "duplicates"
  | "cleanup"
  | "performance"
  | "processes"
  | "startup"
  | "applications"
  | "battery"
  | "drive-health"
  | "assistant"
  | "activity"
  | "settings";

export interface Route {
  id: RouteId;
  label: string;
  group: "main" | "storage" | "system" | "tools";
  /** Words the command palette matches against, beyond the label. */
  keywords: string[];
}

export const ROUTES: Route[] = [
  { id: "overview", label: "Overview", group: "main", keywords: ["dashboard", "home", "health", "score"] },

  { id: "storage-map", label: "Storage Map", group: "storage", keywords: ["treemap", "folders", "space", "usage"] },
  { id: "large-files", label: "Large Files", group: "storage", keywords: ["big", "size", "biggest"] },
  { id: "duplicates", label: "Duplicates", group: "storage", keywords: ["copies", "identical", "hash"] },
  { id: "cleanup", label: "Cleanup", group: "storage", keywords: ["clean", "temp", "cache", "reclaim", "free space"] },

  { id: "performance", label: "Performance", group: "system", keywords: ["cpu", "memory", "ram", "gpu", "network", "graphs"] },
  { id: "processes", label: "Processes", group: "system", keywords: ["tasks", "apps running", "end task", "task manager"] },
  { id: "startup", label: "Startup", group: "system", keywords: ["boot", "sign in", "autostart", "login"] },
  { id: "applications", label: "Applications", group: "system", keywords: ["installed", "programs", "uninstall"] },
  { id: "battery", label: "Battery", group: "system", keywords: ["power", "charge", "capacity", "cycles"] },

  { id: "drive-health", label: "Drive Health", group: "tools", keywords: ["smart", "nvme", "ssd", "wear", "temperature"] },
  { id: "assistant", label: "AllInsight AI", group: "tools", keywords: ["ai", "assistant", "ask", "model", "gguf", "llama"] },
  { id: "activity", label: "Activity", group: "tools", keywords: ["history", "log", "cleanup history", "events"] },
  { id: "settings", label: "Settings", group: "tools", keywords: ["preferences", "options", "privacy", "protected", "theme"] },
];

export const GROUP_LABELS: Record<Route["group"], string | null> = {
  main: null,
  storage: "Storage",
  system: "System",
  tools: null,
};

/** Map a backend insight action onto a screen. */
export function routeForAction(action: InsightAction): RouteId | null {
  switch (action) {
    case "open_cleanup":
      return "cleanup";
    case "open_large_files":
      return "large-files";
    case "open_duplicates":
      return "duplicates";
    case "open_storage_map":
      return "storage-map";
    case "open_drive_health":
      return "drive-health";
    case "open_startup":
      return "startup";
    case "open_processes":
      return "processes";
    case "open_battery":
      return "battery";
    case "run_scan":
      return "storage-map";
    default:
      return null;
  }
}
