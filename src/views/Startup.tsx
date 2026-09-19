/**
 * The startup manager.
 *
 * Disabling writes the same approval value Task Manager writes, so a change
 * made here shows up there and can be undone from either place. Nothing is
 * ever deleted.
 */
import * as React from "react";
import { FolderOpen, Info, Lock, Power, RefreshCw } from "lucide-react";

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
import { SearchInput, Switch } from "@/components/ui/controls";
import { Tooltip } from "@/components/ui/overlay";
import { Table, Td, Th, Tr } from "@/components/ui/data";
import { formatCount, shortenPath } from "@/lib/format";
import type { StartupImpact, StartupList } from "@/lib/types";

const IMPACT_TONE: Record<StartupImpact, "ok" | "warn" | "danger" | "unknown"> = {
  low: "ok",
  medium: "warn",
  high: "danger",
  unknown: "unknown",
};

const IMPACT_LABEL: Record<StartupImpact, string> = {
  low: "Low",
  medium: "Medium",
  high: "High",
  unknown: "Unknown",
};

export function StartupView() {
  const { toast, reportError } = useStore();
  const { data, loading, error, reload } = useAsync<StartupList>(() => api.getStartupItems(), []);
  const [query, setQuery] = React.useState("");
  const [busy, setBusy] = React.useState<string | null>(null);

  const rows = React.useMemo(() => {
    const items = data?.items ?? [];
    const q = query.trim().toLowerCase();
    return q
      ? items.filter(
          (i) =>
            i.name.toLowerCase().includes(q) ||
            (i.publisher ?? "").toLowerCase().includes(q) ||
            i.command.toLowerCase().includes(q),
        )
      : items;
  }, [data, query]);

  const toggle = async (id: string, enabled: boolean, name: string) => {
    setBusy(id);
    try {
      await api.setStartupEnabled(id, enabled);
      toast({
        tone: "success",
        title: enabled ? `${name} will start with Windows` : `${name} will no longer start with Windows`,
      });
      reload();
    } catch (e) {
      reportError(e, "That startup item could not be changed.");
    } finally {
      setBusy(null);
    }
  };

  const highImpact = (data?.items ?? []).filter((i) => i.enabled && i.impact === "high").length;

  return (
    <div className="view-enter space-y-5">
      <PageHeader
        title="Startup"
        subtitle="Programs that run when you sign in to Windows."
        actions={
          <Button variant="ghost" icon={<RefreshCw className="size-3.5" />} onClick={reload}>
            Refresh
          </Button>
        }
      />

      <div className="grid gap-4 sm:grid-cols-3">
        <Panel className="p-4">
          <p className="text-2xs uppercase tracking-wider text-[var(--color-ink-subtle)]">
            Enabled
          </p>
          <p className="numeric mt-1 font-display text-2xl font-semibold text-[var(--color-ink)]">
            {formatCount(data?.enabled_count ?? 0)}
          </p>
          <p className="text-xs text-[var(--color-ink-muted)]">
            of {formatCount(data?.items.length ?? 0)} entries
          </p>
        </Panel>

        <Panel className="p-4">
          <p className="text-2xs uppercase tracking-wider text-[var(--color-ink-subtle)]">
            Estimated high impact
          </p>
          <p className="numeric mt-1 font-display text-2xl font-semibold text-[var(--color-ink)]">
            {formatCount(highImpact)}
          </p>
          <p className="text-xs text-[var(--color-ink-muted)]">enabled and large on disk</p>
        </Panel>

        <Panel className="p-4">
          <p className="text-2xs uppercase tracking-wider text-[var(--color-ink-subtle)]">
            Permission
          </p>
          <p className="mt-1 font-display text-lg font-semibold text-[var(--color-ink)]">
            {data?.elevated ? "Administrator" : "Standard user"}
          </p>
          <p className="text-xs text-[var(--color-ink-muted)]">
            {data?.elevated
              ? "All entries can be changed"
              : "Machine-wide entries need administrator permission"}
          </p>
        </Panel>
      </div>

      <Panel>
        <div className="p-3">
          <SearchInput value={query} onChange={setQuery} placeholder="Filter startup entries" />
        </div>
      </Panel>

      <Panel>
        <PanelHeader
          title="Startup entries"
          description="From the Run registry keys and the Startup folders, the same places Windows reads."
        />

        {loading ? (
          <div className="space-y-2 p-4">
            {Array.from({ length: 5 }).map((_, i) => (
              <Skeleton key={i} className="h-10" />
            ))}
          </div>
        ) : error ? (
          <EmptyState title="Startup entries could not be read" description={error} />
        ) : rows.length === 0 ? (
          <EmptyState
            icon={<Power className="size-5" />}
            title={query ? "Nothing matched that filter" : "Nothing starts with Windows"}
            description={
              query ? undefined : "No programs are registered to run when you sign in."
            }
          />
        ) : (
          <Table>
            <thead>
              <tr>
                <Th>Application</Th>
                <Th>Publisher</Th>
                <Th>Location</Th>
                <Th>Impact</Th>
                <Th align="right">Enabled</Th>
                <Th align="right">Actions</Th>
              </tr>
            </thead>
            <tbody>
              {rows.map((item) => (
                <Tr key={item.id}>
                  <Td className="max-w-[220px]">
                    <span className="block truncate font-medium" title={item.name}>
                      {item.name}
                    </span>
                    <span
                      data-selectable
                      className="block truncate text-2xs text-[var(--color-ink-subtle)]"
                      title={item.command}
                    >
                      {shortenPath(item.command, 46)}
                    </span>
                  </Td>
                  <Td className="max-w-[160px] text-[var(--color-ink-muted)]">
                    <span className="block truncate">{item.publisher ?? "Unknown"}</span>
                  </Td>
                  <Td className="text-[var(--color-ink-muted)]">{item.location_label}</Td>
                  <Td>
                    <Tooltip content="Windows does not publish its measured startup impact, so this band is estimated from the size of the program.">
                      <span>
                        <Badge tone={IMPACT_TONE[item.impact]}>{IMPACT_LABEL[item.impact]}</Badge>
                      </span>
                    </Tooltip>
                  </Td>
                  <Td align="right">
                    <div className="flex justify-end">
                      <Tooltip
                        content={
                          item.can_toggle
                            ? ""
                            : "This entry applies to every user. Restart AllInsight as administrator to change it."
                        }
                      >
                        <span className="inline-flex items-center gap-1.5">
                          {!item.can_toggle ? (
                            <Lock className="size-3 text-[var(--color-ink-subtle)]" />
                          ) : null}
                          <Switch
                            checked={item.enabled}
                            disabled={!item.can_toggle || busy === item.id}
                            onCheckedChange={(value) => toggle(item.id, value, item.name)}
                            label={`${item.name} starts with Windows`}
                          />
                        </span>
                      </Tooltip>
                    </div>
                  </Td>
                  <Td align="right">
                    <Button
                      size="sm"
                      variant="ghost"
                      disabled={!item.executable}
                      onClick={() =>
                        item.executable &&
                        api.showInExplorer(item.executable).catch((e) => reportError(e))
                      }
                      icon={<FolderOpen className="size-3.5" />}
                      aria-label="Open file location"
                    />
                  </Td>
                </Tr>
              ))}
            </tbody>
          </Table>
        )}
      </Panel>

      <div className="flex gap-2 rounded-md border border-[var(--color-line)] bg-[var(--color-surface)] p-3">
        <Info className="mt-0.5 size-3.5 shrink-0 text-[var(--color-ink-subtle)]" />
        <Hint>
          Disabling an entry does not uninstall anything and does not delete the registry value or
          the shortcut. Windows records the change in the same place Task Manager uses, so you can
          undo it from either application.
        </Hint>
      </div>
    </div>
  );
}
