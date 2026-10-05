/**
 * Settings backup and transfer.
 *
 * Export writes one JSON file. Import is always two steps: the backend first
 * describes every change the file would make, and only a confirmation of
 * that exact description applies it. Changes that weaken protection are
 * listed first and need a separate, explicit tick; the backend refuses them
 * without it, so the checkbox is a courtesy, not the safeguard.
 */
import * as React from "react";
import { AlertTriangle, Download, RotateCcw, Upload } from "lucide-react";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";

import { api } from "@/lib/api";
import { useAsync, useStore } from "@/app/store";
import { Badge, Button, Panel, PanelHeader } from "@/components/ui/primitives";
import { Checkbox, SettingRow } from "@/components/ui/controls";
import { Dialog } from "@/components/ui/overlay";
import type { BackupEntry, ImportPreview } from "@/lib/types";

/** "2026-10-05" or "2026-10-05-161200" as a readable local date. */
function backupDate(entry: BackupEntry): string {
  const [y, m, d, t] = entry.created.split("-");
  const date = new Date(Number(y), Number(m) - 1, Number(d));
  const day = date.toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });
  return t && t.length === 6 ? `${day}, ${t.slice(0, 2)}:${t.slice(2, 4)}` : day;
}

export function ConfigTransfer() {
  const { toast, reportError, refreshSettings } = useStore();
  const backups = useAsync<BackupEntry[]>(() => api.listConfigBackups(), []);

  const [source, setSource] = React.useState<string | null>(null);
  const [preview, setPreview] = React.useState<ImportPreview | null>(null);
  const [accepted, setAccepted] = React.useState(false);
  const [applying, setApplying] = React.useState(false);

  const exportConfig = async () => {
    try {
      const destination = await saveDialog({
        title: "Export settings",
        defaultPath: "allinsight-settings.json",
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (typeof destination !== "string") return;
      const written = await api.exportConfig(destination);
      toast({ tone: "success", title: "Settings exported", body: `Saved to ${written}.` });
    } catch (e) {
      reportError(e, "The settings file could not be written.");
    }
  };

  const review = async (path: string) => {
    try {
      const result = await api.previewConfigImport(path);
      setSource(path);
      setAccepted(false);
      setPreview(result);
    } catch (e) {
      reportError(e, "That settings file could not be read.");
    }
  };

  const chooseFile = async () => {
    const chosen = await openDialog({
      title: "Import settings",
      multiple: false,
      filters: [{ name: "JSON", extensions: ["json"] }],
    });
    if (typeof chosen === "string") await review(chosen);
  };

  const close = () => {
    setPreview(null);
    setSource(null);
  };

  const apply = async () => {
    if (!preview || !source) return;
    setApplying(true);
    try {
      await api.applyConfigImport(source, preview.token, accepted);
      await refreshSettings();
      backups.reload();
      close();
      toast({
        tone: "success",
        title: "Settings imported",
        body: "Your previous settings were backed up first and can be restored below.",
      });
    } catch (e) {
      reportError(e, "Those settings could not be imported.");
    } finally {
      setApplying(false);
    }
  };

  const nothingToDo =
    preview !== null &&
    preview.changes.length === 0 &&
    preview.protected_added.length === 0 &&
    preview.protected_removed.length === 0;

  return (
    <>
      <Panel>
        <PanelHeader
          title="Backup and transfer"
          description="Your settings, alert levels and protected folders in one file. It holds no file names, measurements or history."
        />
        <div className="divide-y divide-[var(--color-line)]">
          <SettingRow
            title="Export settings"
            description="Save a copy to move to another computer or keep as a backup."
            control={
              <Button size="sm" variant="secondary" icon={<Download className="size-3.5" />} onClick={exportConfig}>
                Export
              </Button>
            }
          />
          <SettingRow
            title="Import settings"
            description="You see every change before anything is applied. Your current settings are backed up first."
            control={
              <Button size="sm" variant="secondary" icon={<Upload className="size-3.5" />} onClick={chooseFile}>
                Import
              </Button>
            }
          />
        </div>
      </Panel>

      <Panel>
        <PanelHeader
          title="Automatic backups"
          description="A copy of your settings is kept each day for a week, and before every import."
        />
        {(backups.data ?? []).length === 0 ? (
          <p className="px-4 py-3 text-xs text-[var(--color-ink-muted)]">
            The first daily backup is written a few seconds after AllInsight starts.
          </p>
        ) : (
          <ul className="divide-y divide-[var(--color-line)]">
            {(backups.data ?? []).map((entry) => (
              <li key={entry.path} className="flex items-center justify-between gap-3 px-4 py-2.5">
                <div className="flex min-w-0 items-center gap-2">
                  <span className="text-xs text-[var(--color-ink)]">{backupDate(entry)}</span>
                  <Badge tone="neutral">{entry.kind === "daily" ? "Daily" : "Before import"}</Badge>
                </div>
                <Button
                  size="sm"
                  variant="ghost"
                  icon={<RotateCcw className="size-3.5" />}
                  onClick={() => review(entry.path)}
                >
                  Restore
                </Button>
              </li>
            ))}
          </ul>
        )}
      </Panel>

      <Dialog
        open={preview !== null}
        onOpenChange={(open) => (open ? null : close())}
        title="Review imported settings"
        description={
          preview
            ? `Exported from AllInsight ${preview.application_version}${
                preview.exported_at ? ` on ${new Date(preview.exported_at).toLocaleString()}` : ""
              }.`
            : undefined
        }
        width="lg"
        footer={
          <>
            <Button variant="ghost" onClick={close}>
              Cancel
            </Button>
            <Button
              variant="primary"
              loading={applying}
              disabled={nothingToDo || (preview?.weakens_protection === true && !accepted)}
              onClick={apply}
            >
              Import settings
            </Button>
          </>
        }
      >
        {preview ? (
          <div className="max-h-[55vh] space-y-4 overflow-y-auto px-5 pb-5 pt-3 text-xs">
            {nothingToDo ? (
              <p className="text-[var(--color-ink-muted)]">These settings match yours. Nothing would change.</p>
            ) : null}

            {preview.protected_removed.length > 0 ? (
              <section className="space-y-1.5">
                <h4 className="flex items-center gap-1.5 font-medium text-[var(--color-warn)]">
                  <AlertTriangle className="size-3.5" />
                  No longer protected
                </h4>
                {preview.protected_removed.map((path) => (
                  <p key={path} data-selectable className="break-all font-mono text-2xs">
                    {path}
                  </p>
                ))}
              </section>
            ) : null}

            {preview.changes.length > 0 ? (
              <section>
                <h4 className="mb-1.5 font-medium text-[var(--color-ink)]">What will change</h4>
                <table className="w-full">
                  <tbody className="divide-y divide-[var(--color-line)]">
                    {preview.changes.map((change) => (
                      <tr key={change.key}>
                        <td className="py-1.5 pr-3 text-[var(--color-ink)]">
                          <span className="inline-flex items-center gap-1.5">
                            {change.weakens_protection ? (
                              <AlertTriangle
                                className="size-3.5 text-[var(--color-warn)]"
                                aria-label="Reduces protection"
                              />
                            ) : null}
                            {change.label}
                          </span>
                        </td>
                        <td className="py-1.5 pr-3 text-[var(--color-ink-muted)]">{change.from}</td>
                        <td className="py-1.5 text-[var(--color-ink)]">→ {change.to}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </section>
            ) : null}

            {preview.protected_added.length > 0 ? (
              <section className="space-y-1.5">
                <h4 className="font-medium text-[var(--color-ink)]">Newly protected</h4>
                {preview.protected_added.map((path) => (
                  <p key={path} data-selectable className="break-all font-mono text-2xs text-[var(--color-ink-muted)]">
                    {path}
                  </p>
                ))}
              </section>
            ) : null}

            <p className="text-[var(--color-ink-subtle)]">
              Not affected: your AI model and engine locations, which stay as they are on this computer.
            </p>

            {preview.weakens_protection ? (
              <label className="flex items-start gap-2 rounded-lg border border-[var(--color-line-strong)] p-3">
                <Checkbox
                  checked={accepted}
                  onCheckedChange={setAccepted}
                  label="I understand these changes reduce protection"
                />
                <span className="text-[var(--color-ink)]">
                  I understand the changes marked above make AllInsight less careful, and I want to
                  apply them.
                </span>
              </label>
            ) : null}
          </div>
        ) : null}
      </Dialog>
    </>
  );
}
