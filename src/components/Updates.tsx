/**
 * Settings → Updates, and the prompt shown when an automatic check finds a
 * new version.
 *
 * Everything here is driven by the backend's update state; the interface
 * never decides on its own that an update is available, verified or safe. It
 * also never fetches anything itself: the content security policy forbids
 * it, and every network request lives in services::update.
 */
import * as React from "react";
import {
  CircleAlert,
  CircleArrowUp,
  CircleCheck,
  Download,
  PackageCheck,
  RefreshCw,
  RotateCw,
  ShieldAlert,
  ShieldCheck,
  WifiOff,
} from "lucide-react";

import { api, onUpdate } from "@/lib/api";
import { useStore } from "@/app/store";
import { Badge, Button, Panel, PanelHeader, Skeleton, Spinner } from "@/components/ui/primitives";
import { ProgressBar } from "@/components/ui/data";
import { Dialog } from "@/components/ui/overlay";
import { formatBytes, formatRelative } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { ReleaseInfo, UpdateView } from "@/lib/types";

/**
 * A Settings section to open on the next visit. The prompt navigates to
 * Settings and the view reads this when it mounts; an event would be missed
 * because the view is not listening yet.
 */
let requestedSection: string | null = null;

export function takeRequestedSettingsSection(): string | null {
  const section = requestedSection;
  requestedSection = null;
  return section;
}

/** The live update state, kept current by backend events. */
function useUpdateState() {
  const [view, setView] = React.useState<UpdateView | null>(null);
  React.useEffect(() => {
    let alive = true;
    api
      .getUpdateStatus()
      .then((v) => alive && setView(v))
      .catch(() => {});
    const unlisten = onUpdate((v) => alive && setView(v));
    return () => {
      alive = false;
      unlisten.then((f) => f());
    };
  }, []);
  return [view, setView] as const;
}

/** Run one updater action, showing its result even if the event was missed. */
function useUpdateActions(setView: (v: UpdateView) => void) {
  const { reportError } = useStore();
  const [busy, setBusy] = React.useState(false);
  const run = React.useCallback(
    async (action: () => Promise<UpdateView>) => {
      setBusy(true);
      try {
        setView(await action());
      } catch (e) {
        reportError(e, "The update could not continue.");
      } finally {
        setBusy(false);
      }
    },
    [reportError, setView],
  );
  return {
    busy,
    check: () => run(api.checkForUpdates),
    download: () => run(api.downloadUpdate),
    install: () => run(api.installUpdate),
    later: () => run(api.dismissUpdate),
  };
}

// ------------------------------------------------------------------ panel

export function UpdatesPanel() {
  const { settings, saveSettings } = useStore();
  const [view, setView] = useUpdateState();
  const actions = useUpdateActions(setView);

  if (!view || !settings) {
    return <Skeleton className="h-80" />;
  }

  const setAuto = (auto: boolean) => {
    saveSettings({ ...settings, update_auto_check: auto })
      .then(() => setView({ ...view, auto_check: auto }))
      .catch(() => {});
  };

  const unavailable = view.phase.state === "unavailable";

  return (
    <>
      <Panel>
        <div className="flex flex-wrap items-start justify-between gap-6 p-6">
          <div>
            <p className="text-2xs font-medium uppercase tracking-[0.08em] text-[var(--color-ink-subtle)]">
              Current version
            </p>
            <p className="numeric mt-1 font-display text-3xl font-semibold tracking-[-0.01em] text-[var(--color-ink)]">
              v{view.current_version}
            </p>
            <div className="mt-2 flex items-center gap-2 text-2xs text-[var(--color-ink-muted)]">
              <Badge tone="neutral">Stable</Badge>
              <span>
                {view.last_checked
                  ? `Last checked ${formatRelative(view.last_checked)}`
                  : "Not checked yet"}
              </span>
            </div>
          </div>
          {!unavailable ? (
            <Button
              variant="primary"
              icon={<RefreshCw className={cn("size-3.5", view.phase.state === "checking" && "animate-spin")} />}
              disabled={
                actions.busy ||
                ["checking", "downloading", "verifying"].includes(view.phase.state)
              }
              onClick={actions.check}
            >
              {view.phase.state === "checking" ? "Checking…" : "Check for Updates"}
            </Button>
          ) : null}
        </div>

        <div className="border-t border-[var(--color-line)] px-6 py-5">
          <UpdateStatusBlock view={view} actions={actions} />
        </div>
      </Panel>

      {!unavailable ? (
        <Panel>
          <PanelHeader
            title="Update preference"
            description="AllInsight only goes online for updates if you allow it."
          />
          <div role="radiogroup" aria-label="Update preference" className="grid gap-2 p-4 sm:grid-cols-2">
            <PreferenceOption
              selected={view.auto_check}
              onSelect={() => setAuto(true)}
              title="Automatically check for updates"
              description={`About once every ${view.check_interval_hours === 24 ? "day" : `${view.check_interval_hours} hours`}. AllInsight tells you what it found and installs nothing on its own.`}
            />
            <PreferenceOption
              selected={!view.auto_check}
              onSelect={() => setAuto(false)}
              title="Ask me before checking"
              description="AllInsight contacts the update server only when you click Check for Updates."
              badge="Default"
            />
          </div>
        </Panel>
      ) : null}

      <Panel className="border-[color-mix(in_srgb,var(--color-accent)_35%,transparent)]">
        <div className="flex items-start gap-3 p-5">
          <ShieldCheck className="mt-0.5 size-5 shrink-0 text-[var(--color-accent)]" />
          <div className="space-y-1.5">
            <p className="text-sm font-semibold text-[var(--color-ink)]">Privacy</p>
            <p className="text-xs leading-relaxed text-[var(--color-ink-muted)]">
              AllInsight processes your data locally. Your analytics, documents, datasets and
              personal data are never uploaded as part of the update process.
            </p>
            <p className="text-2xs leading-relaxed text-[var(--color-ink-subtle)]">
              A check downloads one public file that lists the latest release, the same file for
              everyone. It carries no account, no ID and nothing about this device. Every update is
              verified against AllInsight's signing key before it can be installed.
            </p>
          </div>
        </div>
      </Panel>
    </>
  );
}

function PreferenceOption({
  selected,
  onSelect,
  title,
  description,
  badge,
}: {
  selected: boolean;
  onSelect: () => void;
  title: string;
  description: string;
  badge?: string;
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={selected}
      onClick={onSelect}
      className={cn(
        "flex items-start gap-3 rounded-xl border p-3.5 text-left transition-quick",
        selected
          ? "border-[var(--color-accent)] bg-[var(--color-accent-soft)]"
          : "border-[var(--color-line)] hover:border-[var(--color-line-strong)] hover:bg-[var(--color-surface-hover)]",
      )}
    >
      <span
        aria-hidden
        className={cn(
          "mt-0.5 grid size-4 shrink-0 place-items-center rounded-full border transition-quick",
          selected ? "border-[var(--color-accent)]" : "border-[var(--color-line-strong)]",
        )}
      >
        <span
          className={cn(
            "size-2 rounded-full bg-[var(--color-accent)] transition-quick",
            selected ? "scale-100 opacity-100" : "scale-0 opacity-0",
          )}
        />
      </span>
      <span className="min-w-0">
        <span className="flex items-center gap-2 text-xs font-medium text-[var(--color-ink)]">
          {title}
          {badge ? <Badge tone="neutral">{badge}</Badge> : null}
        </span>
        <span className="mt-0.5 block text-2xs leading-relaxed text-[var(--color-ink-muted)]">
          {description}
        </span>
      </span>
    </button>
  );
}

type Actions = ReturnType<typeof useUpdateActions>;

function StatusLine({
  icon,
  tone = "muted",
  title,
  children,
}: {
  icon: React.ReactNode;
  tone?: "muted" | "ok" | "warn" | "danger" | "accent";
  title: string;
  children?: React.ReactNode;
}) {
  const color = {
    muted: "text-[var(--color-ink-subtle)]",
    ok: "text-[var(--color-ok)]",
    warn: "text-[var(--color-warn)]",
    danger: "text-[var(--color-danger)]",
    accent: "text-[var(--color-accent)]",
  }[tone];
  return (
    <div className="view-enter flex items-start gap-3" aria-live="polite">
      <span className={cn("mt-0.5 shrink-0", color)}>{icon}</span>
      <div className="min-w-0 flex-1">
        <p className="text-sm font-medium text-[var(--color-ink)]">{title}</p>
        {children}
      </div>
    </div>
  );
}

function UpdateStatusBlock({ view, actions }: { view: UpdateView; actions: Actions }) {
  const phase = view.phase;
  const report = view.last_install;

  switch (phase.state) {
    case "unavailable":
      return (
        <StatusLine icon={<CircleAlert className="size-4" />} title="Updates are not available in this build">
          <p className="mt-0.5 text-xs text-[var(--color-ink-muted)]">{phase.reason}</p>
        </StatusLine>
      );
    case "checking":
      return (
        <StatusLine icon={<Spinner className="size-4" />} tone="accent" title="Checking for updates…">
          <p className="mt-0.5 text-xs text-[var(--color-ink-muted)]">
            Reading the latest release information.
          </p>
        </StatusLine>
      );
    case "up_to_date":
      return (
        <StatusLine icon={<CircleCheck className="size-4" />} tone="ok" title="You're up to date.">
          <p className="mt-0.5 text-xs text-[var(--color-ink-muted)]">
            AllInsight v{phase.latest} is the latest version.
          </p>
        </StatusLine>
      );
    case "available":
      return (
        <StatusLine icon={<CircleArrowUp className="size-4" />} tone="accent" title="Update available">
          <ReleaseSummary release={phase.release} />
          {phase.release.installable ? (
            <div className="mt-4 flex gap-2">
              <Button variant="primary" icon={<Download className="size-3.5" />} loading={actions.busy} onClick={actions.download}>
                Update Now
              </Button>
              <Button variant="ghost" onClick={actions.later}>
                Later
              </Button>
            </div>
          ) : (
            <p className="mt-3 text-xs text-[var(--color-ink-muted)]">{phase.release.note}</p>
          )}
        </StatusLine>
      );
    case "downloading": {
      const pct = phase.total ? (phase.downloaded / phase.total) * 100 : 0;
      return (
        <StatusLine icon={<Download className="size-4" />} tone="accent" title="Downloading update…">
          <ProgressBar value={pct} className="mt-3" label="Download progress" />
          <p className="numeric mt-1.5 flex justify-between text-2xs text-[var(--color-ink-muted)]">
            <span>{phase.total ? `${Math.floor(pct)}%` : ""}</span>
            <span>
              {formatBytes(phase.downloaded)}
              {phase.total ? ` / ${formatBytes(phase.total)}` : ""}
            </span>
          </p>
        </StatusLine>
      );
    }
    case "verifying":
      return (
        <StatusLine icon={<Spinner className="size-4" />} tone="accent" title="Verifying update…">
          <p className="mt-0.5 text-xs text-[var(--color-ink-muted)]">
            Checking the download against AllInsight's signed release information.
          </p>
        </StatusLine>
      );
    case "ready":
      return phase.manual_install ? (
        <StatusLine icon={<PackageCheck className="size-4" />} tone="ok" title="Update downloaded and verified.">
          <p className="mt-0.5 text-xs text-[var(--color-ink-muted)]">
            This copy of AllInsight was installed by your system's package manager, so the
            verified package is installed the same way. Show it in its folder, then open it.
          </p>
          <div className="mt-4">
            <Button variant="primary" onClick={actions.install}>
              Show the package
            </Button>
          </div>
        </StatusLine>
      ) : (
        <StatusLine icon={<PackageCheck className="size-4" />} tone="ok" title="Update ready.">
          <p className="mt-0.5 text-xs text-[var(--color-ink-muted)]">
            Restart AllInsight to complete the update to v{phase.release.version}. Your settings and
            history are kept, and backed up first.
          </p>
          <div className="mt-4 flex gap-2">
            <Button variant="primary" icon={<RotateCw className="size-3.5" />} loading={actions.busy} onClick={actions.install}>
              Restart Now
            </Button>
            <Button variant="ghost" onClick={actions.later}>
              Later
            </Button>
          </div>
        </StatusLine>
      );
    case "failed":
      if (phase.kind === "verification") {
        return (
          <StatusLine icon={<ShieldAlert className="size-4" />} tone="danger" title="Update verification failed.">
            <p className="mt-0.5 text-xs text-[var(--color-ink-muted)]">{phase.message}</p>
            <div className="mt-3">
              <Button size="sm" variant="secondary" onClick={actions.check}>
                Try Again
              </Button>
            </div>
          </StatusLine>
        );
      }
      if (phase.kind === "offline") {
        return (
          <StatusLine icon={<WifiOff className="size-4" />} tone="muted" title="Unable to check for updates.">
            <p className="mt-0.5 text-xs text-[var(--color-ink-muted)]">
              AllInsight is still fully functional offline.
            </p>
            <div className="mt-3">
              <Button size="sm" variant="secondary" onClick={actions.check}>
                Try Again
              </Button>
            </div>
          </StatusLine>
        );
      }
      return (
        <StatusLine icon={<CircleAlert className="size-4" />} tone="warn" title={phase.kind === "install" ? "The update was not installed." : "Couldn't check for updates."}>
          <p className="mt-0.5 text-xs text-[var(--color-ink-muted)]">{phase.message}</p>
          <div className="mt-3">
            <Button size="sm" variant="secondary" onClick={actions.check}>
              Try Again
            </Button>
          </div>
        </StatusLine>
      );
    case "idle":
    default:
      if (report?.outcome === "completed") {
        return (
          <StatusLine icon={<CircleCheck className="size-4" />} tone="ok" title={`Updated to v${report.version}.`}>
            <p className="mt-0.5 text-xs text-[var(--color-ink-muted)]">
              The update finished and your data was kept.
            </p>
          </StatusLine>
        );
      }
      if (report?.outcome === "not_completed") {
        return (
          <StatusLine icon={<CircleAlert className="size-4" />} tone="warn" title={`The update to v${report.attempted} did not finish.`}>
            <p className="mt-0.5 text-xs text-[var(--color-ink-muted)]">
              v{report.running} is still installed and working. Check for updates to try again.
            </p>
          </StatusLine>
        );
      }
      return (
        <StatusLine icon={<ShieldCheck className="size-4" />} title="Check for updates whenever you like.">
          <p className="mt-0.5 text-xs text-[var(--color-ink-muted)]">
            Updates are signed, checked, and installed only when you say so.
          </p>
        </StatusLine>
      );
  }
}

function ReleaseSummary({ release }: { release: ReleaseInfo }) {
  return (
    <div className="mt-1">
      <p className="flex items-center gap-2 text-xs text-[var(--color-ink-muted)]">
        <span className="numeric font-medium text-[var(--color-ink)]">v{release.version}</span>
        {release.release_date ? <span>· {release.release_date}</span> : null}
        {release.size ? <span>· {formatBytes(release.size)}</span> : null}
        {release.security ? <Badge tone="warn">Security update</Badge> : null}
      </p>
      {release.notes.length > 0 ? (
        <>
          <p className="mt-3 text-2xs font-medium uppercase tracking-[0.08em] text-[var(--color-ink-subtle)]">
            What's new
          </p>
          <ul className="mt-1.5 space-y-1">
            {release.notes.map((note, i) => (
              <li key={i} className="flex gap-2 text-xs leading-relaxed text-[var(--color-ink-muted)]">
                <span aria-hidden className="mt-[7px] size-1 shrink-0 rounded-full bg-[var(--color-ink-subtle)]" />
                {note}
              </li>
            ))}
          </ul>
        </>
      ) : null}
    </div>
  );
}

// ------------------------------------------------------------------ prompt

/**
 * Shown when an automatic check finds a new version. Asking is all it does:
 * "Update Now" downloads, verifies and then waits for "Restart Now".
 */
export function UpdatePrompt() {
  const { navigate } = useStore();
  const [release, setRelease] = React.useState<ReleaseInfo | null>(null);

  React.useEffect(() => {
    const unlisten = onUpdate((v) => {
      if (v.prompt && v.phase.state === "available" && v.phase.release.installable) {
        setRelease(v.phase.release);
      }
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  if (!release) return null;

  return (
    <Dialog
      open
      onOpenChange={(open) => !open && setRelease(null)}
      title="New update available"
      description={`AllInsight v${release.version}`}
      footer={
        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={() => setRelease(null)}>
            Later
          </Button>
          <Button
            variant="primary"
            icon={<Download className="size-3.5" />}
            onClick={() => {
              setRelease(null);
              requestedSection = "updates";
              navigate("settings");
              // Covers Settings already being on screen.
              window.dispatchEvent(new CustomEvent("allinsight:settings-section", { detail: "updates" }));
              api.downloadUpdate().catch(() => {});
            }}
          >
            Update Now
          </Button>
        </div>
      }
    >
      <div className="px-5 pb-2 pt-1">
        <ReleaseSummary release={release} />
      </div>
    </Dialog>
  );
}
