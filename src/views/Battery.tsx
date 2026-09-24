/**
 * Battery.
 *
 * Charge comes from Windows and is always available. Health is the ratio of
 * full-charge capacity to design capacity, and is shown only when the hardware
 * reported both figures. A laptop that reports neither gets an explanation,
 * not a plausible-looking percentage.
 */
import { BatteryCharging, BatteryMedium, Info, Plug, RefreshCw } from "lucide-react";

import { usePlatformWords } from "@/lib/platform";
import { api } from "@/lib/api";
import { useAsync, usePolled } from "@/app/store";
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
import { formatCount, formatDuration } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { BatteryStatus } from "@/lib/types";

function healthTone(health: number | null): "ok" | "warn" | "danger" | "unknown" {
  if (health === null) return "unknown";
  if (health >= 80) return "ok";
  if (health >= 60) return "warn";
  return "danger";
}

export function BatteryView() {
  const initial = useAsync<BatteryStatus>(() => api.getBatteryStatus(), []);
  const live = usePolled<BatteryStatus>(() => api.getBatteryStatus(), 20000, !initial.loading);
  const battery = live ?? initial.data;
  const w = usePlatformWords();

  if (initial.loading && !battery) {
    return (
      <div className="view-enter space-y-5">
        <PageHeader title="Battery" />
        <Skeleton className="h-56" />
      </div>
    );
  }

  if (!battery || !battery.present) {
    return (
      <div className="view-enter space-y-5">
        <PageHeader title="Battery" subtitle="Power and battery condition." />
        <Panel>
          <EmptyState
            icon={<Plug className="size-5" />}
            title="No battery is installed in this device"
            description={
              battery?.notes[0] ??
              "AllInsight found no battery. This is expected on a desktop or on a machine running from mains power only."
            }
          />
        </Panel>
      </div>
    );
  }

  const tone = healthTone(battery.health_percent);
  const charging = battery.charging || battery.power_source === "ac_power";

  return (
    <div className="view-enter space-y-5">
      <PageHeader
        title="Battery"
        subtitle={
          battery.manufacturer
            ? `${battery.manufacturer}${battery.chemistry ? ` · ${battery.chemistry}` : ""}`
            : "Power and battery condition."
        }
        actions={
          <Button
            variant="ghost"
            icon={<RefreshCw className="size-3.5" />}
            onClick={initial.reload}
          >
            Refresh
          </Button>
        }
      />

      <div className="grid gap-4 lg:grid-cols-3">
        <Panel className="lg:col-span-2">
          <div className="p-5">
            <div className="flex items-end justify-between gap-6">
              <div>
                <p className="flex items-center gap-1.5 text-2xs font-semibold uppercase tracking-wider text-[var(--color-ink-subtle)]">
                  {charging ? (
                    <BatteryCharging className="size-3.5" />
                  ) : (
                    <BatteryMedium className="size-3.5" />
                  )}
                  Current charge
                </p>
                <p className="readout mt-2 text-[48px] text-[var(--color-ink)]">
                  {battery.charge_percent !== null ? `${battery.charge_percent}%` : "Unknown"}
                </p>
                <p className="mt-1 text-xs text-[var(--color-ink-muted)]">
                  {charging
                    ? battery.charging
                      ? "Charging"
                      : "Running on mains power"
                    : battery.runtime_seconds
                      ? `About ${formatDuration(battery.runtime_seconds)} remaining`
                      : "Running on battery"}
                </p>
              </div>

              <Badge tone={charging ? "accent" : "neutral"} dot>
                {battery.power_source === "ac_power"
                  ? "Plugged in"
                  : battery.power_source === "battery"
                    ? "On battery"
                    : "Unknown source"}
              </Badge>
            </div>

            <ProgressBar
              className="mt-4"
              height={10}
              value={battery.charge_percent ?? 0}
              tone={
                charging
                  ? "accent"
                  : (battery.charge_percent ?? 100) < 20
                    ? "danger"
                    : (battery.charge_percent ?? 100) < 40
                      ? "warn"
                      : "ok"
              }
              label="Charge level"
            />
          </div>
        </Panel>

        <Panel>
          <div className="flex h-full flex-col justify-center p-5">
            <p className="text-2xs font-semibold uppercase tracking-wider text-[var(--color-ink-subtle)]">
              Battery health
            </p>
            {battery.health_percent !== null ? (
              <>
                <p
                  className={cn(
                    "readout mt-2 text-[48px]",
                    tone === "ok"
                      ? "text-[var(--color-ok)]"
                      : tone === "warn"
                        ? "text-[var(--color-warn)]"
                        : "text-[var(--color-danger)]",
                  )}
                >
                  {battery.health_percent}%
                </p>
                <p className="mt-1.5 text-xs leading-relaxed text-[var(--color-ink-muted)]">
                  This battery currently holds approximately {battery.health_percent}% of its
                  original design capacity.
                </p>
              </>
            ) : (
              <>
                <p className="readout mt-2 text-[30px] text-[var(--color-unknown)]">
                  Not available
                </p>
                <p className="mt-1.5 text-xs leading-relaxed text-[var(--color-ink-muted)]">
                  This device does not report its design capacity to {w.os}, so health cannot be
                  calculated. AllInsight will not estimate it.
                </p>
              </>
            )}
          </div>
        </Panel>
      </div>

      <Panel>
        <PanelHeader
          title="Details"
          description={`Everything the battery firmware reports through ${w.os}.`}
        />
        <div className="grid gap-x-8 px-4 py-2 sm:grid-cols-2">
          <dl>
            <KeyValue
              label="Design capacity"
              value={
                battery.design_capacity_mwh
                  ? `${formatCount(battery.design_capacity_mwh)} mWh`
                  : "Not reported"
              }
            />
            <KeyValue
              label="Full charge capacity"
              value={
                battery.full_charge_capacity_mwh
                  ? `${formatCount(battery.full_charge_capacity_mwh)} mWh`
                  : "Not reported"
              }
            />
            <KeyValue
              label="Remaining capacity"
              value={
                battery.remaining_capacity_mwh
                  ? `${formatCount(battery.remaining_capacity_mwh)} mWh`
                  : "Not reported"
              }
            />
          </dl>
          <dl>
            <KeyValue
              label="Charge cycles"
              value={battery.cycle_count ? formatCount(battery.cycle_count) : "Not reported"}
            />
            <KeyValue
              label="Voltage"
              value={battery.voltage_mv ? `${formatCount(battery.voltage_mv)} mV` : "Not reported"}
            />
            <KeyValue label="Chemistry" value={battery.chemistry ?? "Not reported"} />
          </dl>
        </div>
      </Panel>

      {battery.notes.length > 0 ? (
        <Panel>
          <PanelHeader title="Notes" />
          <div className="space-y-1.5 p-4">
            {battery.notes.map((note) => (
              <p key={note} className="text-xs leading-relaxed text-[var(--color-ink-muted)]">
                {note}
              </p>
            ))}
          </div>
        </Panel>
      ) : null}

      <div className="flex gap-2 rounded-md border border-[var(--color-line)] bg-[var(--color-surface)] p-3">
        <Info className="mt-0.5 size-3.5 shrink-0 text-[var(--color-ink-subtle)]" />
        <Hint>
          Health is full-charge capacity divided by design capacity, both read from the battery
          itself. Cycle counts are reported by some manufacturers and omitted by others.
        </Hint>
      </div>
    </div>
  );
}
