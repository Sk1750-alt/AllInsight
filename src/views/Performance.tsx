/**
 * Performance.
 *
 * Live CPU, memory, GPU, disk and network, with the history AllInsight has kept
 * since it started. Where a counter is not published by the hardware, the card
 * says so rather than drawing a flat line at zero.
 */
import * as React from "react";
import { Cpu, Gauge, HardDrive, MemoryStick, Network } from "lucide-react";

import { api } from "@/lib/api";
import { usePolled, useStore } from "@/app/store";
import {
  Badge,
  EmptyState,
  Hint,
  KeyValue,
  Panel,
  PanelHeader,
  PageHeader,
  Skeleton,
} from "@/components/ui/primitives";
import { ProgressBar, Sparkline } from "@/components/ui/data";
import { formatBytes, formatDuration, formatPercent, formatRate } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { ProcessList } from "@/lib/types";

function MetricPanel({
  title,
  icon,
  now,
  detail,
  values,
  max = 100,
  color = "var(--color-accent)",
  formatValue,
  footer,
  unavailable,
}: {
  title: string;
  icon: React.ReactNode;
  now: string;
  detail?: string;
  values: number[];
  max?: number;
  color?: string;
  formatValue?: (v: number) => string;
  footer?: React.ReactNode;
  unavailable?: string;
}) {
  return (
    <Panel>
      <div className="flex items-start justify-between gap-4 px-4 pt-3">
        <div>
          <p className="flex items-center gap-1.5 text-2xs font-semibold uppercase tracking-wider text-[var(--color-ink-subtle)]">
            {icon}
            {title}
          </p>
          <p className="numeric mt-1.5 font-display text-2xl font-semibold text-[var(--color-ink)]">
            {now}
          </p>
          {detail ? (
            <p className="mt-0.5 text-xs text-[var(--color-ink-muted)]">{detail}</p>
          ) : null}
        </div>
      </div>

      <div className="px-4 pb-3 pt-2">
        {unavailable ? (
          <p className="py-4 text-center text-2xs text-[var(--color-ink-subtle)]">{unavailable}</p>
        ) : (
          <Sparkline values={values} max={max} color={color} height={56} formatValue={formatValue} />
        )}
      </div>

      {footer ? (
        <div className="border-t border-[var(--color-line)] px-4 py-2.5">{footer}</div>
      ) : null}
    </Panel>
  );
}

export function PerformanceView() {
  const { navigate } = useStore();
  const snapshot = usePolled(() => api.getSystemSummary(), 1500);
  const history = usePolled(() => api.getMetricsHistory(), 3000);
  const processes = usePolled<ProcessList>(() => api.getProcesses(6, false), 4000);

  if (!snapshot || !history) {
    return (
      <div className="view-enter space-y-5">
        <PageHeader title="Performance" />
        <div className="grid gap-4 lg:grid-cols-2">
          <Skeleton className="h-52" />
          <Skeleton className="h-52" />
          <Skeleton className="h-52" />
          <Skeleton className="h-52" />
        </div>
      </div>
    );
  }

  const samples = history.samples;
  const netMax = Math.max(
    1,
    ...samples.map((s) => Math.max(s.net_down_bps, s.net_up_bps)),
  );
  const diskMax = Math.max(
    1,
    ...samples.map((s) => Math.max(s.disk_read_bps, s.disk_write_bps)),
  );

  return (
    <div className="view-enter space-y-5">
      <PageHeader
        title="Performance"
        subtitle={`${snapshot.cpu.brand} · ${snapshot.os_name}`}
        actions={
          <Badge tone="neutral">
            {samples.length} samples since AllInsight started
          </Badge>
        }
      />

      <div className="grid gap-4 lg:grid-cols-2">
        <MetricPanel
          title="Processor"
          icon={<Cpu className="size-3.5" />}
          now={formatPercent(snapshot.cpu.usage_percent)}
          detail={`${snapshot.cpu.logical_cores} logical cores${
            snapshot.cpu.physical_cores ? ` · ${snapshot.cpu.physical_cores} physical` : ""
          }`}
          values={samples.map((s) => s.cpu_percent)}
          footer={
            <div className="grid grid-cols-3 gap-2 text-2xs">
              <div>
                <p className="text-[var(--color-ink-subtle)]">Average</p>
                <p className="numeric text-[var(--color-ink)]">
                  {formatPercent(history.cpu_average)}
                </p>
              </div>
              <div>
                <p className="text-[var(--color-ink-subtle)]">Peak</p>
                <p className="numeric text-[var(--color-ink)]">{formatPercent(history.cpu_peak)}</p>
              </div>
              <div>
                <p className="text-[var(--color-ink-subtle)]">Clock</p>
                <p className="numeric text-[var(--color-ink)]">
                  {snapshot.cpu.frequency_mhz > 0
                    ? `${(snapshot.cpu.frequency_mhz / 1000).toFixed(1)} GHz`
                    : "Not reported"}
                </p>
              </div>
            </div>
          }
        />

        <MetricPanel
          title="Memory"
          icon={<MemoryStick className="size-3.5" />}
          now={`${formatBytes(snapshot.memory.used_bytes)} / ${formatBytes(snapshot.memory.total_bytes)}`}
          detail={`${formatPercent(snapshot.memory.used_percent)} committed`}
          values={samples.map((s) => s.memory_percent)}
          color="#5c8ed6"
          footer={
            <div className="grid grid-cols-3 gap-2 text-2xs">
              <div>
                <p className="text-[var(--color-ink-subtle)]">Available</p>
                <p className="numeric text-[var(--color-ink)]">
                  {formatBytes(snapshot.memory.available_bytes)}
                </p>
              </div>
              <div>
                <p className="text-[var(--color-ink-subtle)]">Peak</p>
                <p className="numeric text-[var(--color-ink)]">
                  {formatPercent(history.memory_peak)}
                </p>
              </div>
              <div>
                <p className="text-[var(--color-ink-subtle)]">Paging file</p>
                <p className="numeric text-[var(--color-ink)]">
                  {snapshot.memory.swap_total_bytes > 0
                    ? `${formatBytes(snapshot.memory.swap_used_bytes)} used`
                    : "None"}
                </p>
              </div>
            </div>
          }
        />

        <MetricPanel
          title="Graphics"
          icon={<Gauge className="size-3.5" />}
          now={
            snapshot.gpu.utilization_percent !== null
              ? formatPercent(snapshot.gpu.utilization_percent)
              : "Not available"
          }
          detail={snapshot.gpu.adapters[0]?.name}
          values={samples.map((s) => s.gpu_percent ?? 0)}
          color="#8a7fd4"
          unavailable={
            snapshot.gpu.utilization_percent === null
              ? (snapshot.gpu.note ??
                "This device does not publish GPU utilisation counters to Windows.")
              : undefined
          }
          footer={
            snapshot.gpu.adapters.length > 0 ? (
              <div className="space-y-1">
                {snapshot.gpu.adapters.map((adapter) => (
                  <div key={adapter.name} className="flex justify-between text-2xs">
                    <span className="truncate text-[var(--color-ink-muted)]">{adapter.name}</span>
                    <span className="numeric shrink-0 text-[var(--color-ink-subtle)]">
                      {adapter.driver_version ?? "Driver unknown"}
                    </span>
                  </div>
                ))}
              </div>
            ) : null
          }
        />

        <MetricPanel
          title="Disk activity"
          icon={<HardDrive className="size-3.5" />}
          now={formatRate(snapshot.disk.read_bytes_per_sec + snapshot.disk.write_bytes_per_sec)}
          detail={`${formatRate(snapshot.disk.read_bytes_per_sec)} read · ${formatRate(snapshot.disk.write_bytes_per_sec)} write`}
          values={samples.map((s) => s.disk_read_bps + s.disk_write_bps)}
          max={diskMax}
          color="#c9a227"
          formatValue={formatRate}
        />

        <MetricPanel
          title="Network"
          icon={<Network className="size-3.5" />}
          now={formatRate(snapshot.network.download_bytes_per_sec)}
          detail={`${formatRate(snapshot.network.upload_bytes_per_sec)} upload · ${snapshot.network.interfaces} interfaces`}
          values={samples.map((s) => s.net_down_bps)}
          max={netMax}
          color="#78a860"
          formatValue={formatRate}
          footer={
            <div className="grid grid-cols-2 gap-2 text-2xs">
              <div>
                <p className="text-[var(--color-ink-subtle)]">Received since boot</p>
                <p className="numeric text-[var(--color-ink)]">
                  {formatBytes(snapshot.network.total_received_bytes)}
                </p>
              </div>
              <div>
                <p className="text-[var(--color-ink-subtle)]">Sent since boot</p>
                <p className="numeric text-[var(--color-ink)]">
                  {formatBytes(snapshot.network.total_transmitted_bytes)}
                </p>
              </div>
            </div>
          }
        />

        <Panel>
          <PanelHeader
            title="Using the most right now"
            description="Live from the process list."
          />
          {!processes ? (
            <div className="space-y-2 p-4">
              <Skeleton className="h-6" />
              <Skeleton className="h-6" />
              <Skeleton className="h-6" />
            </div>
          ) : processes.processes.length === 0 ? (
            <EmptyState title="No processes reported" />
          ) : (
            <div className="divide-y divide-[var(--color-line)]">
              {processes.processes.map((process) => (
                <button
                  key={process.pid}
                  onClick={() => navigate("processes")}
                  className="flex w-full items-center gap-3 px-4 py-2 text-left transition-quick hover:bg-[var(--color-surface-hover)]"
                >
                  <span className="min-w-0 flex-1 truncate text-xs text-[var(--color-ink)]">
                    {process.name}
                  </span>
                  <span className="w-24">
                    <ProgressBar value={process.cpu_percent} tone="accent" height={4} />
                  </span>
                  <span className="numeric w-12 shrink-0 text-right text-2xs text-[var(--color-ink-muted)]">
                    {formatPercent(process.cpu_percent)}
                  </span>
                  <span className="numeric w-16 shrink-0 text-right text-2xs text-[var(--color-ink-subtle)]">
                    {formatBytes(process.memory_bytes)}
                  </span>
                </button>
              ))}
            </div>
          )}
        </Panel>
      </div>

      <div className="grid gap-4 sm:grid-cols-3">
        <Panel>
          <PanelHeader title="System" />
          <dl className="px-4 pb-3">
            <KeyValue label="Uptime" value={formatDuration(snapshot.uptime_seconds)} />
            <KeyValue label="Processes" value={snapshot.process_count} />
            <KeyValue label="Device name" value={snapshot.host_name || "Unknown"} />
          </dl>
        </Panel>

        <Panel className="sm:col-span-2">
          <PanelHeader title="Per-core usage" />
          <div className="flex flex-wrap gap-1.5 p-4">
            {snapshot.cpu.core_usage.map((usage, index) => (
              <div key={index} className="w-[calc(12.5%-6px)] min-w-14">
                <div className="flex items-baseline justify-between">
                  <span className="text-2xs text-[var(--color-ink-subtle)]">{index}</span>
                  <span
                    className={cn(
                      "numeric text-2xs",
                      usage > 80 ? "text-[var(--color-warn)]" : "text-[var(--color-ink-muted)]",
                    )}
                  >
                    {usage.toFixed(0)}
                  </span>
                </div>
                <ProgressBar
                  value={usage}
                  tone={usage > 80 ? "warn" : "accent"}
                  height={4}
                  label={`Core ${index}`}
                />
              </div>
            ))}
          </div>
        </Panel>
      </div>

      <Hint>
        History is kept in memory for this session only and is never written to disk.
      </Hint>
    </div>
  );
}
