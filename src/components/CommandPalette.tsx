/**
 * The command palette.
 *
 * Searches screens and settings always, and searches installed applications,
 * the results of the last large-file scan, and the current recommendations
 * once they have been loaded. It never searches the disk: doing so from a text
 * box would mean maintaining an index of the user's files, which is precisely
 * what AllInsight does not build.
 */
import * as React from "react";
import {
  Activity as ActivityIcon,
  BatteryMedium,
  Blocks,
  Brain,
  CircleGauge,
  Copy,
  Cpu,
  Files,
  HardDrive,
  LayoutDashboard,
  Lightbulb,
  ListTree,
  Power,
  Search,
  Settings as SettingsIcon,
  Trash2,
} from "lucide-react";

import { api } from "@/lib/api";
import { useStore } from "@/app/store";
import { ROUTES, routeForAction, type RouteId } from "@/app/navigation";
import { cn } from "@/lib/utils";
import { formatBytes, shortenPath } from "@/lib/format";

export const ROUTE_ICONS: Record<RouteId, React.ComponentType<{ className?: string }>> = {
  overview: LayoutDashboard,
  "storage-map": ListTree,
  "large-files": Files,
  duplicates: Copy,
  cleanup: Trash2,
  performance: CircleGauge,
  processes: Cpu,
  startup: Power,
  applications: Blocks,
  battery: BatteryMedium,
  "drive-health": HardDrive,
  assistant: Brain,
  activity: ActivityIcon,
  settings: SettingsIcon,
};

interface Result {
  key: string;
  group: "Screens" | "Recommendations" | "Applications" | "Files from the last scan";
  icon: React.ComponentType<{ className?: string }>;
  label: string;
  detail: string;
  onChoose: () => void;
}

export function CommandPalette({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { navigate, reportError } = useStore();
  const [query, setQuery] = React.useState("");
  const [highlighted, setHighlighted] = React.useState(0);
  const inputRef = React.useRef<HTMLInputElement>(null);

  // Loaded once per opening, so the palette never blocks on a fetch and never
  // holds this data while it is closed.
  const [extras, setExtras] = React.useState<Result[]>([]);

  React.useEffect(() => {
    if (!open) return;
    setQuery("");
    setHighlighted(0);
    requestAnimationFrame(() => inputRef.current?.focus());

    let cancelled = false;
    (async () => {
      const collected: Result[] = [];

      const [insights, apps, files] = await Promise.allSettled([
        api.getInsights(),
        api.getInstalledApplications(false),
        api.getLargeFiles(),
      ]);

      if (insights.status === "fulfilled") {
        for (const insight of insights.value) {
          const route = routeForAction(insight.action);
          collected.push({
            key: `insight:${insight.id}`,
            group: "Recommendations",
            icon: Lightbulb,
            label: insight.title,
            detail: insight.value ?? insight.severity,
            onChoose: () => {
              if (route) navigate(route);
              onClose();
            },
          });
        }
      }

      if (apps.status === "fulfilled") {
        for (const app of apps.value.apps.slice(0, 400)) {
          collected.push({
            key: `app:${app.id}`,
            group: "Applications",
            icon: Blocks,
            label: app.name,
            detail: app.publisher ?? "Unknown publisher",
            onChoose: () => {
              navigate("applications");
              onClose();
            },
          });
        }
      }

      if (files.status === "fulfilled" && files.value) {
        for (const file of files.value.entries.slice(0, 400)) {
          collected.push({
            key: `file:${file.path}`,
            group: "Files from the last scan",
            icon: Files,
            label: file.name,
            detail: `${formatBytes(file.size_bytes)} · ${shortenPath(file.directory, 40)}`,
            onChoose: () => {
              api.showInExplorer(file.path).catch((e) => reportError(e));
              onClose();
            },
          });
        }
      }

      if (!cancelled) setExtras(collected);
    })();

    return () => {
      cancelled = true;
    };
  }, [open, navigate, onClose, reportError]);

  const results = React.useMemo(() => {
    const q = query.trim().toLowerCase();

    const screens: Result[] = ROUTES.filter(
      (r) => !q || r.label.toLowerCase().includes(q) || r.keywords.some((k) => k.includes(q)),
    ).map((r) => ({
      key: `route:${r.id}`,
      group: "Screens" as const,
      icon: ROUTE_ICONS[r.id],
      label: r.label,
      detail: r.keywords.slice(0, 3).join(" · "),
      onChoose: () => {
        navigate(r.id);
        onClose();
      },
    }));

    if (!q) return screens;

    const matched = extras.filter(
      (r) => r.label.toLowerCase().includes(q) || r.detail.toLowerCase().includes(q),
    );

    // Screens first, then everything else, capped so the list stays scannable.
    return [...screens, ...matched].slice(0, 40);
  }, [query, extras, navigate, onClose]);

  React.useEffect(() => {
    setHighlighted((h) => Math.min(h, Math.max(0, results.length - 1)));
  }, [results.length]);

  if (!open) return null;

  let lastGroup: string | null = null;

  return (
    <div
      className="dialog-overlay fixed inset-0 z-50 flex items-start justify-center bg-[rgba(23,24,26,0.22)] pt-[14vh] backdrop-blur-[6px]"
      onClick={onClose}
    >
      <div
        className="dialog-enter w-full max-w-xl overflow-hidden rounded-2xl border border-[var(--color-line)] bg-[var(--color-surface-raised)] shadow-[var(--shadow-float)]"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-label="Search AllInsight"
      >
        <div className="flex items-center gap-2 border-b border-[var(--color-line)] px-3">
          <Search className="size-4 text-[var(--color-ink-subtle)]" />
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") onClose();
              if (e.key === "ArrowDown") {
                e.preventDefault();
                setHighlighted((h) => Math.min(h + 1, results.length - 1));
              }
              if (e.key === "ArrowUp") {
                e.preventDefault();
                setHighlighted((h) => Math.max(h - 1, 0));
              }
              if (e.key === "Enter") results[highlighted]?.onChoose();
            }}
            placeholder="Search screens, settings, applications and recommendations"
            className="h-11 flex-1 bg-transparent text-sm text-[var(--color-ink)] outline-none placeholder:text-[var(--color-ink-subtle)]"
          />
          <kbd className="rounded border border-[var(--color-line)] px-1.5 py-0.5 font-mono text-[10px] text-[var(--color-ink-subtle)]">
            Esc
          </kbd>
        </div>

        <div className="max-h-80 overflow-y-auto p-1.5">
          {results.length === 0 ? (
            <p className="px-3 py-6 text-center text-xs text-[var(--color-ink-muted)]">
              Nothing matched. AllInsight searches screens, settings, installed applications and the
              results of your last scan. It does not index the contents of your files.
            </p>
          ) : (
            results.map((result, index) => {
              const Icon = result.icon;
              const showGroup = result.group !== lastGroup;
              lastGroup = result.group;
              return (
                <React.Fragment key={result.key}>
                  {showGroup ? (
                    <p className="px-2.5 pb-1 pt-2 text-2xs font-semibold uppercase tracking-wider text-[var(--color-ink-subtle)]">
                      {result.group}
                    </p>
                  ) : null}
                  <button
                    onMouseEnter={() => setHighlighted(index)}
                    onClick={result.onChoose}
                    className={cn(
                      "flex w-full items-center gap-2.5 rounded px-2.5 py-2 text-left text-xs transition-quick",
                      index === highlighted
                        ? "bg-[var(--color-surface-hover)] text-[var(--color-ink)]"
                        : "text-[var(--color-ink-muted)]",
                    )}
                  >
                    <Icon className="size-4 shrink-0 text-[var(--color-ink-subtle)]" />
                    <span className="truncate font-medium">{result.label}</span>
                    <span className="ml-auto truncate pl-3 text-2xs text-[var(--color-ink-subtle)]">
                      {result.detail}
                    </span>
                  </button>
                </React.Fragment>
              );
            })
          )}
        </div>
      </div>
    </div>
  );
}
