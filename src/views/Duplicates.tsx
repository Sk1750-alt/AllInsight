/**
 * Duplicate files.
 *
 * Groups are formed by content hash, never by name. Within a group one copy is
 * always kept: the interface will not let every copy in a group be selected,
 * because "free space" is never worth losing the only copy of something.
 */
import * as React from "react";
import { Copy, FolderOpen, Play, ShieldCheck, Square, Trash2 } from "lucide-react";

import { usePlatformWords } from "@/lib/platform";
import { api } from "@/lib/api";
import { useAsync, useStore } from "@/app/store";
import {
  Badge,
  Button,
  EmptyState,
  Hint,
  Panel,
  PageHeader,
} from "@/components/ui/primitives";
import { Checkbox, Select } from "@/components/ui/controls";
import { ConfirmDialog, Tooltip } from "@/components/ui/overlay";
import { formatBytes, formatCount, formatDate, shortenPath } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { DuplicateGroup, DuplicateReport, StorageOverview } from "@/lib/types";

const MIN_SIZES = [
  { value: "1048576", label: "Over 1 MB" },
  { value: "10485760", label: "Over 10 MB" },
  { value: "104857600", label: "Over 100 MB" },
  { value: "1073741824", label: "Over 1 GB" },
];

function GroupCard({
  group,
  index,
  selected,
  onToggle,
  onShow,
}: {
  group: DuplicateGroup;
  index: number;
  selected: Set<string>;
  onToggle: (path: string, value: boolean) => void;
  onShow: (path: string) => void;
}) {
  const selectableCount = group.files.filter((f) => !f.protected).length;
  const selectedInGroup = group.files.filter((f) => selected.has(f.path)).length;
  // One copy always stays. The last unselected copy cannot be selected.
  const atLimit = selectedInGroup >= selectableCount || selectedInGroup >= group.files.length - 1;
  const w = usePlatformWords();

  return (
    <div className="panel">
      <div className="flex items-center justify-between gap-4 px-4 py-2.5 hairline">
        <div className="flex items-baseline gap-3">
          <span className="text-xs font-medium text-[var(--color-ink)]">
            Duplicate group {index + 1}
          </span>
          <span className="numeric text-2xs text-[var(--color-ink-subtle)]">
            {group.files.length} copies · {formatBytes(group.size_bytes)} each
          </span>
        </div>
        <span className="numeric text-xs font-medium text-[var(--color-accent)]">
          {formatBytes(group.reclaimable_bytes)} reclaimable
        </span>
      </div>

      <div className="divide-y divide-[var(--color-line)]">
        {group.files.map((file) => {
          const isSelected = selected.has(file.path);
          const disabled = file.protected || (!isSelected && atLimit);
          return (
            <div
              key={file.path}
              className={cn(
                "flex items-center gap-3 px-4 py-2.5",
                isSelected && "bg-[var(--color-danger-soft)]",
              )}
            >
              <Tooltip
                content={
                  file.protected
                    ? (file.protection_note ?? "This copy is protected.")
                    : disabled
                      ? "At least one copy is always kept."
                      : ""
                }
              >
                <span>
                  <Checkbox
                    checked={isSelected}
                    disabled={disabled}
                    onCheckedChange={(value) => onToggle(file.path, value)}
                    label={`Select ${file.name}`}
                  />
                </span>
              </Tooltip>

              <div className="min-w-0 flex-1">
                <p className="truncate text-xs font-medium text-[var(--color-ink)]">{file.name}</p>
                <p
                  data-selectable
                  className="truncate text-2xs text-[var(--color-ink-muted)]"
                  title={file.directory}
                >
                  {shortenPath(file.directory, 62)}
                </p>
              </div>

              <span className="numeric shrink-0 text-2xs text-[var(--color-ink-subtle)]">
                {formatDate(file.modified)}
              </span>

              {file.protected ? <Badge tone="unknown">Protected</Badge> : null}
              {!isSelected && !file.protected && atLimit ? (
                <Badge tone="ok">Keeping</Badge>
              ) : null}

              <Button
                size="sm"
                variant="ghost"
                onClick={() => onShow(file.path)}
                icon={<FolderOpen className="size-3.5" />}
                aria-label={w.showInFolder}
              />
            </div>
          );
        })}
      </div>
    </div>
  );
}

export function DuplicatesView() {
  const { scanning, scanGeneration, toast, reportError, settings } = useStore();
  const overview = useAsync<StorageOverview>(() => api.getStorageOverview(), []);
  const report = useAsync<DuplicateReport | null>(() => api.getDuplicates(), [scanGeneration]);
  const w = usePlatformWords();

  // Seeded from Settings, so the preference there is the one that applies.
  const [minBytes, setMinBytes] = React.useState(() =>
    String(settings?.duplicate_min_bytes ?? 10_485_760),
  );
  const [root, setRoot] = React.useState("__home__");
  const [selected, setSelected] = React.useState<Set<string>>(new Set());
  const [confirming, setConfirming] = React.useState(false);
  const [removing, setRemoving] = React.useState(false);

  const rootOptions = React.useMemo(
    () => [
      { value: "__home__", label: "Your user folder", hint: "Recommended" },
      ...(overview.data?.volumes ?? [])
        .filter((v) => v.is_ready && v.kind !== "network")
        .map((v) => ({ value: v.mount_point, label: `All of ${v.letter}` })),
    ],
    [overview.data],
  );

  const startSearch = async () => {
    try {
      setSelected(new Set());
      await api.findDuplicates(root === "__home__" ? [] : [root], Number(minBytes));
      toast({
        tone: "info",
        title: "Looking for duplicates",
        body: "Files are compared by content, so this reads data as well as sizes.",
      });
    } catch (e) {
      reportError(e, "The duplicate search could not be started.");
    }
  };

  const selectedBytes = React.useMemo(() => {
    let total = 0;
    for (const group of report.data?.groups ?? []) {
      for (const file of group.files) {
        if (selected.has(file.path)) total += file.size_bytes;
      }
    }
    return total;
  }, [report.data, selected]);

  const removeSelected = async () => {
    setRemoving(true);
    let removed = 0;
    let failed = 0;
    for (const path of selected) {
      try {
        await api.recycleReviewedFile(path);
        removed += 1;
      } catch {
        failed += 1;
      }
    }
    setRemoving(false);
    setConfirming(false);
    setSelected(new Set());
    toast({
      tone: failed > 0 ? "warning" : "success",
      title: `${removed} ${removed === 1 ? "copy" : "copies"} moved to the ${w.trash}`,
      body: failed > 0 ? `${failed} could not be moved and were left alone.` : undefined,
    });
    report.reload();
  };

  return (
    <div className="view-enter space-y-5">
      <PageHeader
        title="Duplicates"
        subtitle="Files with identical contents, found by hashing rather than by name."
        actions={
          scanning ? (
            <Button
              variant="secondary"
              icon={<Square className="size-3" />}
              onClick={() => api.cancelScan()}
            >
              Stop
            </Button>
          ) : (
            <Button variant="primary" icon={<Play className="size-3.5" />} onClick={startSearch}>
              Find duplicates
            </Button>
          )
        }
      />

      <Panel>
        <div className="flex flex-wrap items-center gap-3 p-3">
          <Select
            label="Search in"
            value={root}
            onValueChange={setRoot}
            options={rootOptions}
            className="min-w-44"
          />
          <Select
            label="Minimum size"
            value={minBytes}
            onValueChange={setMinBytes}
            options={MIN_SIZES}
            className="min-w-36"
          />
          {report.data ? (
            <span className="ml-auto text-2xs text-[var(--color-ink-subtle)]">
              {formatCount(report.data.files_compared)} files compared ·{" "}
              {formatCount(report.data.files_hashed)} fully hashed
            </span>
          ) : null}
        </div>
      </Panel>

      {!report.data ? (
        <Panel>
          <EmptyState
            icon={<Copy className="size-5" />}
            title="No duplicate search has run yet"
            description="AllInsight groups files by size first, then compares a small part of each, and only reads a file in full when it still looks like a match. Hashes stay on this device."
            action={
              <Button variant="primary" onClick={startSearch} disabled={scanning}>
                Find duplicates
              </Button>
            }
          />
        </Panel>
      ) : report.data.groups.length === 0 ? (
        <Panel>
          <EmptyState
            icon={<ShieldCheck className="size-5" />}
            title="No duplicates found"
            description="Nothing above the minimum size has an identical copy elsewhere in the area searched."
          />
        </Panel>
      ) : (
        <>
          <div className="flex flex-wrap items-center justify-between gap-4 rounded-lg border border-[var(--color-line)] bg-[var(--color-surface)] px-4 py-3">
            <div>
              <p className="readout text-[26px] text-[var(--color-accent)]">
                {formatBytes(report.data.total_reclaimable_bytes)}
              </p>
              <p className="text-2xs text-[var(--color-ink-muted)]">
                across {formatCount(report.data.groups.length)} groups, if one copy of each is kept
              </p>
            </div>

            <div className="flex items-center gap-3">
              {selected.size > 0 ? (
                <span className="numeric text-xs text-[var(--color-ink-muted)]">
                  {selected.size} selected · {formatBytes(selectedBytes)}
                </span>
              ) : null}
              <Button
                variant="ghost"
                size="sm"
                disabled={selected.size === 0}
                onClick={() => setSelected(new Set())}
              >
                Clear
              </Button>
              <Button
                variant="danger"
                size="sm"
                disabled={selected.size === 0}
                icon={<Trash2 className="size-3.5" />}
                onClick={() => setConfirming(true)}
              >
                Move selected to {w.trash}
              </Button>
            </div>
          </div>

          <div className="space-y-3">
            {report.data.groups.map((group, index) => (
              <GroupCard
                key={group.id}
                group={group}
                index={index}
                selected={selected}
                onToggle={(path, value) =>
                  setSelected((current) => {
                    const next = new Set(current);
                    if (value) next.add(path);
                    else next.delete(path);
                    return next;
                  })
                }
                onShow={(path) => api.showInExplorer(path).catch((e) => reportError(e))}
              />
            ))}
          </div>
        </>
      )}

      <Hint>
        Duplicates are never removed automatically. At least one copy of every group is always
        kept, and anything you do remove goes to the {w.trash}.
      </Hint>

      <ConfirmDialog
        open={confirming}
        onOpenChange={setConfirming}
        title={`Move ${selected.size} duplicate ${selected.size === 1 ? "copy" : "copies"} to the ${w.trash}?`}
        loading={removing}
        confirmLabel={`Move to ${w.trash}`}
        onConfirm={removeSelected}
        estimate={
          <div className="flex items-baseline justify-between">
            <span className="text-2xs uppercase tracking-wider text-[var(--color-ink-subtle)]">
              Space recovered
            </span>
            <span className="numeric font-display text-lg font-semibold text-[var(--color-accent)]">
              {formatBytes(selectedBytes)}
            </span>
          </div>
        }
        whatHappens={`Each selected copy is moved to the ${w.trash}, where it stays recoverable until it is emptied.`}
        whatIsUntouched="At least one copy of every group. Protected files. Anything you did not select."
      />
    </div>
  );
}
