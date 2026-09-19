/**
 * Installed applications.
 *
 * Uninstalling always hands over to the vendor's own uninstaller. AllInsight never
 * deletes an application's folder as a shortcut: that leaves the registry,
 * services and scheduled tasks behind and is how "cleaner" utilities break
 * machines.
 */
import * as React from "react";
import { Blocks, FolderOpen, Info, RefreshCw, Ruler, Trash2 } from "lucide-react";

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
import { SearchInput, SegmentedControl } from "@/components/ui/controls";
import { ConfirmDialog, Tooltip } from "@/components/ui/overlay";
import { Table, Td, Th, Tr } from "@/components/ui/data";
import { formatBytes, formatCount, shortenPath } from "@/lib/format";
import type { AppList, InstalledApp } from "@/lib/types";

type SortKey = "size" | "name" | "date";

export function ApplicationsView() {
  const { toast, reportError } = useStore();
  const [measure, setMeasure] = React.useState(false);
  const { data, loading, error, reload } = useAsync<AppList>(
    () => api.getInstalledApplications(measure),
    [measure],
  );

  const [query, setQuery] = React.useState("");
  const [sort, setSort] = React.useState<SortKey>("size");
  const [pending, setPending] = React.useState<InstalledApp | null>(null);
  const [starting, setStarting] = React.useState(false);

  const rows = React.useMemo(() => {
    const apps = data?.apps ?? [];
    const q = query.trim().toLowerCase();
    const filtered = q
      ? apps.filter(
          (a) =>
            a.name.toLowerCase().includes(q) || (a.publisher ?? "").toLowerCase().includes(q),
        )
      : apps;
    const sorted = [...filtered];
    const size = (a: InstalledApp) => a.measured_size_bytes ?? a.estimated_size_bytes ?? 0;
    if (sort === "size") sorted.sort((a, b) => size(b) - size(a));
    if (sort === "name") sorted.sort((a, b) => a.name.localeCompare(b.name));
    if (sort === "date")
      sorted.sort((a, b) => (b.install_date ?? "").localeCompare(a.install_date ?? ""));
    return sorted;
  }, [data, query, sort]);

  const totalSize = rows.reduce(
    (sum, a) => sum + (a.measured_size_bytes ?? a.estimated_size_bytes ?? 0),
    0,
  );

  const uninstall = async () => {
    if (!pending) return;
    setStarting(true);
    try {
      await api.uninstallApplication(pending.id);
      toast({
        tone: "info",
        title: `The uninstaller for ${pending.name} has started`,
        body: "Follow its own prompts to finish. AllInsight takes no further part.",
      });
      setPending(null);
    } catch (e) {
      reportError(e, "The uninstaller could not be started.");
    } finally {
      setStarting(false);
    }
  };

  return (
    <div className="view-enter space-y-5">
      <PageHeader
        title="Applications"
        subtitle={
          data
            ? `${formatCount(data.total)} installed, about ${formatBytes(totalSize)} in total`
            : "Reading the installed application list"
        }
        actions={
          <>
            <Tooltip content="Measures each install folder on disk. Slower, but far more accurate than the size vendors report.">
              <span>
                <Button
                  variant={measure ? "primary" : "secondary"}
                  icon={<Ruler className="size-3.5" />}
                  onClick={() => setMeasure((m) => !m)}
                >
                  {measure ? "Measured sizes" : "Measure real sizes"}
                </Button>
              </span>
            </Tooltip>
            <Button variant="ghost" icon={<RefreshCw className="size-3.5" />} onClick={reload}>
              Refresh
            </Button>
          </>
        }
      />

      <Panel>
        <div className="flex flex-wrap items-center gap-3 p-3">
          <SearchInput
            value={query}
            onChange={setQuery}
            placeholder="Filter by name or publisher"
            className="min-w-64 flex-1"
          />
          <SegmentedControl
            label="Sort by"
            value={sort}
            onChange={setSort}
            options={[
              { value: "size", label: "Size" },
              { value: "name", label: "Name" },
              { value: "date", label: "Installed" },
            ]}
          />
        </div>
      </Panel>

      <Panel>
        <PanelHeader
          title="Installed applications"
          description="Read from the same registry entries Windows Settings uses."
        />

        {loading ? (
          <div className="space-y-2 p-4">
            {Array.from({ length: 8 }).map((_, i) => (
              <Skeleton key={i} className="h-10" />
            ))}
          </div>
        ) : error ? (
          <EmptyState title="The application list could not be read" description={error} />
        ) : rows.length === 0 ? (
          <EmptyState
            icon={<Blocks className="size-5" />}
            title={query ? "Nothing matched that filter" : "No applications found"}
          />
        ) : (
          <div className="max-h-[calc(100vh-330px)] overflow-y-auto">
            <Table>
              <thead>
                <tr>
                  <Th>Application</Th>
                  <Th>Publisher</Th>
                  <Th>Version</Th>
                  <Th align="right">Size</Th>
                  <Th align="right">Installed</Th>
                  <Th align="right">Actions</Th>
                </tr>
              </thead>
              <tbody>
                {rows.map((app) => {
                  const size = app.measured_size_bytes ?? app.estimated_size_bytes;
                  return (
                    <Tr key={app.id}>
                      <Td className="max-w-[260px]">
                        <div className="flex items-center gap-2">
                          <span className="truncate font-medium" title={app.name}>
                            {app.name}
                          </span>
                          {app.scope === "current_user" ? (
                            <Badge tone="neutral">This user</Badge>
                          ) : null}
                        </div>
                        {app.install_location ? (
                          <span
                            data-selectable
                            className="block truncate text-2xs text-[var(--color-ink-subtle)]"
                            title={app.install_location}
                          >
                            {shortenPath(app.install_location, 48)}
                          </span>
                        ) : null}
                      </Td>
                      <Td className="max-w-[180px] text-[var(--color-ink-muted)]">
                        <span className="block truncate">{app.publisher ?? "Unknown"}</span>
                      </Td>
                      <Td className="text-[var(--color-ink-muted)]">{app.version ?? "-"}</Td>
                      <Td align="right">
                        {size ? (
                          <Tooltip
                            content={
                              app.measured_size_bytes
                                ? "Measured on disk by AllInsight."
                                : "Reported by the application when it was installed. Often approximate."
                            }
                          >
                            <span
                              className={
                                app.measured_size_bytes
                                  ? "text-[var(--color-ink)]"
                                  : "text-[var(--color-ink-muted)]"
                              }
                            >
                              {formatBytes(size)}
                            </span>
                          </Tooltip>
                        ) : (
                          <span className="text-[var(--color-ink-subtle)]">Not reported</span>
                        )}
                      </Td>
                      <Td align="right" className="text-[var(--color-ink-muted)]">
                        {app.install_date ?? "-"}
                      </Td>
                      <Td align="right">
                        <div className="flex justify-end gap-1">
                          <Tooltip content="Open the install folder">
                            <span>
                              <Button
                                size="sm"
                                variant="ghost"
                                disabled={!app.install_location}
                                onClick={() =>
                                  app.install_location &&
                                  api
                                    .showInExplorer(app.install_location)
                                    .catch((e) => reportError(e))
                                }
                                icon={<FolderOpen className="size-3.5" />}
                                aria-label="Open install folder"
                              />
                            </span>
                          </Tooltip>
                          <Tooltip
                            content={
                              app.has_uninstaller
                                ? "Start this application's own uninstaller"
                                : "This application did not register an uninstaller with Windows."
                            }
                          >
                            <span>
                              <Button
                                size="sm"
                                variant="ghost"
                                disabled={!app.has_uninstaller}
                                onClick={() => setPending(app)}
                                icon={<Trash2 className="size-3.5" />}
                                aria-label="Uninstall"
                              />
                            </span>
                          </Tooltip>
                        </div>
                      </Td>
                    </Tr>
                  );
                })}
              </tbody>
            </Table>
          </div>
        )}
      </Panel>

      <div className="flex gap-2 rounded-md border border-[var(--color-line)] bg-[var(--color-surface)] p-3">
        <Info className="mt-0.5 size-3.5 shrink-0 text-[var(--color-ink-subtle)]" />
        <Hint>
          Sizes without the measured label come from the value the installer wrote into the
          registry, which vendors frequently leave stale or omit. Measuring reads the install
          folder directly.
        </Hint>
      </div>

      <ConfirmDialog
        open={!!pending}
        onOpenChange={(open) => !open && setPending(null)}
        title={`Uninstall ${pending?.name ?? ""}?`}
        loading={starting}
        destructive={false}
        confirmLabel="Start uninstaller"
        onConfirm={uninstall}
        whatHappens="AllInsight starts the uninstaller that this application registered with Windows, and steps back. The uninstaller takes over from there, including any administrator prompt Windows decides to show."
        whatIsUntouched="AllInsight does not delete any folder or registry entry itself. If the uninstaller leaves files behind, they stay until you remove them deliberately."
      />
    </div>
  );
}
