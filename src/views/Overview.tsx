/**
 * The Overview.
 *
 * One screen that answers "is this machine alright?" without the user having
 * to know which of the other screens to open. The score, the four vital signs,
 * the recommendations and the local explanation - and every number on it comes
 * from a measurement, with "not available" shown where one is missing.
 */
import * as React from "react";
import {
  AlertTriangle,
  ArrowRight,
  BatteryMedium,
  CircleCheck,
  Cpu,
  HardDrive,
  Info,
  Lightbulb,
  RefreshCw,
  ShieldAlert,
  Sparkles,
} from "lucide-react";

import { usePlatformWords } from "@/lib/platform";
import { api } from "@/lib/api";
import { useAsync, usePolled, useStore } from "@/app/store";
import { routeForAction } from "@/app/navigation";
import { Badge, Button, EmptyState, Hint, Panel, PanelHeader, Skeleton } from "@/components/ui/primitives";
import { ProgressBar, ScoreRing, Sparkline } from "@/components/ui/data";
import { formatBytes, formatDuration, formatPercent, formatRate } from "@/lib/format";
import { cn, healthTone, severityTone, toneClasses, usageTone } from "@/lib/utils";
import type { DashboardSnapshot, Insight, Severity } from "@/lib/types";

const SEVERITY_ICON: Record<Severity, React.ComponentType<{ className?: string }>> = {
  critical: ShieldAlert,
  warning: AlertTriangle,
  advice: Lightbulb,
  positive: CircleCheck,
  neutral: Info,
};

function VitalCard({
  title,
  value,
  detail,
  tone,
  progress,
  footer,
  icon,
  onClick,
}: {
  title: string;
  value: React.ReactNode;
  detail?: React.ReactNode;
  tone?: "ok" | "warn" | "danger" | "unknown" | "accent" | "neutral";
  progress?: number;
  footer?: React.ReactNode;
  icon: React.ReactNode;
  onClick?: () => void;
}) {
  const t = toneClasses(tone ?? "neutral");
  return (
    <button
      onClick={onClick}
      disabled={!onClick}
      className={cn(
        "panel flex w-full flex-col gap-3 p-4 text-left transition-quick",
        onClick && "hover:border-[var(--color-line-strong)] hover:bg-[var(--color-surface-raised)]",
      )}
    >
      <div className="flex items-center justify-between">
        <span className="flex items-center gap-1.5 text-2xs font-semibold uppercase tracking-wider text-[var(--color-ink-subtle)]">
          {icon}
          {title}
        </span>
        {onClick ? <ArrowRight className="size-3.5 text-[var(--color-ink-subtle)]" /> : null}
      </div>

      <div>
        {/* Numbers are readouts; a word such as "Healthy" is a state, and is
            set at text size rather than dressed up as a measurement. */}
        <div
          className={cn(
            /\d/.test(String(value))
              ? "readout text-[30px] leading-none"
              : "font-display text-[19px] font-semibold leading-tight tracking-[-0.01em]",
            t.text,
          )}
        >
          {value}
        </div>
        {detail ? (
          <div className="mt-1.5 text-xs text-[var(--color-ink-muted)]">{detail}</div>
        ) : null}
      </div>

      {progress !== undefined ? <ProgressBar value={progress} tone={tone ?? "accent"} /> : null}
      {footer ? <div className="text-2xs text-[var(--color-ink-subtle)]">{footer}</div> : null}
    </button>
  );
}

function InsightRow({ insight, onAct }: { insight: Insight; onAct: () => void }) {
  const tone = severityTone(insight.severity);
  const t = toneClasses(tone);
  const Icon = SEVERITY_ICON[insight.severity];
  const route = routeForAction(insight.action);

  return (
    <div className="flex items-start gap-3 px-4 py-3">
      <span
        className={cn(
          "mt-0.5 flex size-6 shrink-0 items-center justify-center rounded-md",
          t.bg,
          t.text,
        )}
      >
        <Icon className="size-3.5" />
      </span>

      <div className="min-w-0 flex-1">
        <div className="flex items-baseline gap-2">
          <p className="text-xs font-medium text-[var(--color-ink)]">{insight.title}</p>
          {insight.value ? (
            <span className={cn("numeric shrink-0 text-2xs font-semibold", t.text)}>
              {insight.value}
            </span>
          ) : null}
        </div>
        <p className="mt-1 text-xs leading-relaxed text-[var(--color-ink-muted)]">{insight.body}</p>
      </div>

      {route && insight.action_label ? (
        <Button size="sm" variant="subtle" className="shrink-0" onClick={onAct}>
          {insight.action_label}
        </Button>
      ) : null}
    </div>
  );
}

export function OverviewView() {
  const { navigate, scanGeneration, toast } = useStore();
  const w = usePlatformWords();
  const { data, loading, error, reload } = useAsync<DashboardSnapshot>(
    () => api.getDashboard(),
    [scanGeneration],
  );

  // Live metrics refresh independently of the heavier dashboard payload.
  const live = usePolled(() => api.getSystemSummary(), 2000, !loading);
  const history = usePolled(() => api.getMetricsHistory(), 4000, !loading);
  const [measuring, setMeasuring] = React.useState(false);

  const system = live ?? data?.system ?? null;
  const storage = data?.storage;
  const systemVolume = storage?.volumes.find((v) => v.is_system) ?? storage?.volumes[0];
  const worstDrive = data?.drives.drives.reduce<(typeof data.drives.drives)[number] | null>(
    (worst, drive) => {
      const rank = { critical: 0, warning: 1, unknown: 2, healthy: 3 } as const;
      if (!worst) return drive;
      return rank[drive.state] < rank[worst.state] ? drive : worst;
    },
    null,
  );

  const measureCleanup = async () => {
    setMeasuring(true);
    try {
      await api.getCleanupCandidates();
      reload();
    } catch (e) {
      toast({ tone: "error", title: e instanceof Error ? e.message : "Measurement failed." });
    } finally {
      setMeasuring(false);
    }
  };

  if (error) {
    return (
      <Panel>
        <EmptyState
          icon={<AlertTriangle className="size-5" />}
          title="The overview could not be loaded"
          description={error}
          action={
            <Button icon={<RefreshCw className="size-3.5" />} onClick={reload}>
              Try again
            </Button>
          }
        />
      </Panel>
    );
  }

  return (
    <div className="view-enter space-y-5">
      {/* Headline: score on the left, the four vital signs on the right. */}
      <div className="grid gap-5 lg:grid-cols-[300px_1fr]">
        <Panel className="flex flex-col items-center justify-center gap-4 p-6">
          <div className="text-center">
            <p className="text-2xs font-semibold uppercase tracking-[0.2em] text-[var(--color-ink-subtle)]">
              Device health
            </p>
          </div>

          {loading || !data ? (
            <Skeleton className="size-[132px] rounded-full" />
          ) : (
            <ScoreRing
              score={data.score.score}
              label={data.score.label}
              partial={data.score.partial}
            />
          )}

          <div className="w-full space-y-1.5">
            {(data?.score.reasons ?? []).slice(0, 3).map((reason) => (
              <p
                key={reason}
                className="text-center text-xs leading-relaxed text-[var(--color-ink-muted)]"
              >
                {reason}
              </p>
            ))}
            {data?.score.partial ? (
              <p className="pt-1 text-center text-2xs text-[var(--color-ink-subtle)]">
                Some checks could not be measured, so this score covers less than the full picture.
              </p>
            ) : null}
          </div>
        </Panel>

        <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
          <VitalCard
            icon={<HardDrive className="size-3.5" />}
            title="Storage"
            tone={systemVolume ? usageTone(systemVolume.used_percent) : "unknown"}
            value={systemVolume ? formatPercent(systemVolume.used_percent) : "-"}
            detail={
              systemVolume
                ? `${formatBytes(systemVolume.free_bytes)} free on ${systemVolume.letter}`
                : "No volume reported"
            }
            progress={systemVolume?.used_percent ?? 0}
            onClick={() => navigate("storage-map")}
          />

          <VitalCard
            icon={<Cpu className="size-3.5" />}
            title="Performance"
            tone="accent"
            value={system ? formatPercent(system.cpu.usage_percent) : "-"}
            detail={
              system
                ? `${formatBytes(system.memory.used_bytes)} of ${formatBytes(system.memory.total_bytes)} memory in use`
                : undefined
            }
            footer={
              history ? (
                <Sparkline
                  values={history.samples.map((s) => s.cpu_percent)}
                  max={100}
                  height={28}
                />
              ) : undefined
            }
            onClick={() => navigate("performance")}
          />

          <VitalCard
            icon={<HardDrive className="size-3.5" />}
            title="Drive health"
            tone={worstDrive ? healthTone(worstDrive.state) : "unknown"}
            value={
              worstDrive
                ? worstDrive.state === "healthy"
                  ? "Healthy"
                  : worstDrive.state === "unknown"
                    ? "Unknown"
                    : worstDrive.state === "warning"
                      ? "Warning"
                      : "Critical"
                : "Unknown"
            }
            detail={
              !worstDrive
                ? "No drive reported"
                : worstDrive.estimated_life_remaining_percent !== null
                  ? `${worstDrive.estimated_life_remaining_percent}% of rated life remaining`
                  : // "Unknown" on its own reads as a fault. Say which kind it
                    // is: Windows withholding the data from a standard user is
                    // one click from being fixed, a drive that publishes
                    // nothing is not.
                    worstDrive.elevation_would_help
                    ? "Needs administrator permission to read"
                    : worstDrive.reliability_unavailable
                      ? "This drive does not report detailed health"
                      : worstDrive.model
            }
            footer={
              worstDrive?.elevation_would_help ? (
                <span className="text-[var(--color-accent)]">Restart as administrator</span>
              ) : (
                worstDrive?.model
              )
            }
            onClick={() => navigate("drive-health")}
          />

          <VitalCard
            icon={<BatteryMedium className="size-3.5" />}
            title="Battery"
            tone={
              !data?.battery.present
                ? "unknown"
                : data.battery.health_percent === null
                  ? "unknown"
                  : data.battery.health_percent >= 80
                    ? "ok"
                    : data.battery.health_percent >= 60
                      ? "warn"
                      : "danger"
            }
            value={
              !data?.battery.present
                ? "No battery"
                : data.battery.charge_percent !== null
                  ? `${data.battery.charge_percent}%`
                  : "-"
            }
            detail={
              !data?.battery.present
                ? "This device runs on mains power"
                : data.battery.health_percent !== null
                  ? `${data.battery.health_percent}% of design capacity`
                  : "Health not reported by this device"
            }
            footer={
              data?.battery.present && data.battery.runtime_seconds
                ? `About ${formatDuration(data.battery.runtime_seconds)} remaining`
                : undefined
            }
            onClick={() => navigate("battery")}
          />
        </div>
      </div>

      {/* Recommendations and the local explanation. */}
      <div className="grid gap-5 lg:grid-cols-[1fr_360px]">
        <Panel>
          <PanelHeader
            title="Recommended actions"
            description="Every item below is backed by something AllInsight measured on this device."
            actions={
              <Button
                size="sm"
                variant="ghost"
                icon={<RefreshCw className="size-3.5" />}
                onClick={reload}
              >
                Refresh
              </Button>
            }
          />
          {loading && !data ? (
            <div className="space-y-3 p-4">
              <Skeleton className="h-12" />
              <Skeleton className="h-12" />
              <Skeleton className="h-12" />
            </div>
          ) : (data?.insights.length ?? 0) === 0 ? (
            <EmptyState
              icon={<CircleCheck className="size-5" />}
              title="Nothing needs your attention"
              description="AllInsight found no problems in the areas it could measure."
            />
          ) : (
            <div className="divide-y divide-[var(--color-line)]">
              {data!.insights.slice(0, 6).map((insight) => (
                <InsightRow
                  key={insight.id}
                  insight={insight}
                  onAct={() => {
                    const route = routeForAction(insight.action);
                    if (route) navigate(route);
                  }}
                />
              ))}
            </div>
          )}
        </Panel>

        <div className="space-y-5">
          <Panel>
            <PanelHeader
              title="Cleanup"
              description={
                data?.scanned || data?.reclaimable_bytes
                  ? undefined
                  : "Not measured yet."
              }
            />
            <div className="space-y-3 p-4">
              <div>
                <p className="text-2xs uppercase tracking-wider text-[var(--color-ink-subtle)]">
                  Potentially reclaimable
                </p>
                {data?.reclaimable_bytes ? (
                  <p className="readout mt-1 text-[30px] text-[var(--color-accent)]">
                    {formatBytes(data.reclaimable_bytes)}
                  </p>
                ) : (
                  <p className="mt-1 text-[15px] font-medium text-[var(--color-ink-muted)]">Not measured</p>
                )}
              </div>
              <div className="flex gap-2">
                <Button
                  size="sm"
                  variant={data?.reclaimable_bytes ? "primary" : "secondary"}
                  onClick={() => navigate("cleanup")}
                >
                  Open cleanup
                </Button>
                {!data?.reclaimable_bytes ? (
                  <Button size="sm" variant="ghost" loading={measuring} onClick={measureCleanup}>
                    Measure now
                  </Button>
                ) : null}
              </div>
              <Hint>
                Only temporary and cached data is included. Documents, downloads and media are
                never part of a cleanup.
              </Hint>
            </div>
          </Panel>

          <Panel>
            <PanelHeader
              title="AllInsight AI"
              actions={
                <Badge tone={data?.summary.from_model ? "accent" : "neutral"} dot>
                  {data?.summary.from_model ? "Local model" : "Measurements"}
                </Badge>
              }
            />
            <div className="space-y-3 p-4">
              {loading && !data ? (
                <>
                  <Skeleton className="h-3" />
                  <Skeleton className="h-3 w-4/5" />
                  <Skeleton className="h-3 w-3/5" />
                </>
              ) : (
                <p
                  data-selectable
                  className="whitespace-pre-line text-xs leading-relaxed text-[var(--color-ink)]"
                >
                  {data?.summary.text}
                </p>
              )}

              <div className="flex flex-wrap gap-2">
                {(data?.summary.actions ?? []).map((action) => {
                  const route = routeForAction(action.action);
                  if (!route) return null;
                  return (
                    <Button
                      key={action.label}
                      size="sm"
                      variant="subtle"
                      onClick={() => navigate(route)}
                    >
                      {action.label}
                    </Button>
                  );
                })}
              </div>

              <Button
                size="sm"
                variant="ghost"
                icon={<Sparkles className="size-3.5" />}
                onClick={() => navigate("assistant")}
              >
                Ask a question
              </Button>
            </div>
          </Panel>

          {system ? (
            <Panel>
              <PanelHeader title="Right now" />
              <div className="grid grid-cols-2 gap-x-4 gap-y-2.5 p-4 text-xs">
                <div>
                  <p className="text-2xs text-[var(--color-ink-subtle)]">Download</p>
                  <p className="numeric mt-0.5 text-[var(--color-ink)]">
                    {formatRate(system.network.download_bytes_per_sec)}
                  </p>
                </div>
                <div>
                  <p className="text-2xs text-[var(--color-ink-subtle)]">Upload</p>
                  <p className="numeric mt-0.5 text-[var(--color-ink)]">
                    {formatRate(system.network.upload_bytes_per_sec)}
                  </p>
                </div>
                <div>
                  <p className="text-2xs text-[var(--color-ink-subtle)]">Disk read</p>
                  <p className="numeric mt-0.5 text-[var(--color-ink)]">
                    {formatRate(system.disk.read_bytes_per_sec)}
                  </p>
                </div>
                <div>
                  <p className="text-2xs text-[var(--color-ink-subtle)]">Disk write</p>
                  <p className="numeric mt-0.5 text-[var(--color-ink)]">
                    {formatRate(system.disk.write_bytes_per_sec)}
                  </p>
                </div>
                <div>
                  <p className="text-2xs text-[var(--color-ink-subtle)]">GPU</p>
                  <p className="numeric mt-0.5 text-[var(--color-ink)]">
                    {system.gpu.utilization_percent !== null
                      ? formatPercent(system.gpu.utilization_percent)
                      : "Not reported"}
                  </p>
                </div>
                <div>
                  <p className="text-2xs text-[var(--color-ink-subtle)]">Uptime</p>
                  <p className="numeric mt-0.5 text-[var(--color-ink)]">
                    {formatDuration(system.uptime_seconds)}
                  </p>
                </div>
              </div>
            </Panel>
          ) : null}
        </div>
      </div>

      {/* Every volume, not just the system one. */}
      <Panel>
        <PanelHeader
          title="Drives"
          description="Removable and network drives are shown but excluded from the totals."
        />
        <div className="divide-y divide-[var(--color-line)]">
          {(storage?.volumes ?? []).map((volume) => (
            <div key={volume.mount_point} className="flex items-center gap-4 px-4 py-3">
              <div className="w-40 shrink-0">
                <p className="text-xs font-medium text-[var(--color-ink)]">
                  {volume.letter}
                  {volume.label ? (
                    <span className="ml-1.5 font-normal text-[var(--color-ink-muted)]">
                      {volume.label}
                    </span>
                  ) : null}
                </p>
                <p className="text-2xs text-[var(--color-ink-subtle)]">
                  {volume.filesystem ?? "Unknown filesystem"} · {volume.kind.replace("_", " ")}
                  {volume.is_system ? ` · ${w.os}` : ""}
                </p>
              </div>

              <div className="min-w-0 flex-1">
                <ProgressBar
                  value={volume.used_percent}
                  tone={usageTone(volume.used_percent)}
                  label={`${volume.letter} usage`}
                />
              </div>

              <div className="numeric w-56 shrink-0 text-right text-xs">
                {volume.is_ready ? (
                  <>
                    <span className="text-[var(--color-ink)]">
                      {formatBytes(volume.used_bytes)}
                    </span>
                    <span className="text-[var(--color-ink-subtle)]">
                      {" "}
                      of {formatBytes(volume.total_bytes)} used
                    </span>
                  </>
                ) : (
                  <span className="text-[var(--color-ink-subtle)]">No media</span>
                )}
              </div>
            </div>
          ))}
        </div>
      </Panel>
    </div>
  );
}
