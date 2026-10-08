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
import { UpdatePrompt } from "./Updates";
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
      data-route={route.id}
      onClick={onSelect}
      aria-current={active ? "page" : undefined}
      className={cn(
        "group relative z-10 flex w-full items-center gap-2.5 rounded-md px-2.5 py-[6px] text-left text-[13px] transition-quick",
        active
          ? "font-medium text-[var(--color-ink)]"
          : "text-[var(--color-ink-muted)] hover:bg-[var(--color-selection)] hover:text-[var(--color-ink)]",
      )}
    >
      <Icon
        className={cn(
          "size-4 shrink-0",
          active ? "text-[var(--color-accent)]" : "text-[var(--color-ink-subtle)] group-hover:text-[var(--color-ink-muted)]",
        )}
      />
      <span className="truncate">{route.label}</span>
    </button>
  );
}

function Sidebar({ onOpenPalette }: { onOpenPalette: () => void }) {
  const { route, navigate, environment } = useStore();
  const listRef = React.useRef<HTMLDivElement>(null);
  const [plate, setPlate] = React.useState<{ top: number; height: number; animate: boolean } | null>(
    null,
  );

  React.useLayoutEffect(() => {
    const list = listRef.current;
    const item = list?.querySelector<HTMLElement>(`[data-route="${route}"]`);
    if (!list || !item) {
      setPlate(null);
      return;
    }
    const top = item.getBoundingClientRect().top - list.getBoundingClientRect().top + list.scrollTop;
    // The first placement is instant; every later one glides.
    setPlate((prev) => ({ top, height: item.offsetHeight, animate: prev !== null }));
  }, [route]);

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
      className="vibrancy flex w-[228px] shrink-0 flex-col border-r border-[var(--color-line)]"
    >
      <div className="flex items-center gap-2.5 px-5 pb-4 pt-5">
        <Logo size={26} />
        <Wordmark />
      </div>

      <button
        onClick={onOpenPalette}
        className="mx-3 mb-5 flex items-center gap-2 rounded-md border border-[var(--color-line)] bg-[var(--color-surface)] px-2.5 py-1.5 text-xs text-[var(--color-ink-subtle)] transition-quick hover:border-[var(--color-line-strong)] hover:text-[var(--color-ink-muted)]"
      >
        <Search className="size-3.5" />
        <span className="flex-1 text-left">Search</span>
        <kbd className="font-sans text-[11px] opacity-70">
          {environment?.platform === "macos" ? "⌘K" : "Ctrl K"}
        </kbd>
      </button>

      <div ref={listRef} className="relative flex-1 space-y-5 overflow-y-auto px-3 pb-3">
        {/* One selection plate that glides to the chosen item rather than
            jumping, like a source list on macOS. */}
        <div
          aria-hidden
          className="pointer-events-none absolute left-3 right-3 top-0 rounded-md bg-[var(--color-surface)] shadow-[0_0_0_1px_var(--color-line),0_1px_2px_rgba(23,24,26,0.04)]"
          style={{
            height: plate?.height ?? 0,
            translate: `0 ${plate?.top ?? 0}px`,
            opacity: plate ? 1 : 0,
            transition: plate?.animate
              ? "translate 380ms var(--ease-out), height 380ms var(--ease-out), opacity 200ms"
              : "none",
          }}
        />
        {groups.map(({ group, label, routes }) => (
          <div key={group}>
            {label ? (
              <p className="px-2.5 pb-1.5 text-[10.5px] font-medium uppercase tracking-[0.08em] text-[var(--color-ink-subtle)]">
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

      <div className="space-y-2 px-5 py-4">
        <div className="flex items-center gap-1.5 text-2xs text-[var(--color-ink-subtle)]">
          <WifiOff className="size-3" />
          Offline mode - all local features available
        </div>
        <div className="flex items-center justify-between">
          <Badge tone={environment?.elevated ? "accent" : "neutral"} dot>
            {environment?.elevated
              ? environment.platform === "windows"
                ? "Administrator"
                : "root"
              : "Standard user"}
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
    <div className="vibrancy border-b border-[var(--color-line)] px-10 py-2">
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
            "pointer-events-auto toast-enter panel-raised rounded-2xl border p-3.5",
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
          <div className="mx-auto w-full max-w-[1180px] px-12 py-10">{children}</div>
        </main>
      </div>

      <CommandPalette open={paletteOpen} onClose={() => setPaletteOpen(false)} />
      <Toasts />
      <UpdatePrompt />

      {/* A quiet affordance so a long scan can always be stopped, wherever the
          user has navigated to. */}
      {scanning ? (
        <div className="fixed bottom-4 left-[244px] z-40">
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
