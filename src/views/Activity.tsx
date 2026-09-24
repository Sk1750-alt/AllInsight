/**
 * Activity.
 *
 * Cleanup history and a plain log of what AllInsight did. Both are aggregate on
 * purpose: sizes, counts and category names, never file names. A history that
 * listed every file removed would be an index of the user's data, which is
 * exactly what this application refuses to build.
 */
import * as React from "react";
import {
  Brain,
  FileClock,
  HardDrive,
  Power,
  RefreshCw,
  ScanLine,
  Trash2,
  TriangleAlert,
} from "lucide-react";

import { api } from "@/lib/api";
import { useAsync } from "@/app/store";
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
import { Table, Td, Th, Tr } from "@/components/ui/data";
import { formatBytes, formatCount, formatDateTime, formatRelative } from "@/lib/format";
import type { ActivityEntry, CleanupHistoryEntry, CleanupTotals } from "@/lib/types";

const KIND_ICON: Record<string, React.ComponentType<{ className?: string }>> = {
  cleanup: Trash2,
  scan: ScanLine,
  alert: TriangleAlert,
  ai: Brain,
  startup: Power,
  application: HardDrive,
  process: FileClock,
  review: Trash2,
};

const TRIGGER_LABEL: Record<string, string> = {
  manual: "Manual",
  auto: "Automatic",
  emergency: "Low space",
};

export function ActivityView() {
  const totals = useAsync<CleanupTotals>(() => api.getCleanupTotals(), []);
  const history = useAsync<CleanupHistoryEntry[]>(() => api.getCleanupHistory(100), []);
  const log = useAsync<ActivityEntry[]>(() => api.getActivity(150), []);

  const reloadAll = () => {
    totals.reload();
    history.reload();
    log.reload();
  };

  return (
    <div className="view-enter space-y-5">
      <PageHeader
        title="Activity"
        subtitle="What AllInsight has done on this device, recorded as totals rather than file lists."
        actions={
          <Button variant="ghost" icon={<RefreshCw className="size-3.5" />} onClick={reloadAll}>
            Refresh
          </Button>
        }
      />

      <div className="grid gap-4 sm:grid-cols-3">
        <Panel className="p-4">
          <p className="text-2xs uppercase tracking-wider text-[var(--color-ink-subtle)]">
            Space reclaimed
          </p>
          <p className="readout mt-1 text-[30px] text-[var(--color-accent)]">
            {formatBytes(totals.data?.reclaimed_bytes ?? 0)}
          </p>
          <p className="text-xs text-[var(--color-ink-muted)]">since AllInsight was installed</p>
        </Panel>

        <Panel className="p-4">
          <p className="text-2xs uppercase tracking-wider text-[var(--color-ink-subtle)]">
            Cleanup runs
          </p>
          <p className="readout mt-1 text-[30px] text-[var(--color-ink)]">
            {formatCount(totals.data?.runs ?? 0)}
          </p>
          <p className="text-xs text-[var(--color-ink-muted)]">manual and automatic</p>
        </Panel>

        <Panel className="p-4">
          <p className="text-2xs uppercase tracking-wider text-[var(--color-ink-subtle)]">
            Items removed
          </p>
          <p className="readout mt-1 text-[30px] text-[var(--color-ink)]">
            {formatCount(totals.data?.removed_items ?? 0)}
          </p>
          <p className="text-xs text-[var(--color-ink-muted)]">temporary and cached files</p>
        </Panel>
      </div>

      <Panel>
        <PanelHeader
          title="Cleanup history"
          description="Sizes, counts and category names only. No file names are recorded."
        />

        {history.loading ? (
          <div className="space-y-2 p-4">
            {Array.from({ length: 4 }).map((_, i) => (
              <Skeleton key={i} className="h-10" />
            ))}
          </div>
        ) : (history.data?.length ?? 0) === 0 ? (
          <EmptyState
            icon={<Trash2 className="size-5" />}
            title="No cleanup has run yet"
            description="When you clean something, it is recorded here as a total."
          />
        ) : (
          <Table>
            <thead>
              <tr>
                <Th>When</Th>
                <Th>Trigger</Th>
                <Th>Categories</Th>
                <Th align="right">Reclaimed</Th>
                <Th align="right">Items</Th>
                <Th align="right">Skipped</Th>
              </tr>
            </thead>
            <tbody>
              {history.data!.map((entry, index) => (
                <Tr key={`${entry.ran_at}-${index}`}>
                  <Td>
                    <span className="block text-[var(--color-ink)]">
                      {formatDateTime(entry.ran_at)}
                    </span>
                    <span className="block text-2xs text-[var(--color-ink-subtle)]">
                      {formatRelative(entry.ran_at)}
                    </span>
                  </Td>
                  <Td>
                    <Badge tone={entry.trigger === "auto" ? "accent" : "neutral"}>
                      {TRIGGER_LABEL[entry.trigger] ?? entry.trigger}
                    </Badge>
                  </Td>
                  <Td className="max-w-[320px] text-[var(--color-ink-muted)]">
                    <span className="block truncate" title={entry.categories.join(", ")}>
                      {entry.categories.join(", ") || "-"}
                    </span>
                  </Td>
                  <Td align="right" className="font-medium text-[var(--color-accent)]">
                    {formatBytes(entry.reclaimed_bytes)}
                  </Td>
                  <Td align="right">{formatCount(entry.removed_items)}</Td>
                  <Td align="right" className="text-[var(--color-ink-subtle)]">
                    {formatCount(entry.skipped_items)}
                  </Td>
                </Tr>
              ))}
            </tbody>
          </Table>
        )}
      </Panel>

      <Panel>
        <PanelHeader
          title="Event log"
          description="The most recent things AllInsight did, kept locally."
        />

        {log.loading ? (
          <div className="space-y-2 p-4">
            {Array.from({ length: 6 }).map((_, i) => (
              <Skeleton key={i} className="h-8" />
            ))}
          </div>
        ) : (log.data?.length ?? 0) === 0 ? (
          <EmptyState title="Nothing recorded yet" />
        ) : (
          <div className="max-h-[420px] divide-y divide-[var(--color-line)] overflow-y-auto">
            {log.data!.map((entry, index) => {
              const Icon = KIND_ICON[entry.kind] ?? FileClock;
              return (
                <div
                  key={`${entry.happened_at}-${index}`}
                  className="flex items-start gap-3 px-4 py-2.5"
                >
                  <Icon className="mt-0.5 size-3.5 shrink-0 text-[var(--color-ink-subtle)]" />
                  <div className="min-w-0 flex-1">
                    <p className="text-xs text-[var(--color-ink)]">{entry.summary}</p>
                    {entry.detail ? (
                      <p className="mt-0.5 text-2xs text-[var(--color-ink-muted)]">
                        {entry.detail}
                      </p>
                    ) : null}
                  </div>
                  <span className="numeric shrink-0 text-2xs text-[var(--color-ink-subtle)]">
                    {formatRelative(entry.happened_at)}
                  </span>
                </div>
              );
            })}
          </div>
        )}
      </Panel>

      <Hint>
        This history lives in a local database in your AllInsight data folder. Deleting that folder
        removes it completely, and nothing is ever sent anywhere.
      </Hint>
    </div>
  );
}
