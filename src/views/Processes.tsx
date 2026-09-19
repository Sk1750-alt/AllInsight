/**
 * The process monitor.
 *
 * Ending a process is the one destructive action on this screen, so the rules
 * are visible: processes Windows needs are marked and their End button is
 * disabled, and everything else asks for confirmation first.
 */
import * as React from "react";
import { FolderOpen, Lock, RefreshCw, ShieldAlert, XCircle } from "lucide-react";

import { api } from "@/lib/api";
import { usePolled, useStore } from "@/app/store";
import {
  Badge,
  Button,
  EmptyState,
  Hint,
  Panel,
  PanelHeader,
  PageHeader,
} from "@/components/ui/primitives";
import { SearchInput, SegmentedControl } from "@/components/ui/controls";
import { ConfirmDialog, Tooltip } from "@/components/ui/overlay";
import { ProgressBar, Table, Td, Th, Tr } from "@/components/ui/data";
import { formatBytes, formatCount, formatDuration, formatPercent } from "@/lib/format";
import type { ProcessInfo, ProcessList } from "@/lib/types";

type SortKey = "cpu" | "memory" | "name" | "disk";

export function ProcessesView() {
  const { toast, reportError, settings } = useStore();
  const [query, setQuery] = React.useState("");
  const [sort, setSort] = React.useState<SortKey>("cpu");
  const [pending, setPending] = React.useState<ProcessInfo | null>(null);
  const [ending, setEnding] = React.useState(false);
  const [paused, setPaused] = React.useState(false);

  const list = usePolled<ProcessList>(() => api.getProcesses(140, true), 2500, !paused && !pending);
  const [snapshot, setSnapshot] = React.useState<ProcessList | null>(null);
  React.useEffect(() => {
    if (list) setSnapshot(list);
  }, [list]);

  const rows = React.useMemo(() => {
    const processes = snapshot?.processes ?? [];
    const q = query.trim().toLowerCase();
    const filtered = q
      ? processes.filter(
          (p) =>
            p.name.toLowerCase().includes(q) ||
            (p.publisher ?? "").toLowerCase().includes(q) ||
            String(p.pid).includes(q),
        )
      : processes;

    const sorted = [...filtered];
    if (sort === "cpu") sorted.sort((a, b) => b.cpu_percent - a.cpu_percent);
    if (sort === "memory") sorted.sort((a, b) => b.memory_bytes - a.memory_bytes);
    if (sort === "name") sorted.sort((a, b) => a.name.localeCompare(b.name));
    if (sort === "disk")
      sorted.sort(
        (a, b) =>
          b.disk_read_bytes + b.disk_write_bytes - (a.disk_read_bytes + a.disk_write_bytes),
      );
    return sorted;
  }, [snapshot, query, sort]);

  const endProcess = async () => {
    if (!pending) return;
    setEnding(true);
    try {
      await api.endProcess(pending.pid, true);
      toast({ tone: "success", title: `${pending.name} was ended` });
      setPending(null);
    } catch (e) {
      reportError(e, "That process could not be ended.");
    } finally {
      setEnding(false);
    }
  };

  const showLocation = async (process: ProcessInfo) => {
    try {
      const location = process.executable ?? (await api.getProcessLocation(process.pid));
      if (!location) {
        toast({ tone: "warning", title: "Windows did not report a location for that process." });
        return;
      }
      await api.showInExplorer(location);
    } catch (e) {
      reportError(e);
    }
  };

  return (
    <div className="view-enter space-y-5">
      <PageHeader
        title="Processes"
        subtitle={
          snapshot
            ? `${formatCount(snapshot.total)} processes running`
            : "Reading the process list"
        }
        actions={
          <>
            <SegmentedControl
              label="Sort by"
              value={sort}
              onChange={setSort}
              options={[
                { value: "cpu", label: "CPU" },
                { value: "memory", label: "Memory" },
                { value: "disk", label: "Disk" },
                { value: "name", label: "Name" },
              ]}
            />
            <Button
              variant={paused ? "primary" : "secondary"}
              size="md"
              icon={<RefreshCw className="size-3.5" />}
              onClick={() => setPaused((p) => !p)}
            >
              {paused ? "Resume" : "Pause"}
            </Button>
          </>
        }
      />

      <Panel>
        <div className="p-3">
          <SearchInput
            value={query}
            onChange={setQuery}
            placeholder="Filter by name, publisher or process id"
          />
        </div>
      </Panel>

      <Panel>
        <PanelHeader
          title="Running processes"
          description={
            paused ? "Paused. The list is frozen at the last sample." : "Refreshing every 2.5 s."
          }
        />

        {rows.length === 0 ? (
          <EmptyState
            title={query ? "Nothing matched that filter" : "No processes to show"}
            description={query ? "Try a different name or process id." : undefined}
          />
        ) : (
          <div className="max-h-[calc(100vh-330px)] overflow-y-auto">
            <Table>
              <thead>
                <tr>
                  <Th>Process</Th>
                  <Th>Publisher</Th>
                  <Th align="right">CPU</Th>
                  <Th align="right">Memory</Th>
                  <Th align="right">Disk</Th>
                  <Th align="right">PID</Th>
                  <Th align="right">Running</Th>
                  <Th align="right">Actions</Th>
                </tr>
              </thead>
              <tbody>
                {rows.map((process) => (
                  <Tr key={process.pid}>
                    <Td className="max-w-[220px]">
                      <div className="flex items-center gap-2">
                        <span className="truncate font-medium" title={process.name}>
                          {process.name}
                        </span>
                        {process.risk === "critical" ? (
                          <Tooltip content="Windows needs this process. AllInsight will not end it.">
                            <span>
                              <Lock className="size-3 text-[var(--color-ink-subtle)]" />
                            </span>
                          </Tooltip>
                        ) : null}
                        {process.is_top_cpu ? <Badge tone="accent">Top CPU</Badge> : null}
                      </div>
                    </Td>
                    <Td className="max-w-[180px] text-[var(--color-ink-muted)]">
                      <span className="block truncate">{process.publisher ?? "Unknown"}</span>
                    </Td>
                    <Td align="right">
                      <div className="flex items-center justify-end gap-2">
                        <span className="w-12">
                          <ProgressBar
                            value={process.cpu_percent}
                            tone={process.cpu_percent > 50 ? "warn" : "accent"}
                            height={3}
                            label="CPU"
                          />
                        </span>
                        <span className="w-10">{formatPercent(process.cpu_percent, 1)}</span>
                      </div>
                    </Td>
                    <Td align="right">{formatBytes(process.memory_bytes)}</Td>
                    <Td align="right" className="text-[var(--color-ink-muted)]">
                      {formatBytes(process.disk_read_bytes + process.disk_write_bytes)}
                    </Td>
                    <Td align="right" className="text-[var(--color-ink-subtle)]">
                      {process.pid}
                    </Td>
                    <Td align="right" className="text-[var(--color-ink-subtle)]">
                      {formatDuration(process.run_time_seconds)}
                    </Td>
                    <Td align="right">
                      <div className="flex justify-end gap-1">
                        <Tooltip content="Open the folder containing this program">
                          <span>
                            <Button
                              size="sm"
                              variant="ghost"
                              onClick={() => showLocation(process)}
                              icon={<FolderOpen className="size-3.5" />}
                              aria-label="Open file location"
                            />
                          </span>
                        </Tooltip>
                        <Tooltip
                          content={
                            process.risk === "critical"
                              ? "Windows requires this process."
                              : "End this process"
                          }
                        >
                          <span>
                            <Button
                              size="sm"
                              variant="ghost"
                              disabled={process.risk === "critical"}
                              onClick={() => setPending(process)}
                              icon={<XCircle className="size-3.5" />}
                              aria-label="End process"
                            />
                          </span>
                        </Tooltip>
                      </div>
                    </Td>
                  </Tr>
                ))}
              </tbody>
            </Table>
          </div>
        )}
      </Panel>

      <Hint>
        Ending a process discards anything it has not saved. AllInsight refuses outright for the
        processes Windows needs in order to keep running.
      </Hint>

      <ConfirmDialog
        open={!!pending}
        onOpenChange={(open) => !open && setPending(null)}
        title={`End ${pending?.name ?? ""}?`}
        loading={ending}
        confirmLabel="End process"
        onConfirm={endProcess}
        estimate={
          pending ? (
            <div className="grid grid-cols-3 gap-3 text-2xs">
              <div>
                <p className="text-[var(--color-ink-subtle)]">Process id</p>
                <p className="numeric text-[var(--color-ink)]">{pending.pid}</p>
              </div>
              <div>
                <p className="text-[var(--color-ink-subtle)]">Memory</p>
                <p className="numeric text-[var(--color-ink)]">
                  {formatBytes(pending.memory_bytes)}
                </p>
              </div>
              <div>
                <p className="text-[var(--color-ink-subtle)]">Publisher</p>
                <p className="truncate text-[var(--color-ink)]">{pending.publisher ?? "Unknown"}</p>
              </div>
            </div>
          ) : null
        }
        whatHappens="The process is stopped immediately. Anything it had open and unsaved is lost."
        whatIsUntouched="Nothing is uninstalled or deleted. The program can be started again normally."
        extra={
          pending?.risk === "system_component" ? (
            <div className="flex gap-2 rounded-md border border-[color-mix(in_srgb,var(--color-warn)_35%,transparent)] bg-[var(--color-warn-soft)] p-2.5">
              <ShieldAlert className="mt-0.5 size-3.5 shrink-0 text-[var(--color-warn)]" />
              <p className="text-2xs leading-relaxed text-[var(--color-ink)]">
                This is part of Windows. Ending it can make the desktop, the Start menu or the
                taskbar disappear until Windows restarts it or you sign out and back in.
              </p>
            </div>
          ) : settings?.require_confirmation_for_processes === false ? null : null
        }
      />
    </div>
  );
}
