/**
 * Large files.
 *
 * A review screen, not a cleanup screen. Everything here belongs to the user,
 * so nothing is pre-selected, removal always goes to the Recycle Bin (Trash), and
 * anything on the protected list is shown with the reason rather than hidden.
 */
import * as React from "react";
import {
  FileWarning,
  FolderOpen,
  Play,
  Search,
  ShieldCheck,
  Square,
  Trash2,
} from "lucide-react";

import { usePlatformWords } from "@/lib/platform";
import { api } from "@/lib/api";
import { useAsync, useStore } from "@/app/store";
import {
  Badge,
  Button,
  EmptyState,
  Hint,
  Panel,
  PanelHeader,
  PageHeader,
} from "@/components/ui/primitives";
import { SearchInput, Select } from "@/components/ui/controls";
import { ConfirmDialog, Tooltip } from "@/components/ui/overlay";
import { Table, Td, Th, Tr } from "@/components/ui/data";
import { formatBytes, formatCount, formatDate, shortenPath } from "@/lib/format";
import type { LargeFileEntry, LargeFileReport, StorageOverview } from "@/lib/types";

const THRESHOLDS = [
  { value: "104857600", label: "Over 100 MB" },
  { value: "524288000", label: "Over 500 MB" },
  { value: "1073741824", label: "Over 1 GB" },
  { value: "5368709120", label: "Over 5 GB" },
  { value: "10737418240", label: "Over 10 GB" },
];

type SortKey = "size" | "modified" | "name";

export function LargeFilesView() {
  const { scanning, scanGeneration, toast, reportError, settings } = useStore();
  const overview = useAsync<StorageOverview>(() => api.getStorageOverview(), []);
  const report = useAsync<LargeFileReport | null>(() => api.getLargeFiles(), [scanGeneration]);
  const w = usePlatformWords();

  // Seeded from Settings, so the preference there is the one that applies.
  const [threshold, setThreshold] = React.useState(() =>
    String(settings?.large_file_threshold_bytes ?? 1_073_741_824),
  );
  const [root, setRoot] = React.useState<string>("");
  const [query, setQuery] = React.useState("");
  const [sort, setSort] = React.useState<SortKey>("size");
  const [pending, setPending] = React.useState<LargeFileEntry | null>(null);
  const [removing, setRemoving] = React.useState(false);

  React.useEffect(() => {
    if (!root && overview.data) {
      setRoot("__home__");
    }
  }, [overview.data, root]);

  const rootOptions = React.useMemo(
    () => [
      { value: "__home__", label: "Your user folder", hint: "Recommended" },
      ...(overview.data?.volumes ?? [])
        .filter((v) => v.is_ready && v.kind !== "network")
        .map((v) => ({
          value: v.mount_point,
          label: `All of ${v.letter}`,
          hint: `${formatBytes(v.free_bytes)} free`,
        })),
    ],
    [overview.data],
  );

  const startSearch = async () => {
    try {
      const roots = root === "__home__" ? [] : [root];
      await api.findLargeFiles(roots, Number(threshold));
      toast({ tone: "info", title: "Searching for large files" });
    } catch (e) {
      reportError(e, "The search could not be started.");
    }
  };

  const rows = React.useMemo(() => {
    const entries = report.data?.entries ?? [];
    const q = query.trim().toLowerCase();
    const filtered = q
      ? entries.filter(
          (e) => e.name.toLowerCase().includes(q) || e.path.toLowerCase().includes(q),
        )
      : entries;
    const sorted = [...filtered];
    if (sort === "size") sorted.sort((a, b) => b.size_bytes - a.size_bytes);
    if (sort === "modified") sorted.sort((a, b) => (b.modified ?? 0) - (a.modified ?? 0));
    if (sort === "name") sorted.sort((a, b) => a.name.localeCompare(b.name));
    return sorted;
  }, [report.data, query, sort]);

  const remove = async () => {
    if (!pending) return;
    setRemoving(true);
    try {
      await api.recycleReviewedFile(pending.path);
      toast({
        tone: "success",
        title: `Moved to the ${w.trash}`,
        body: `${pending.name} can be restored from there.`,
      });
      setPending(null);
      report.reload();
    } catch (e) {
      reportError(e, `That file could not be moved to the ${w.trash}.`);
    } finally {
      setRemoving(false);
    }
  };

  return (
    <div className="view-enter space-y-5">
      <PageHeader
        title="Large Files"
        subtitle="The biggest files on this device, so you can decide what is still worth keeping."
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
              Find large files
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
            value={threshold}
            onValueChange={setThreshold}
            options={THRESHOLDS}
            className="min-w-36"
          />
          <SearchInput
            value={query}
            onChange={setQuery}
            placeholder="Filter by name or folder"
            className="min-w-56 flex-1"
          />
          <Select
            label="Sort by"
            value={sort}
            onValueChange={(v) => setSort(v as SortKey)}
            options={[
              { value: "size", label: "Largest first" },
              { value: "modified", label: "Recently changed" },
              { value: "name", label: "Name" },
            ]}
            className="min-w-36"
          />
        </div>
      </Panel>

      {!report.data ? (
        <Panel>
          <EmptyState
            icon={<Search className="size-5" />}
            title="No search has run yet"
            description="Choose where to look and a minimum size, then start the search. AllInsight reads file sizes only; it never opens a file."
            action={
              <Button variant="primary" onClick={startSearch} disabled={scanning}>
                Find large files
              </Button>
            }
          />
        </Panel>
      ) : rows.length === 0 ? (
        <Panel>
          <EmptyState
            icon={<ShieldCheck className="size-5" />}
            title={query ? "Nothing matched that filter" : "No files that large were found"}
            description={
              query
                ? "Try a different name, or clear the filter."
                : "Try a smaller minimum size, or search a whole drive instead of your user folder."
            }
          />
        </Panel>
      ) : (
        <Panel>
          <PanelHeader
            title={`${formatCount(rows.length)} files`}
            description={`${formatBytes(rows.reduce((sum, r) => sum + r.size_bytes, 0))} in total${
              report.data.truncated ? " · only the largest are listed" : ""
            }`}
          />
          <div className="max-h-[calc(100vh-380px)] overflow-y-auto">
            <Table>
              <thead>
                <tr>
                  <Th>Name</Th>
                  <Th>Location</Th>
                  <Th align="right">Size</Th>
                  <Th align="right">Modified</Th>
                  <Th>Risk</Th>
                  <Th align="right">Actions</Th>
                </tr>
              </thead>
              <tbody>
                {rows.map((entry) => (
                  <Tr key={entry.path}>
                    <Td className="max-w-[240px]">
                      <span className="block truncate font-medium" title={entry.name}>
                        {entry.name}
                      </span>
                    </Td>
                    <Td className="max-w-[320px]">
                      <span
                        data-selectable
                        className="block truncate text-[var(--color-ink-muted)]"
                        title={entry.directory}
                      >
                        {shortenPath(entry.directory, 46)}
                      </span>
                    </Td>
                    <Td align="right" className="font-medium">
                      {formatBytes(entry.size_bytes)}
                    </Td>
                    <Td align="right" className="text-[var(--color-ink-muted)]">
                      {formatDate(entry.modified)}
                    </Td>
                    <Td>
                      {entry.risk === "protected" ? (
                        <Tooltip content={entry.risk_note ?? "This item is protected."}>
                          <span>
                            <Badge tone="unknown">Protected</Badge>
                          </span>
                        </Tooltip>
                      ) : entry.risk === "low" ? (
                        <Badge tone="ok">Low risk</Badge>
                      ) : (
                        <Badge tone="neutral">Review</Badge>
                      )}
                    </Td>
                    <Td align="right">
                      <div className="flex justify-end gap-1">
                        <Tooltip content={w.showInFolder}>
                          <span>
                            <Button
                              size="sm"
                              variant="ghost"
                              onClick={() =>
                                api.showInExplorer(entry.path).catch((e) => reportError(e))
                              }
                              icon={<FolderOpen className="size-3.5" />}
                            />
                          </span>
                        </Tooltip>
                        <Tooltip
                          content={
                            entry.risk === "protected"
                              ? "AllInsight will not remove a protected file."
                              : `Move to the ${w.trash}`
                          }
                        >
                          <span>
                            <Button
                              size="sm"
                              variant="ghost"
                              disabled={entry.risk === "protected"}
                              onClick={() => setPending(entry)}
                              icon={<Trash2 className="size-3.5" />}
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
        </Panel>
      )}

      <Hint>
        AllInsight never removes anything from this screen on its own. Files you remove here go to the{" "}
        {w.trash}, so they can be restored.
      </Hint>

      <ConfirmDialog
        open={!!pending}
        onOpenChange={(open) => !open && setPending(null)}
        title={`Move ${pending?.name ?? ""} to the ${w.trash}?`}
        loading={removing}
        confirmLabel={`Move to ${w.trash}`}
        onConfirm={remove}
        estimate={
          pending ? (
            <div className="flex items-baseline justify-between">
              <span className="text-2xs uppercase tracking-wider text-[var(--color-ink-subtle)]">
                Space recovered
              </span>
              <span className="numeric font-display text-lg font-semibold text-[var(--color-accent)]">
                {formatBytes(pending.size_bytes)}
              </span>
            </div>
          ) : null
        }
        whatHappens={`The file is moved to the ${w.trash}. It stays there, and stays recoverable, until you empty it.`}
        whatIsUntouched="Nothing else. Only this one file is moved."
        extra={
          pending ? (
            <div className="flex gap-2 rounded-md border border-[var(--color-line)] p-2.5">
              <FileWarning className="mt-0.5 size-3.5 shrink-0 text-[var(--color-ink-subtle)]" />
              <p data-selectable className="break-all font-mono text-2xs text-[var(--color-ink-muted)]">
                {pending.path}
              </p>
            </div>
          ) : null
        }
      />
    </div>
  );
}
