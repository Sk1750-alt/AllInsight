/**
 * Drive health.
 *
 * The screen where honesty matters most. A drive that does not report its
 * reliability counters is shown as Unknown, with an explanation and an offer
 * to elevate, and never as Healthy. A green badge here has to mean the drive
 * actually said so.
 */
import * as React from "react";
import { HardDrive, Info, RefreshCw, ShieldAlert, ShieldCheck, ShieldQuestion } from "lucide-react";

import { api } from "@/lib/api";
import { useAsync, useStore } from "@/app/store";
import {
  Badge,
  Button,
  EmptyState,
  Hint,
  KeyValue,
  Panel,
  PanelHeader,
  PageHeader,
  Skeleton,
} from "@/components/ui/primitives";
import { ProgressBar } from "@/components/ui/data";
import { formatBytes, formatCount, formatDuration } from "@/lib/format";
import { cn, healthTone, toneClasses } from "@/lib/utils";
import type { DriveHealth, DriveHealthReport, HealthState } from "@/lib/types";

const STATE_ICON: Record<HealthState, React.ComponentType<{ className?: string }>> = {
  healthy: ShieldCheck,
  warning: ShieldAlert,
  critical: ShieldAlert,
  unknown: ShieldQuestion,
};

const STATE_LABEL: Record<HealthState, string> = {
  healthy: "Healthy",
  warning: "Warning",
  critical: "Critical",
  unknown: "Unknown",
};

const MEDIA_LABEL: Record<string, string> = {
  ssd: "Solid state",
  hdd: "Hard disk",
  storage_class_memory: "Storage class memory",
  unspecified: "Unspecified",
};

function DriveCard({ drive, onElevate }: { drive: DriveHealth; onElevate: () => void }) {
  const tone = healthTone(drive.state);
  const t = toneClasses(tone);
  const Icon = STATE_ICON[drive.state];

  return (
    <Panel>
      <div className="flex items-start justify-between gap-4 px-4 py-3.5 hairline">
        <div className="flex min-w-0 items-start gap-3">
          <span className={cn("mt-0.5 flex size-8 items-center justify-center rounded-md", t.bg, t.text)}>
            <Icon className="size-4" />
          </span>
          <div className="min-w-0">
            <p className="truncate text-sm font-semibold text-[var(--color-ink)]">{drive.model}</p>
            <p className="mt-0.5 text-2xs text-[var(--color-ink-muted)]">
              {MEDIA_LABEL[drive.media] ?? drive.media} · {drive.bus}
              {drive.size_bytes ? ` · ${formatBytes(drive.size_bytes)}` : ""}
              {drive.volumes.length > 0 ? ` · ${drive.volumes.join(", ")}` : ""}
            </p>
          </div>
        </div>
        <Badge tone={tone} dot>
          {STATE_LABEL[drive.state]}
        </Badge>
      </div>

      {drive.notes.length > 0 ? (
        <div className="space-y-1.5 border-b border-[var(--color-line)] px-4 py-3">
          {drive.notes.map((note) => (
            <p key={note} className="text-xs leading-relaxed text-[var(--color-ink-muted)]">
              {note}
            </p>
          ))}
          {drive.elevation_would_help ? (
            <Button size="sm" variant="subtle" className="mt-1" onClick={onElevate}>
              Restart as administrator
            </Button>
          ) : null}
        </div>
      ) : null}

      {drive.estimated_life_remaining_percent !== null ? (
        <div className="border-b border-[var(--color-line)] px-4 py-3">
          <div className="flex items-baseline justify-between">
            <span className="text-2xs uppercase tracking-wider text-[var(--color-ink-subtle)]">
              Rated life remaining
            </span>
            <span className="numeric text-sm font-semibold text-[var(--color-ink)]">
              {drive.estimated_life_remaining_percent}%
            </span>
          </div>
          <ProgressBar
            className="mt-2"
            value={drive.estimated_life_remaining_percent}
            tone={
              drive.estimated_life_remaining_percent > 50
                ? "ok"
                : drive.estimated_life_remaining_percent > 20
                  ? "warn"
                  : "danger"
            }
            label="Rated life remaining"
          />
          <p className="mt-1.5 text-2xs text-[var(--color-ink-subtle)]">
            From the drive's own wear counter, not an estimate by AllInsight.
          </p>
        </div>
      ) : null}

      {(() => {
        // A row the drive does not report is left out rather than printed as a
        // column of "Not reported". Built as data first so the missing names
        // can be collected without mutating anything mid-render.
        const left: Array<[string, string | null]> = [
          [
            "Temperature",
            drive.temperature_celsius !== null ? `${drive.temperature_celsius} °C` : null,
          ],
          [
            "Highest temperature",
            drive.temperature_max_celsius !== null
              ? `${drive.temperature_max_celsius} °C`
              : null,
          ],
          [
            "Power-on hours",
            drive.power_on_hours !== null
              ? `${formatCount(drive.power_on_hours)} h (${formatDuration(drive.power_on_hours * 3600)})`
              : null,
          ],
          [
            "Start/stop cycles",
            drive.start_stop_cycles !== null ? formatCount(drive.start_stop_cycles) : null,
          ],
          [
            "Spindle speed",
            drive.spindle_speed_rpm ? `${formatCount(drive.spindle_speed_rpm)} rpm` : null,
          ],
        ];

        const right: Array<[string, string | null]> = [
          [
            "Read errors",
            drive.read_errors_total !== null
              ? `${formatCount(drive.read_errors_total)} total, ${formatCount(drive.read_errors_uncorrected ?? 0)} uncorrected`
              : null,
          ],
          [
            "Write errors",
            drive.write_errors_total !== null
              ? `${formatCount(drive.write_errors_total)} total, ${formatCount(drive.write_errors_uncorrected ?? 0)} uncorrected`
              : null,
          ],
          ["Windows status", drive.windows_health ?? null],
          ["Operational", drive.operational_status.join(", ") || null],
          ["Firmware", drive.firmware ?? null],
          ["Serial", drive.serial_number ?? null],
        ];

        const shown = (rows: Array<[string, string | null]>) =>
          rows
            .filter((row): row is [string, string] => row[1] !== null)
            .map(([label, value]) => <KeyValue key={label} label={label} value={value} />);

        // Spindle speed is absent on every solid-state drive by definition, so
        // naming it as unreported would be noise rather than information.
        const missing = [...left, ...right]
          .filter(([label, value]) => value === null && label !== "Spindle speed")
          .map(([label]) => label.toLowerCase());

        return (
          <>
            <div className="grid gap-x-6 px-4 py-2 sm:grid-cols-2">
              <dl>{shown(left)}</dl>
              <dl>{shown(right)}</dl>
            </div>

            {missing.length > 0 ? (
              <p className="px-4 pb-3 text-2xs text-[var(--color-ink-subtle)]">
                This drive does not report {missing.join(", ")}.
              </p>
            ) : null}
          </>
        );
      })()}
    </Panel>
  );
}

export function DriveHealthView() {
  const { reportError } = useStore();
  const { data, loading, error, reload } = useAsync<DriveHealthReport>(
    () => api.getDriveHealth(),
    [],
  );

  const elevate = () => api.restartElevated().catch((e) => reportError(e));

  return (
    <div className="view-enter space-y-5">
      <PageHeader
        title="Drive Health"
        subtitle="What the hardware itself reports, and nothing more."
        actions={
          <>
            {data && !data.elevated ? (
              <Button variant="secondary" onClick={elevate}>
                Restart as administrator
              </Button>
            ) : null}
            <Button variant="ghost" icon={<RefreshCw className="size-3.5" />} onClick={reload}>
              Refresh
            </Button>
          </>
        }
      />

      {loading ? (
        <div className="space-y-4">
          <Skeleton className="h-64" />
          <Skeleton className="h-64" />
        </div>
      ) : error ? (
        <Panel>
          <EmptyState title="Drive health could not be read" description={error} />
        </Panel>
      ) : (data?.drives.length ?? 0) === 0 ? (
        <Panel>
          <EmptyState
            icon={<HardDrive className="size-5" />}
            title="No drives reported"
            description={
              data?.error ??
              "Windows did not return any physical drives. This can happen in a virtual machine or with an unusual storage driver."
            }
          />
        </Panel>
      ) : (
        <div className="space-y-4">
          {data!.drives.map((drive) => (
            <DriveCard key={drive.device_id} drive={drive} onElevate={elevate} />
          ))}
        </div>
      )}

      <Panel>
        <PanelHeader title="How AllInsight decides" />
        <div className="space-y-2 p-4 text-xs leading-relaxed text-[var(--color-ink-muted)]">
          <p>
            Model, bus and capacity come from the Windows storage service and are always
            available. Wear, temperature, power-on hours and error counts come from the drive's
            reliability counters, which most drives only expose to an elevated process.
          </p>
          <p>
            When those counters cannot be read, the status is{" "}
            <span className="text-[var(--color-ink)]">Unknown</span> rather than Healthy. Windows
            reporting no complaint is not the same as the drive reporting it is well, and AllInsight
            will not present one as the other.
          </p>
          <p>
            An uncorrected read or write error, a predictive failure, or heavy wear always
            downgrades the status, whatever else is true.
          </p>
        </div>
      </Panel>

      <div className="flex gap-2 rounded-md border border-[var(--color-line)] bg-[var(--color-surface)] p-3">
        <Info className="mt-0.5 size-3.5 shrink-0 text-[var(--color-ink-subtle)]" />
        <Hint>
          No health figure here is estimated, interpolated or filled in. If a drive does not
          report a value, AllInsight says so.
        </Hint>
      </div>
    </div>
  );
}
