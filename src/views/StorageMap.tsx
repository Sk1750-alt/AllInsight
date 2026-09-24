/**
 * The storage map.
 *
 * A squarified treemap of one directory level at a time. The frontend never
 * holds the whole tree: drilling down asks the backend for exactly the level
 * about to be drawn, which is what keeps this usable on a volume with millions
 * of files.
 */
import * as React from "react";
import { ChevronRight, FolderOpen, HardDrive, Play, RefreshCw, Square } from "lucide-react";

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
  Skeleton,
} from "@/components/ui/primitives";
import { Select } from "@/components/ui/controls";
import { categoryColor, ProgressBar, StackedBar } from "@/components/ui/data";
import { Tooltip } from "@/components/ui/overlay";
import { formatBytes, formatCount, formatMilliseconds, formatRelative } from "@/lib/format";
import { cn, usageTone } from "@/lib/utils";
import type { ScanSummary, StorageOverview, TreemapNode } from "@/lib/types";

/**
 * Squarified treemap layout.
 *
 * The classic Bruls, Huizing and van Wijk algorithm: lay children out in rows,
 * choosing the orientation that keeps the worst aspect ratio in the row as
 * close to square as possible. Squares are far easier to compare by eye than
 * the long slivers a naive slice-and-dice layout produces.
 */
interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

interface LaidOut extends Rect {
  node: TreemapNode;
}

function worstRatio(row: number[], length: number, total: number): number {
  if (row.length === 0 || length === 0) return Infinity;
  const sum = row.reduce((a, b) => a + b, 0);
  if (sum === 0) return Infinity;
  const scale = total / (length * length);
  const max = Math.max(...row);
  const min = Math.min(...row);
  return Math.max((scale * max) / (sum * sum) * length * length, (sum * sum) / (scale * min * length * length));
}

function squarify(nodes: TreemapNode[], rect: Rect): LaidOut[] {
  const values = nodes.map((n) => Math.max(n.size_bytes, 1));
  const total = values.reduce((a, b) => a + b, 0);
  if (total === 0) return [];

  const area = rect.width * rect.height;
  const scaled = values.map((v) => (v / total) * area);

  const result: LaidOut[] = [];
  let index = 0;
  let current = { ...rect };

  while (index < nodes.length) {
    const vertical = current.width >= current.height;
    const length = vertical ? current.height : current.width;

    const row: number[] = [];
    let rowEnd = index;
    while (rowEnd < nodes.length) {
      const candidate = [...row, scaled[rowEnd]];
      if (
        row.length > 0 &&
        worstRatio(candidate, length, 1) > worstRatio(row, length, 1)
      ) {
        break;
      }
      row.push(scaled[rowEnd]);
      rowEnd += 1;
    }

    const rowSum = row.reduce((a, b) => a + b, 0);
    const thickness = length > 0 ? rowSum / length : 0;

    let offset = 0;
    for (let i = 0; i < row.length; i += 1) {
      const share = rowSum > 0 ? row[i] / rowSum : 0;
      const span = share * length;
      result.push({
        node: nodes[index + i],
        x: vertical ? current.x : current.x + offset,
        y: vertical ? current.y + offset : current.y,
        width: vertical ? thickness : span,
        height: vertical ? span : thickness,
      });
      offset += span;
    }

    if (vertical) {
      current = {
        x: current.x + thickness,
        y: current.y,
        width: Math.max(0, current.width - thickness),
        height: current.height,
      };
    } else {
      current = {
        x: current.x,
        y: current.y + thickness,
        width: current.width,
        height: Math.max(0, current.height - thickness),
      };
    }

    index = rowEnd;
    if (current.width <= 0.5 || current.height <= 0.5) break;
  }

  return result;
}

function Treemap({
  nodes,
  onDrill,
  onSelect,
  selected,
}: {
  nodes: TreemapNode[];
  onDrill: (node: TreemapNode) => void;
  onSelect: (node: TreemapNode | null) => void;
  selected: TreemapNode | null;
}) {
  const containerRef = React.useRef<HTMLDivElement>(null);
  const [size, setSize] = React.useState({ width: 800, height: 440 });

  React.useEffect(() => {
    const element = containerRef.current;
    if (!element) return;
    const observer = new ResizeObserver(([entry]) => {
      setSize({
        width: Math.max(200, entry.contentRect.width),
        height: Math.max(200, entry.contentRect.height),
      });
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  // Below a certain share a tile is unreadable, so the tail is folded into a
  // single "smaller items" tile rather than drawn as invisible slivers.
  const prepared = React.useMemo(() => {
    const sorted = [...nodes].sort((a, b) => b.size_bytes - a.size_bytes);
    const total = sorted.reduce((sum, n) => sum + n.size_bytes, 0) || 1;
    const visible: TreemapNode[] = [];
    let remainder = 0;
    let remainderCount = 0;

    for (const node of sorted) {
      if (node.size_bytes / total >= 0.004 && visible.length < 90) {
        visible.push(node);
      } else {
        remainder += node.size_bytes;
        remainderCount += 1;
      }
    }
    if (remainder > 0) {
      visible.push({
        path: "",
        name: `${remainderCount} smaller items`,
        size_bytes: remainder,
        share: remainder / total,
        file_count: 0,
        has_children: false,
        is_file_bucket: true,
      });
    }
    return visible;
  }, [nodes]);

  const laidOut = React.useMemo(
    () => squarify(prepared, { x: 0, y: 0, width: size.width, height: size.height }),
    [prepared, size.width, size.height],
  );

  return (
    <div ref={containerRef} className="relative h-[440px] w-full overflow-hidden rounded-md">
      {laidOut.map((tile, index) => {
        const readable = tile.width > 56 && tile.height > 28;
        const isSelected = selected?.path === tile.node.path && !!tile.node.path;
        return (
          <Tooltip
            key={`${tile.node.path}-${index}`}
            content={
              <div>
                <p className="font-medium">{tile.node.name}</p>
                <p className="numeric mt-0.5 text-[var(--color-ink-muted)]">
                  {formatBytes(tile.node.size_bytes)}
                  {tile.node.file_count > 0
                    ? ` · ${formatCount(tile.node.file_count)} files`
                    : ""}
                </p>
                {tile.node.has_children ? (
                  <p className="mt-1 text-2xs text-[var(--color-ink-subtle)]">
                    Double-click to open
                  </p>
                ) : null}
              </div>
            }
          >
            <button
              onClick={() => onSelect(tile.node.path ? tile.node : null)}
              onDoubleClick={() => tile.node.has_children && onDrill(tile.node)}
              className={cn(
                "absolute overflow-hidden border text-left transition-quick",
                isSelected
                  ? "border-[var(--color-accent)] z-10"
                  : "border-[var(--color-canvas)] hover:border-[var(--color-ink-subtle)]",
              )}
              style={{
                left: tile.x,
                top: tile.y,
                width: Math.max(0, tile.width - 1),
                height: Math.max(0, tile.height - 1),
                background: tile.node.is_file_bucket
                  ? "var(--color-surface-hover)"
                  : categoryColor(index),
                opacity: tile.node.is_file_bucket ? 0.75 : 0.88,
              }}
            >
              {readable ? (
                <span className="pointer-events-none block p-1.5">
                  <span className="block truncate text-2xs font-medium text-white/95 drop-shadow-sm">
                    {tile.node.name}
                  </span>
                  <span className="numeric block truncate text-2xs text-white/75">
                    {formatBytes(tile.node.size_bytes)}
                  </span>
                </span>
              ) : null}
            </button>
          </Tooltip>
        );
      })}
    </div>
  );
}

export function StorageMapView() {
  const { scanGeneration, scanning, toast, reportError } = useStore();
  const w = usePlatformWords();

  const overview = useAsync<StorageOverview>(() => api.getStorageOverview(), []);
  const [root, setRoot] = React.useState<string>("");
  const [path, setPath] = React.useState<string>("");
  const [selected, setSelected] = React.useState<TreemapNode | null>(null);

  // Default to the volume that holds Windows, since that is the one that fills.
  React.useEffect(() => {
    if (!root && overview.data) {
      const preferred =
        overview.data.system_volume ??
        overview.data.volumes.find((v) => v.kind === "fixed")?.mount_point ??
        "";
      setRoot(preferred);
      setPath(preferred);
    }
  }, [overview.data, root]);

  const summary = useAsync<ScanSummary | null>(
    async () => (root ? api.getScanSummary(root) : null),
    [root, scanGeneration],
  );

  const level = useAsync<TreemapNode[]>(
    async () => (root && path ? api.getTreemapLevel(root, path) : []),
    [root, path, scanGeneration],
  );

  const startScan = async () => {
    try {
      setPath(root);
      setSelected(null);
      await api.scanDirectory(root);
      toast({
        tone: "info",
        title: `Scanning ${root}`,
        body: "You can keep using AllInsight while this runs.",
      });
    } catch (e) {
      reportError(e, "The scan could not be started.");
    }
  };

  const crumbs = React.useMemo(() => {
    if (!root || !path) return [];
    const relative = path.slice(root.length).split("\\").filter(Boolean);
    const out = [{ label: root, path: root }];
    let current = root.replace(/\\$/, "");
    for (const part of relative) {
      current = `${current}\\${part}`;
      out.push({ label: part, path: current });
    }
    return out;
  }, [root, path]);

  const volumeOptions = (overview.data?.volumes ?? [])
    .filter((v) => v.is_ready && v.kind !== "network")
    .map((v) => ({
      value: v.mount_point,
      label: `${v.letter}${v.label ? ` ${v.label}` : ""}`,
      hint: `${formatBytes(v.free_bytes)} free`,
    }));

  const currentVolume = overview.data?.volumes.find((v) => v.mount_point === root);

  return (
    <div className="view-enter space-y-5">
      <PageHeader
        title="Storage Map"
        subtitle="See exactly where space is going, one folder at a time."
        actions={
          <>
            {volumeOptions.length > 0 ? (
              <Select
                label="Drive"
                value={root}
                onValueChange={(next) => {
                  setRoot(next);
                  setPath(next);
                  setSelected(null);
                }}
                options={volumeOptions}
                className="min-w-40"
              />
            ) : null}
            {scanning ? (
              <Button
                variant="secondary"
                icon={<Square className="size-3" />}
                onClick={() => api.cancelScan()}
              >
                Stop
              </Button>
            ) : (
              <Button
                variant="primary"
                icon={<Play className="size-3.5" />}
                onClick={startScan}
                disabled={!root}
              >
                {summary.data ? "Rescan" : "Scan this drive"}
              </Button>
            )}
          </>
        }
      />

      {currentVolume ? (
        <Panel>
          <div className="flex flex-wrap items-center gap-6 p-4">
            <div className="min-w-44">
              <p className="text-2xs uppercase tracking-wider text-[var(--color-ink-subtle)]">
                {currentVolume.letter} capacity
              </p>
              <p className="readout mt-1 text-[26px] text-[var(--color-ink)]">
                {formatBytes(currentVolume.used_bytes)}
                <span className="text-sm font-normal text-[var(--color-ink-subtle)]">
                  {" "}
                  of {formatBytes(currentVolume.total_bytes)}
                </span>
              </p>
            </div>
            <div className="min-w-0 flex-1">
              <ProgressBar
                value={currentVolume.used_percent}
                tone={usageTone(currentVolume.used_percent)}
                height={8}
                label="Volume usage"
              />
              <p className="mt-1.5 text-2xs text-[var(--color-ink-subtle)]">
                {formatBytes(currentVolume.free_bytes)} free ·{" "}
                {currentVolume.filesystem ?? "Unknown filesystem"}
              </p>
            </div>
            {summary.data ? (
              <div className="text-right">
                <p className="text-2xs uppercase tracking-wider text-[var(--color-ink-subtle)]">
                  Last scan
                </p>
                <p className="mt-1 text-xs text-[var(--color-ink)]">
                  {formatRelative(summary.data.completed_at)}
                </p>
                <p className="text-2xs text-[var(--color-ink-subtle)]">
                  {formatCount(summary.data.total_files)} files in{" "}
                  {formatMilliseconds(summary.data.duration_ms)}
                </p>
              </div>
            ) : null}
          </div>
        </Panel>
      ) : null}

      {!summary.data && !summary.loading ? (
        <Panel>
          <EmptyState
            icon={<HardDrive className="size-5" />}
            title="This drive has not been analysed yet"
            description="Scanning reads file sizes only. It never opens a file, and it changes nothing."
            action={
              <Button variant="primary" onClick={startScan} disabled={!root || scanning}>
                Scan {root}
              </Button>
            }
          />
        </Panel>
      ) : (
        <>
          {/* Category breakdown across the whole scan. */}
          {summary.data && summary.data.categories.length > 0 ? (
            <Panel>
              <PanelHeader
                title="What is using the space"
                description={`${formatBytes(summary.data.total_bytes)} across ${formatCount(summary.data.total_files)} files`}
                actions={
                  summary.data.skipped_dirs > 0 ? (
                    <Tooltip content={w.isWindows ? "These folders could not be opened without administrator permission. Their contents are not included in the totals." : "These folders could not be opened, or are system, memory or network filesystems AllInsight does not walk. Their contents are not included in the totals."}>
                      <span>
                        <Badge tone="neutral">
                          {formatCount(summary.data.skipped_dirs)} folders skipped
                        </Badge>
                      </span>
                    </Tooltip>
                  ) : null
                }
              />
              <div className="space-y-3 p-4">
                <StackedBar
                  height={10}
                  segments={summary.data.categories.slice(0, 10).map((c, i) => ({
                    label: c.label,
                    value: c.bytes,
                    color: categoryColor(i),
                  }))}
                />
                <div className="grid gap-x-6 gap-y-1.5 sm:grid-cols-2 lg:grid-cols-3">
                  {summary.data.categories.slice(0, 9).map((category, index) => (
                    <div key={category.category} className="flex items-center gap-2 text-xs">
                      <span
                        className="size-2 shrink-0 rounded-sm"
                        style={{ background: categoryColor(index) }}
                        aria-hidden
                      />
                      <span className="truncate text-[var(--color-ink-muted)]">
                        {category.label}
                      </span>
                      <span className="numeric ml-auto text-[var(--color-ink)]">
                        {formatBytes(category.bytes)}
                      </span>
                    </div>
                  ))}
                </div>
              </div>
            </Panel>
          ) : null}

          <Panel>
            <PanelHeader
              title="Folder map"
              description="Click a tile to see its details. Double-click to open it."
              actions={
                <Button
                  size="sm"
                  variant="ghost"
                  icon={<RefreshCw className="size-3.5" />}
                  onClick={() => level.reload()}
                >
                  Refresh
                </Button>
              }
            />

            <div className="flex flex-wrap items-center gap-1 border-b border-[var(--color-line)] px-4 py-2 text-xs">
              {crumbs.map((crumb, index) => (
                <React.Fragment key={crumb.path}>
                  {index > 0 ? (
                    <ChevronRight className="size-3 text-[var(--color-ink-subtle)]" />
                  ) : null}
                  <button
                    onClick={() => {
                      setPath(crumb.path);
                      setSelected(null);
                    }}
                    className={cn(
                      "rounded px-1.5 py-0.5 transition-quick",
                      index === crumbs.length - 1
                        ? "font-medium text-[var(--color-ink)]"
                        : "text-[var(--color-ink-muted)] hover:bg-[var(--color-surface-hover)]",
                    )}
                  >
                    {crumb.label}
                  </button>
                </React.Fragment>
              ))}
            </div>

            <div className="p-4">
              {level.loading ? (
                <Skeleton className="h-[440px]" />
              ) : (level.data?.length ?? 0) === 0 ? (
                <EmptyState
                  icon={<FolderOpen className="size-5" />}
                  title="Nothing to show for this folder"
                  description="It is empty, it could not be read, or the scan stopped before reaching it."
                />
              ) : (
                <Treemap
                  nodes={level.data!}
                  selected={selected}
                  onSelect={setSelected}
                  onDrill={(node) => {
                    setPath(node.path);
                    setSelected(null);
                  }}
                />
              )}
            </div>

            {selected ? (
              <div className="flex flex-wrap items-center justify-between gap-3 border-t border-[var(--color-line)] px-4 py-3">
                <div className="min-w-0">
                  <p className="truncate text-xs font-medium text-[var(--color-ink)]">
                    {selected.name}
                  </p>
                  <p data-selectable className="truncate text-2xs text-[var(--color-ink-subtle)]">
                    {selected.path}
                  </p>
                </div>
                <div className="flex items-center gap-3">
                  <span className="numeric text-xs text-[var(--color-ink)]">
                    {formatBytes(selected.size_bytes)}
                  </span>
                  {selected.file_count > 0 ? (
                    <span className="numeric text-2xs text-[var(--color-ink-subtle)]">
                      {formatCount(selected.file_count)} files
                    </span>
                  ) : null}
                  <Button
                    size="sm"
                    variant="ghost"
                    icon={<FolderOpen className="size-3.5" />}
                    onClick={() =>
                      api.showInExplorer(selected.path).catch((e) => reportError(e))
                    }
                  >
                    {w.showInFolder}
                  </Button>
                  {selected.has_children ? (
                    <Button size="sm" variant="subtle" onClick={() => setPath(selected.path)}>
                      Open
                    </Button>
                  ) : null}
                </div>
              </div>
            ) : null}
          </Panel>

          <Hint>
            Sizes come from the last completed scan. Links and junctions are counted once, where
            the real data lives, so the totals do not double-count.
          </Hint>
        </>
      )}
    </div>
  );
}
