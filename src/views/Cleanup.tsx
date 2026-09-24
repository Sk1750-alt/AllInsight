/**
 * The cleanup screen.
 *
 * The flow is fixed and deliberate: measure, choose, preview, confirm, report.
 * The confirmation always states what will happen and what will not, and the
 * numbers on it come from a dry run through the same code that performs the
 * removal, so the preview cannot promise something different from the action.
 */
import * as React from "react";
import {
  AlertTriangle,
  CheckCircle2,
  Info,
  Lock,
  RefreshCw,
  ShieldCheck,
  Sparkles,
  Trash2,
} from "lucide-react";

import { usePlatformWords } from "@/lib/platform";
import { api } from "@/lib/api";
import { useStore } from "@/app/store";
import {
  Badge,
  Button,
  EmptyState,
  Hint,
  Panel,
  PanelHeader,
  PageHeader,
  Skeleton,
} from "@/components/ui/primitives";
import { Checkbox } from "@/components/ui/controls";
import { ConfirmDialog, Dialog, Tooltip } from "@/components/ui/overlay";
import { ProgressBar } from "@/components/ui/data";
import { formatBytes, formatCount, formatRelative, shortenPath } from "@/lib/format";
import { cn } from "@/lib/utils";
import type {
  CategoryReport,
  CleanupCategory,
  CleanupOutcome,
  CleanupPreview,
} from "@/lib/types";

function CategoryRow({
  report,
  checked,
  onToggle,
  onInspect,
}: {
  report: CategoryReport;
  checked: boolean;
  onToggle: (value: boolean) => void;
  onInspect: () => void;
}) {
  const selectable = report.items > 0 || report.bytes > 0;

  return (
    <div
      className={cn(
        "flex items-center gap-3 px-4 py-3 transition-quick",
        selectable ? "hover:bg-[var(--color-surface-hover)]" : "opacity-60",
      )}
    >
      <Checkbox
        checked={checked}
        onCheckedChange={onToggle}
        disabled={!selectable}
        label={`Include ${report.name}`}
      />

      <button onClick={onInspect} className="min-w-0 flex-1 text-left">
        <div className="flex items-center gap-2">
          <span className="truncate text-xs font-medium text-[var(--color-ink)]">
            {report.name}
          </span>
          {report.requires_elevation ? (
            <Tooltip content="Administrator permission is needed to reach all of this category.">
              <span>
                <Badge tone="neutral">
                  <Lock className="size-2.5" />
                  Admin
                </Badge>
              </span>
            </Tooltip>
          ) : null}
          {report.deletion === "shell_api" ? <Badge tone="warn">Not reversible</Badge> : null}
          {report.auto_clean_eligible ? <Badge tone="neutral">Auto-clean</Badge> : null}
        </div>
        <p className="mt-0.5 truncate text-2xs text-[var(--color-ink-muted)]">
          {report.note ?? report.description}
        </p>
      </button>

      <div className="w-32 shrink-0 text-right">
        <p className="numeric text-xs font-medium text-[var(--color-ink)]">
          {report.bytes > 0 ? formatBytes(report.bytes) : "Nothing"}
        </p>
        {report.items > 0 ? (
          <p className="numeric text-2xs text-[var(--color-ink-subtle)]">
            {formatCount(report.items)} items
          </p>
        ) : null}
      </div>
    </div>
  );
}

function OutcomeSummary({ outcome, onDismiss }: { outcome: CleanupOutcome; onDismiss: () => void }) {
  return (
    <Panel className="border-[color-mix(in_srgb,var(--color-ok)_35%,transparent)]">
      <div className="flex items-start gap-3 p-4">
        <CheckCircle2 className="mt-0.5 size-5 shrink-0 text-[var(--color-ok)]" />
        <div className="min-w-0 flex-1">
          <p className="text-sm font-medium text-[var(--color-ink)]">
            {formatBytes(outcome.reclaimed_bytes)} reclaimed
          </p>
          <p className="mt-0.5 text-xs text-[var(--color-ink-muted)]">
            {formatCount(outcome.removed_items)} items removed
            {outcome.protected_items > 0
              ? ` · ${formatCount(outcome.protected_items)} protected and left alone`
              : ""}
            {outcome.skipped_items + outcome.failed_items > 0
              ? ` · ${formatCount(outcome.skipped_items + outcome.failed_items)} skipped`
              : ""}
          </p>

          {outcome.categories.length > 0 ? (
            <div className="mt-3 space-y-1">
              {outcome.categories
                .filter((c) => c.reclaimed_bytes > 0)
                .map((category) => (
                  <div key={category.category} className="flex justify-between text-xs">
                    <span className="text-[var(--color-ink-muted)]">{category.name}</span>
                    <span className="numeric text-[var(--color-ink)]">
                      {formatBytes(category.reclaimed_bytes)}
                    </span>
                  </div>
                ))}
            </div>
          ) : null}

          {outcome.notes.length > 0 ? (
            <div className="mt-3 space-y-1">
              {outcome.notes.map((note) => (
                <p key={note} className="text-2xs text-[var(--color-ink-subtle)]">
                  {note}
                </p>
              ))}
            </div>
          ) : null}
        </div>
        <Button size="sm" variant="ghost" onClick={onDismiss}>
          Dismiss
        </Button>
      </div>
    </Panel>
  );
}

export function CleanupView() {
  const { toast, reportError, navigate } = useStore();
  const w = usePlatformWords();

  const [preview, setPreview] = React.useState<CleanupPreview | null>(null);
  const [measuring, setMeasuring] = React.useState(false);
  const [selection, setSelection] = React.useState<Set<CleanupCategory>>(new Set());
  const [dryRun, setDryRun] = React.useState<CleanupOutcome | null>(null);
  const [confirming, setConfirming] = React.useState(false);
  const [running, setRunning] = React.useState(false);
  const [outcome, setOutcome] = React.useState<CleanupOutcome | null>(null);
  const [inspecting, setInspecting] = React.useState<CategoryReport | null>(null);

  const measure = React.useCallback(async () => {
    setMeasuring(true);
    setOutcome(null);
    try {
      const next = await api.getCleanupCandidates();
      setPreview(next);
      // Pre-select the categories that are both safe to run unattended and
      // actually have something in them. Nothing irreversible is pre-selected.
      setSelection(
        new Set(
          next.categories
            .filter((c) => c.auto_clean_eligible && c.bytes > 0)
            .map((c) => c.category),
        ),
      );
    } catch (e) {
      reportError(e, "Cleanup could not be measured.");
    } finally {
      setMeasuring(false);
    }
  }, [reportError]);

  React.useEffect(() => {
    measure();
  }, [measure]);

  const selectedReports = React.useMemo(
    () => (preview?.categories ?? []).filter((c) => selection.has(c.category)),
    [preview, selection],
  );

  const selectedBytes = selectedReports.reduce((sum, c) => sum + c.bytes, 0);
  const selectedItems = selectedReports.reduce((sum, c) => sum + c.items, 0);
  const includesIrreversible = selectedReports.some((c) => c.deletion === "shell_api");

  const openConfirm = async () => {
    if (!preview || selection.size === 0) return;
    try {
      const result = await api.previewCleanup(preview.scan_id, [...selection]);
      setDryRun(result);
      setConfirming(true);
    } catch (e) {
      reportError(e, "The cleanup preview could not be produced.");
    }
  };

  const run = async () => {
    if (!preview) return;
    setRunning(true);
    try {
      const result = await api.executeCleanup({
        scan_id: preview.scan_id,
        categories: [...selection],
        candidate_ids: [],
        confirmed: true,
      });
      setOutcome(result);
      setConfirming(false);
      setDryRun(null);
      toast({
        tone: "success",
        title: `${formatBytes(result.reclaimed_bytes)} reclaimed`,
        body: `${formatCount(result.removed_items)} items removed.`,
      });
      await measure();
    } catch (e) {
      reportError(e, "The cleanup could not be completed.");
    } finally {
      setRunning(false);
    }
  };

  const available = (preview?.categories ?? []).filter((c) => c.available || c.bytes > 0);
  const totalBytes = preview?.total_bytes ?? 0;

  return (
    <div className="view-enter space-y-5">
      <PageHeader
        title="Cleanup"
        subtitle={`Temporary and cached data that ${w.os} and your applications rebuild automatically.`}
        actions={
          <Button
            variant="secondary"
            icon={<RefreshCw className="size-3.5" />}
            loading={measuring}
            onClick={measure}
          >
            Measure again
          </Button>
        }
      />

      {outcome ? <OutcomeSummary outcome={outcome} onDismiss={() => setOutcome(null)} /> : null}

      <div className="grid gap-5 lg:grid-cols-[1fr_320px]">
        <Panel>
          <PanelHeader
            title="Safe categories"
            description={
              preview
                ? `Measured ${formatRelative(preview.generated_at)}. ${formatCount(preview.protected_items)} protected items were excluded.`
                : undefined
            }
            actions={
              <div className="flex gap-2">
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() =>
                    setSelection(
                      new Set(
                        available.filter((c) => c.bytes > 0).map((c) => c.category),
                      ),
                    )
                  }
                >
                  Select all
                </Button>
                <Button size="sm" variant="ghost" onClick={() => setSelection(new Set())}>
                  Clear
                </Button>
              </div>
            }
          />

          {measuring && !preview ? (
            <div className="space-y-2 p-4">
              {Array.from({ length: 6 }).map((_, i) => (
                <Skeleton key={i} className="h-11" />
              ))}
            </div>
          ) : available.length === 0 ? (
            <EmptyState
              icon={<ShieldCheck className="size-5" />}
              title="Nothing to clean"
              description="No safe category on this device currently holds anything worth removing."
            />
          ) : (
            <div className="divide-y divide-[var(--color-line)]">
              {available.map((report) => (
                <CategoryRow
                  key={report.category}
                  report={report}
                  checked={selection.has(report.category)}
                  onToggle={(value) =>
                    setSelection((current) => {
                      const next = new Set(current);
                      if (value) next.add(report.category);
                      else next.delete(report.category);
                      return next;
                    })
                  }
                  onInspect={() => setInspecting(report)}
                />
              ))}
            </div>
          )}
        </Panel>

        <div className="space-y-5">
          <Panel>
            <div className="space-y-4 p-4">
              <div>
                <p className="text-2xs uppercase tracking-wider text-[var(--color-ink-subtle)]">
                  Selected
                </p>
                <p className="readout mt-1 text-[40px] text-[var(--color-accent)]">
                  {formatBytes(selectedBytes)}
                </p>
                <p className="mt-0.5 text-xs text-[var(--color-ink-muted)]">
                  {formatCount(selectedItems)} items across {selection.size}{" "}
                  {selection.size === 1 ? "category" : "categories"}
                </p>
              </div>

              {totalBytes > 0 ? (
                <div>
                  <ProgressBar
                    value={(selectedBytes / totalBytes) * 100}
                    tone="accent"
                    label="Share of everything available"
                  />
                  <p className="mt-1.5 text-2xs text-[var(--color-ink-subtle)]">
                    {formatBytes(totalBytes)} available in total
                  </p>
                </div>
              ) : null}

              <Button
                variant="primary"
                className="w-full"
                disabled={selection.size === 0 || selectedBytes === 0}
                icon={<Trash2 className="size-3.5" />}
                onClick={openConfirm}
              >
                Review and clean
              </Button>

              <Hint>
                Nothing is removed until you confirm on the next screen, which lists exactly what
                will and will not be touched.
              </Hint>
            </div>
          </Panel>

          <Panel>
            <PanelHeader title="What is never included" />
            <div className="space-y-2 p-4 text-xs text-[var(--color-ink-muted)]">
              <p>Documents, Desktop, Pictures, Videos, Music and Downloads.</p>
              <p>Source code, databases, browser profiles and saved passwords.</p>
              <p>
                {w.isWindows
                  ? "Windows itself, Program Files, and anything you added to the protected list."
                  : `${w.os} itself, installed applications, other users' files, and anything you added to the protected list.`}
              </p>
              <Button
                size="sm"
                variant="ghost"
                className="mt-1 px-0"
                onClick={() => navigate("settings")}
              >
                Review the protected list
              </Button>
            </div>
          </Panel>

          {preview && !preview.elevated && w.canElevate ? (
            <Panel className="border-[color-mix(in_srgb,var(--color-warn)_30%,transparent)]">
              <div className="flex gap-3 p-4">
                <Info className="mt-0.5 size-4 shrink-0 text-[var(--color-warn)]" />
                <div>
                  <p className="text-xs font-medium text-[var(--color-ink)]">
                    Some categories need administrator permission
                  </p>
                  <p className="mt-1 text-2xs leading-relaxed text-[var(--color-ink-muted)]">
                    AllInsight runs without elevation by choice. Windows Update and servicing caches
                    are only measurable when it is elevated.
                  </p>
                  <Button
                    size="sm"
                    variant="subtle"
                    className="mt-2"
                    onClick={() => api.restartElevated().catch((e) => reportError(e))}
                  >
                    Restart as administrator
                  </Button>
                </div>
              </div>
            </Panel>
          ) : null}
        </div>
      </div>

      {/* Confirmation, driven by the dry run. */}
      <ConfirmDialog
        open={confirming}
        onOpenChange={setConfirming}
        title="Clean the selected categories?"
        loading={running}
        destructive
        confirmLabel={`Clean ${formatBytes(dryRun?.reclaimed_bytes ?? selectedBytes)}`}
        onConfirm={run}
        estimate={
          <div className="space-y-2">
            <div className="flex items-baseline justify-between">
              <span className="text-2xs uppercase tracking-wider text-[var(--color-ink-subtle)]">
                Cleanup preview
              </span>
              <span className="numeric font-display text-lg font-semibold text-[var(--color-accent)]">
                {formatBytes(dryRun?.reclaimed_bytes ?? 0)}
              </span>
            </div>
            <div className="grid grid-cols-3 gap-2 text-2xs">
              <div>
                <p className="text-[var(--color-ink-subtle)]">Files</p>
                <p className="numeric text-[var(--color-ink)]">
                  {formatCount(dryRun?.removed_items ?? 0)}
                </p>
              </div>
              <div>
                <p className="text-[var(--color-ink-subtle)]">Protected</p>
                <p className="numeric text-[var(--color-ink)]">
                  {formatCount((dryRun?.protected_items ?? 0) + (preview?.protected_items ?? 0))}
                </p>
              </div>
              <div>
                <p className="text-[var(--color-ink-subtle)]">Skipped</p>
                <p className="numeric text-[var(--color-ink)]">
                  {formatCount((dryRun?.skipped_items ?? 0) + (preview?.skipped_items ?? 0))}
                </p>
              </div>
            </div>
          </div>
        }
        whatHappens={
          selectedReports.length === 1
            ? selectedReports[0].what_happens
            : `${selectedReports.length} categories will be cleaned: ${selectedReports
                .map((c) => c.name.toLowerCase())
                .join(", ")}. Each removes only its own temporary or cached data, which ${w.os} and your applications rebuild automatically.`
        }
        whatIsUntouched={`Documents, Desktop, Pictures, Videos, Music and Downloads. Browser profiles, saved passwords, source code and databases. ${w.os} itself, installed programs, and anything on your protected list.`}
        extra={
          includesIrreversible ? (
            <div className="flex gap-2 rounded-md border border-[color-mix(in_srgb,var(--color-warn)_35%,transparent)] bg-[var(--color-warn-soft)] p-2.5">
              <AlertTriangle className="mt-0.5 size-3.5 shrink-0 text-[var(--color-warn)]" />
              <p className="text-2xs leading-relaxed text-[var(--color-ink)]">
                Your selection includes emptying the {w.trash}. Items in it cannot be restored
                afterwards.
              </p>
            </div>
          ) : null
        }
      />

      {/* Per-category detail, including a sample of what was found. */}
      <Dialog
        open={!!inspecting}
        onOpenChange={(open) => !open && setInspecting(null)}
        title={inspecting?.name ?? ""}
        description={inspecting?.description}
        width="lg"
        footer={
          <Button variant="secondary" onClick={() => setInspecting(null)}>
            Close
          </Button>
        }
      >
        {inspecting ? (
          <div className="space-y-4">
            <div className="grid grid-cols-2 gap-3">
              <div className="rounded-md border border-[var(--color-line)] p-3">
                <p className="text-2xs uppercase tracking-wider text-[var(--color-ink-subtle)]">
                  Size
                </p>
                <p className="numeric mt-1 text-lg font-semibold text-[var(--color-ink)]">
                  {formatBytes(inspecting.bytes)}
                </p>
              </div>
              <div className="rounded-md border border-[var(--color-line)] p-3">
                <p className="text-2xs uppercase tracking-wider text-[var(--color-ink-subtle)]">
                  Items
                </p>
                <p className="numeric mt-1 text-lg font-semibold text-[var(--color-ink)]">
                  {formatCount(inspecting.items)}
                </p>
              </div>
            </div>

            <div>
              <p className="text-2xs font-semibold uppercase tracking-wider text-[var(--color-ink-subtle)]">
                What will happen
              </p>
              <p className="mt-1 text-xs leading-relaxed text-[var(--color-ink)]">
                {inspecting.what_happens}
              </p>
            </div>

            <div>
              <p className="text-2xs font-semibold uppercase tracking-wider text-[var(--color-ink-subtle)]">
                What will not be affected
              </p>
              <p className="mt-1 text-xs leading-relaxed text-[var(--color-ink-muted)]">
                {inspecting.what_is_untouched}
              </p>
            </div>

            {inspecting.samples.length > 0 ? (
              <div>
                <p className="text-2xs font-semibold uppercase tracking-wider text-[var(--color-ink-subtle)]">
                  Examples from this category
                </p>
                <ul className="mt-1.5 space-y-1">
                  {inspecting.samples.map((sample) => (
                    <li
                      key={sample.id}
                      data-selectable
                      className="flex items-baseline justify-between gap-3 text-2xs"
                    >
                      <span className="truncate font-mono text-[var(--color-ink-muted)]">
                        {shortenPath(sample.path, 62)}
                      </span>
                      <span className="numeric shrink-0 text-[var(--color-ink-subtle)]">
                        {formatBytes(sample.size_bytes)}
                      </span>
                    </li>
                  ))}
                </ul>
              </div>
            ) : null}

            {inspecting.note ? (
              <div className="flex gap-2 rounded-md border border-[var(--color-line)] p-2.5">
                <Sparkles className="mt-0.5 size-3.5 shrink-0 text-[var(--color-ink-subtle)]" />
                <p className="text-2xs leading-relaxed text-[var(--color-ink-muted)]">
                  {inspecting.note}
                </p>
              </div>
            ) : null}
          </div>
        ) : null}
      </Dialog>
    </div>
  );
}
