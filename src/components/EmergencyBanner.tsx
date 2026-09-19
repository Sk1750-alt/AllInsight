/**
 * The critical storage banner.
 *
 * Appears only when a drive is genuinely at risk of failing to function -
 * below five percent free, where Windows updates stop applying and
 * applications start failing to save. It offers the safe actions and nothing
 * else: no "fix it now" button, because there is no single safe action that
 * reclaims that much space without a decision from the user.
 */
import { AlertOctagon, ChevronRight, X } from "lucide-react";
import * as React from "react";

import { usePolled, useStore } from "@/app/store";
import { api } from "@/lib/api";
import { Button } from "./ui/primitives";
import { formatBytes } from "@/lib/format";
import type { StorageOverview } from "@/lib/types";

const CRITICAL_FREE_PERCENT = 5;

export function EmergencyBanner() {
  const { navigate } = useStore();
  const [dismissed, setDismissed] = React.useState<string[]>([]);
  const storage = usePolled<StorageOverview>(() => api.getStorageOverview(), 60_000);

  const critical = (storage?.volumes ?? []).filter((volume) => {
    if (!volume.is_ready || !volume.kind || volume.kind === "network") return false;
    if (volume.total_bytes === 0) return false;
    const freePercent = (volume.free_bytes / volume.total_bytes) * 100;
    return freePercent < CRITICAL_FREE_PERCENT && !dismissed.includes(volume.mount_point);
  });

  if (critical.length === 0) return null;
  const volume = critical[0];
  const name = volume.letter;

  return (
    <div
      role="alert"
      className="border-b border-[color-mix(in_srgb,var(--color-danger)_45%,transparent)] bg-[var(--color-danger-soft)]"
    >
      <div className="mx-auto flex w-full max-w-[1400px] items-center gap-4 px-6 py-2.5">
        <AlertOctagon className="size-4 shrink-0 text-[var(--color-danger)]" />

        <div className="min-w-0 flex-1">
          <p className="text-xs font-semibold text-[var(--color-ink)]">
            Critical storage shortage on {name}
          </p>
          <p className="text-2xs text-[var(--color-ink-muted)]">
            Only {formatBytes(volume.free_bytes)} of {formatBytes(volume.total_bytes)} remains.
            Windows needs free space to update and to page memory, and applications can fail to
            save below this point.
          </p>
        </div>

        <div className="flex shrink-0 items-center gap-2">
          <Button
            size="sm"
            variant="danger"
            icon={<ChevronRight className="size-3.5" />}
            onClick={() => navigate("cleanup")}
          >
            Clean temporary files
          </Button>
          <Button size="sm" variant="subtle" onClick={() => navigate("large-files")}>
            Review large files
          </Button>
          <Button size="sm" variant="ghost" onClick={() => navigate("storage-map")}>
            Open storage map
          </Button>
          <Button
            size="sm"
            variant="ghost"
            aria-label="Dismiss for this session"
            icon={<X className="size-3.5" />}
            onClick={() => setDismissed((current) => [...current, volume.mount_point])}
          />
        </div>
      </div>
    </div>
  );
}
