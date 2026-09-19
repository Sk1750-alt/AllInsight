/**
 * The application frame: sidebar, status strip, toast stack and the command
 * palette. Screens render into the scrolling region on the right.
 */
import * as React from "react";
import { Search, Square, WifiOff, X } from "lucide-react";

import { api } from "@/lib/api";
import { useStore } from "@/app/store";
import { GROUP_LABELS, ROUTES, type Route } from "@/app/navigation";
import { CommandPalette, ROUTE_ICONS } from "./CommandPalette";
import { EmergencyBanner } from "./EmergencyBanner";
import { Logo, Wordmark } from "./Logo";
import { Badge, IconButton } from "./ui/primitives";
import { IndeterminateBar } from "./ui/data";
import { Tooltip } from "./ui/overlay";
import { cn } from "@/lib/utils";
import { formatBytes, formatCount, shortenPath } from "@/lib/format";

const ICONS = ROUTE_ICONS;

function NavItem({ route, active, onSelect }: { route: Route; active: boolean; onSelect: () => void }) {
  const Icon = ICONS[route.id];
  return (
    <button
      onClick={onSelect}
      aria-current={active ? "page" : undefined}
      className={cn(
        "group flex w-full items-center gap-2.5 rounded-md px-2.5 py-[7px] text-left text-xs transition-quick",
        active
          ? "bg-[var(--color-surface-hover)] font-medium text-[var(--color-ink)]"
          : "text-[var(--color-ink-muted)] hover:bg-[var(--color-surface-hover)] hover:text-[var(--color-ink)]",
      )}
    >
      <span
        className={cn(
          "absolute left-0 h-4 w-[2px] rounded-r-full transition-quick",
          active ? "bg-[var(--color-accent)]" : "bg-transparent",
        )}
        style={{ position: "relative", left: -6 }}
        aria-hidden
      />
      <Icon
        className={cn(
          "size-4 shrink-0",
          active ? "text-[var(--color-accent)]" : "text-[var(--color-ink-subtle)]",
        )}
      />
      <span className="truncate">{route.label}</span>
    </button>
  );
}

function Sidebar({ onOpenPalette }: { onOpenPalette: () => void }) {
  const { route, navigate, environment } = useStore();

  const groups = React.useMemo(() => {
    const order: Route["group"][] = ["main", "storage", "system", "tools"];
    return order.map((group) => ({
      group,
      label: GROUP_LABELS[group],
      routes: ROUTES.filter((r) => r.group === group),
    }));
  }, []);

  return (
    <nav
      aria-label="Main"
      className="flex w-[212px] shrink-0 flex-col border-r border-[var(--color-line)] bg-[var(--color-surface)]"
    >
      <div className="flex items-center gap-2.5 px-4 py-4">
        <Logo size={26} />
        <Wordmark />
      </div>

      <button
        onClick={onOpenPalette}
        className="mx-3 mb-3 flex items-center gap-2 rounded-md border border-[var(--color-line)] bg-[var(--color-canvas)] px-2.5 py-1.5 text-2xs text-[var(--color-ink-subtle)] transition-quick hover:border-[var(--color-line-strong)] hover:text-[var(--color-ink-muted)]"
      >
        <Search className="size-3.5" />
        <span className="flex-1 text-left">Search</span>
        <kbd className="rounded border border-[var(--color-line)] px-1 font-mono text-[10px]">
          Ctrl K
        </kbd>
      </button>

      <div className="flex-1 space-y-4 overflow-y-auto px-3 pb-3">
        {groups.map(({ group, label, routes }) => (
          <div key={group}>
            {label ? (
              <p className="px-2.5 pb-1.5 text-2xs font-semibold uppercase tracking-wider text-[var(--color-ink-subtle)]">
                {label}
              </p>
            ) : null}
            <div className="space-y-0.5">
              {routes.map((r) => (
                <NavItem
                  key={r.id}
                  route={r}
                  active={route === r.id}
                  onSelect={() => navigate(r.id)}
                />
              ))}
            </div>
          </div>
        ))}
      </div>

      <div className="space-y-2 border-t border-[var(--color-line)] px-3 py-3">
        <div className="flex items-center gap-1.5 text-2xs text-[var(--color-ink-subtle)]">
          <WifiOff className="size-3" />
          Offline mode - all local features available
        </div>
        <div className="flex items-center justify-between">
          <Badge tone={environment?.elevated ? "accent" : "neutral"} dot>
            {environment?.elevated ? "Administrator" : "Standard user"}
          </Badge>
          <span className="text-2xs text-[var(--color-ink-subtle)]">
            v{environment?.app_version ?? "1.0.0"}
          </span>
        </div>
      </div>
    </nav>
  );
}

/** The live strip under the title bar, visible only while work is running. */
function ScanStrip() {
  const { scanning, scanProgress } = useStore();
  if (!scanning || !scanProgress) return null;

  return (
    <div className="border-b border-[var(--color-line)] bg-[var(--color-surface)] px-6 py-2">
      <div className="flex items-center justify-between gap-4 text-2xs text-[var(--color-ink-muted)]">
        <span className="truncate">
          Scanning {scanProgress.current ? shortenPath(scanProgress.current, 60) : "..."}
        </span>
        <span className="numeric shrink-0">
          {formatBytes(scanProgress.bytes)} analysed · {formatCount(scanProgress.files)} files
          {scanProgress.errors > 0 ? ` · ${formatCount(scanProgress.errors)} skipped` : ""}
        </span>
      </div>
      <IndeterminateBar className="mt-1.5" />
    </div>
  );
}

function Toasts() {
  const { toasts, dismissToast } = useStore();
  if (toasts.length === 0) return null;

  const tones = {
    info: "border-[var(--color-line-strong)]",
    success: "border-[color-mix(in_srgb,var(--color-ok)_45%,transparent)]",
    warning: "border-[color-mix(in_srgb,var(--color-warn)_45%,transparent)]",
    error: "border-[color-mix(in_srgb,var(--color-danger)_45%,transparent)]",
  } as const;

  return (
    <div className="pointer-events-none fixed bottom-4 right-4 z-50 flex w-80 flex-col gap-2">
      {toasts.map((t) => (
        <div
          key={t.id}
          role="status"
          className={cn(
            "pointer-events-auto view-enter rounded-md border bg-[var(--color-surface-raised)] p-3 shadow-xl",
            tones[t.tone],
          )}
        >
          <div className="flex items-start justify-between gap-3">
            <div className="min-w-0">
              <p className="text-xs font-medium text-[var(--color-ink)]">{t.title}</p>
              {t.body ? (
                <p className="mt-0.5 text-xs text-[var(--color-ink-muted)]">{t.body}</p>
              ) : null}
            </div>
            <button
              aria-label="Dismiss"
              onClick={() => dismissToast(t.id)}
              className="shrink-0 rounded p-0.5 text-[var(--color-ink-subtle)] transition-quick hover:text-[var(--color-ink)]"
            >
              <X className="size-3.5" />
            </button>
          </div>
        </div>
      ))}
    </div>
  );
}

export function AppShell({ children }: { children: React.ReactNode }) {
  const { scanning } = useStore();
  const [paletteOpen, setPaletteOpen] = React.useState(false);

  React.useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setPaletteOpen((open) => !open);
      }
      if (event.key === "Escape") setPaletteOpen(false);
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  return (
    <div className="flex h-full bg-[var(--color-canvas)]">
      <Sidebar onOpenPalette={() => setPaletteOpen(true)} />

      <div className="flex min-w-0 flex-1 flex-col">
        <EmergencyBanner />
        <ScanStrip />
        <main className="flex-1 overflow-y-auto">
          <div className="mx-auto w-full max-w-[1400px] px-6 py-6">{children}</div>
        </main>
      </div>

      <CommandPalette open={paletteOpen} onClose={() => setPaletteOpen(false)} />
      <Toasts />

      {/* A quiet affordance so a long scan can always be stopped, wherever the
          user has navigated to. */}
      {scanning ? (
        <div className="fixed bottom-4 left-[228px] z-40">
          <Tooltip content="Stop the running scan">
            <span>
              <IconButton
                label="Stop scan"
                variant="secondary"
                size="sm"
                onClick={() => api.cancelScan()}
                icon={<Square className="size-3" />}
              />
            </span>
          </Tooltip>
        </div>
      ) : null}
    </div>
  );
}
