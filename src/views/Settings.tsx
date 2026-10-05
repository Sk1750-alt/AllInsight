/**
 * Settings.
 *
 * The Privacy and Security sections are not marketing: they show the actual
 * protected list the backend enforces and the exact folders each cleanup
 * category is allowed to touch, read back from Rust. The claim and the
 * mechanism are the same thing.
 */
import * as React from "react";
import {
  ArchiveRestore,
  Bug,
  CircleSlash,
  Cpu,
  FolderPlus,
  Gauge,
  Info,
  Lock,
  Palette,
  Settings2,
  Shield,
  ShieldCheck,
  Sparkles,
  Trash2,
  WifiOff,
  X,
} from "lucide-react";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";

import { usePlatformWords } from "@/lib/platform";
import { api } from "@/lib/api";
import { useAsync, useStore } from "@/app/store";
import {
  Badge,
  Button,
  Divider,
  EmptyState,
  Panel,
  PanelHeader,
  PageHeader,
  Skeleton,
} from "@/components/ui/primitives";
import {
  Checkbox,
  NumberField,
  SearchInput,
  Select,
  SettingRow,
  Switch,
} from "@/components/ui/controls";
import { Dialog } from "@/components/ui/overlay";
import { ConfigTransfer } from "@/components/ConfigTransfer";
import { cn } from "@/lib/utils";
import type {
  CategoryDescription,
  ProtectedPathView,
  Settings as SettingsType,
  Theme,
} from "@/lib/types";

type SectionId =
  | "general"
  | "appearance"
  | "storage"
  | "cleanup"
  | "notifications"
  | "ai"
  | "privacy"
  | "performance"
  | "security"
  | "backup"
  | "advanced"
  | "about";

const SECTIONS: { id: SectionId; label: string; icon: React.ComponentType<{ className?: string }> }[] =
  [
    { id: "general", label: "General", icon: Settings2 },
    { id: "appearance", label: "Appearance", icon: Palette },
    { id: "storage", label: "Storage", icon: Gauge },
    { id: "cleanup", label: "Cleanup", icon: Trash2 },
    { id: "notifications", label: "Notifications", icon: Sparkles },
    { id: "ai", label: "AI", icon: Cpu },
    { id: "privacy", label: "Privacy", icon: WifiOff },
    { id: "performance", label: "Performance", icon: Gauge },
    { id: "security", label: "Security", icon: Shield },
    { id: "backup", label: "Backup", icon: ArchiveRestore },
    { id: "advanced", label: "Advanced", icon: Bug },
    { id: "about", label: "About", icon: Info },
  ];

const THEMES: { value: Theme; label: string; hint?: string }[] = [
  { value: "system", label: "System", hint: "follows your OS" },
  { value: "dark", label: "Dark" },
  { value: "light", label: "Light", hint: "default" },
  { value: "midnight", label: "Midnight", hint: "deeper, warmer" },
  { value: "contrast", label: "High contrast", hint: "pure black, OLED" },
  { value: "paper", label: "Paper", hint: "muted light" },
];

const SIZE_OPTIONS = [
  { value: "104857600", label: "100 MB" },
  { value: "524288000", label: "500 MB" },
  { value: "1073741824", label: "1 GB" },
  { value: "5368709120", label: "5 GB" },
];

const DUPLICATE_SIZES = [
  { value: "1048576", label: "1 MB" },
  { value: "10485760", label: "10 MB" },
  { value: "104857600", label: "100 MB" },
];

export function SettingsView() {
  const { settings, saveSettings, environment, toast, reportError } = useStore();
  const w = usePlatformWords();
  const [section, setSection] = React.useState<SectionId>("general");
  const [draft, setDraft] = React.useState<SettingsType | null>(null);

  React.useEffect(() => {
    if (settings) setDraft(settings);
  }, [settings]);

  const categories = useAsync<CategoryDescription[]>(() => api.getCleanupCategories(), []);
  const protectedPaths = useAsync<ProtectedPathView[]>(() => api.getProtectedPaths(), [settings]);
  const exceptions = useAsync<string[]>(() => api.getCleanupExceptions(), []);
  const [protectedFilter, setProtectedFilter] = React.useState("");

  if (!draft) {
    return (
      <div className="view-enter space-y-5">
        <PageHeader title="Settings" />
        <Skeleton className="h-96" />
      </div>
    );
  }

  const update = (patch: Partial<SettingsType>) => {
    const next = { ...draft, ...patch };
    setDraft(next);
    saveSettings(next).catch(() => setDraft(settings));
  };

  const addProtectedFolder = async () => {
    try {
      const chosen = await openDialog({ directory: true, title: "Choose a folder to protect" });
      if (typeof chosen !== "string") return;
      await api.addProtectedPath(chosen);
      toast({ tone: "success", title: "Folder added to the protected list" });
      protectedPaths.reload();
    } catch (e) {
      reportError(e, "That folder could not be protected.");
    }
  };

  const exportDiagnostics = async () => {
    try {
      const destination = await saveDialog({
        title: "Save diagnostics",
        defaultPath: "allinsight-diagnostics.json",
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (typeof destination !== "string") return;
      const written = await api.exportDiagnostics(destination);
      toast({
        tone: "success",
        title: "Diagnostics written",
        body: `Saved to ${written}. Review it before sharing; nothing was sent anywhere.`,
      });
    } catch (e) {
      reportError(e, "The diagnostics file could not be written.");
    }
  };

  const autoCleanEligible = (categories.data ?? []).filter((c) => c.auto_clean_eligible);

  const filteredProtected = (protectedPaths.data ?? []).filter((p) =>
    protectedFilter ? p.path.toLowerCase().includes(protectedFilter.toLowerCase()) : true,
  );

  return (
    <div className="view-enter space-y-5">
      <PageHeader title="Settings" subtitle="Everything stays on this device." />

      <div className="grid gap-5 lg:grid-cols-[180px_1fr]">
        <nav aria-label="Settings sections" className="space-y-0.5">
          {SECTIONS.map(({ id, label, icon: Icon }) => (
            <button
              key={id}
              onClick={() => setSection(id)}
              className={cn(
                "flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-xs transition-quick",
                section === id
                  ? "bg-[var(--color-surface-hover)] font-medium text-[var(--color-ink)]"
                  : "text-[var(--color-ink-muted)] hover:bg-[var(--color-surface-hover)]",
              )}
            >
              <Icon className="size-3.5 shrink-0 text-[var(--color-ink-subtle)]" />
              {label}
            </button>
          ))}
        </nav>

        <div className="space-y-5">
          {section === "general" ? (
            <Panel>
              <PanelHeader title="General" />
              <div className="divide-y divide-[var(--color-line)]">
                <SettingRow
                  title="Start AllInsight when I sign in"
                  description="Adds an entry for your user account only. It never affects other users on this PC."
                  control={
                    <Switch
                      checked={draft.launch_at_startup}
                      onCheckedChange={(v) => update({ launch_at_startup: v })}
                      label="Start when I sign in"
                    />
                  }
                />
                <SettingRow
                  title="Keep running in the notification area when closed"
                  description="Closing the window hides it to the notification area instead of quitting, so storage and drive alerts keep working. Right-click the AllInsight icon there to reopen or quit."
                  control={
                    <Switch
                      checked={draft.minimise_to_tray}
                      onCheckedChange={(v) => update({ minimise_to_tray: v })}
                      label="Keep running when closed"
                    />
                  }
                />
                <SettingRow
                  title="Scan storage when AllInsight opens"
                  description="Off by default. A scan reads a lot of the disk, and doing it uninvited on every launch is rude."
                  control={
                    <Switch
                      checked={draft.scan_on_launch}
                      onCheckedChange={(v) => update({ scan_on_launch: v })}
                      label="Scan on launch"
                    />
                  }
                />
              </div>
            </Panel>
          ) : null}

          {section === "appearance" ? (
            <Panel>
              <PanelHeader title="Appearance" />
              <div className="divide-y divide-[var(--color-line)]">
                <SettingRow
                  title="Theme"
                  description={`System follows the ${w.os} light and dark setting. The others are fixed palettes.`}
                  control={
                    <Select
                      label="Theme"
                      value={draft.theme}
                      onValueChange={(v) => update({ theme: v as Theme })}
                      options={THEMES}
                      className="min-w-44"
                    />
                  }
                />
                <SettingRow
                  title="Interface scale"
                  description="Between 80 and 150 percent."
                  control={
                    <NumberField
                      label="Interface scale"
                      value={draft.ui_scale}
                      min={80}
                      max={150}
                      step={5}
                      suffix="%"
                      onChange={(v) => update({ ui_scale: v })}
                    />
                  }
                />
                <SettingRow
                  title="Reduce motion"
                  description="Removes the small transitions between screens and panels."
                  control={
                    <Switch
                      checked={draft.reduce_motion}
                      onCheckedChange={(v) => update({ reduce_motion: v })}
                      label="Reduce motion"
                    />
                  }
                />
              </div>
            </Panel>
          ) : null}

          {section === "storage" ? (
            <Panel>
              <PanelHeader title="Storage" />
              <div className="divide-y divide-[var(--color-line)]">
                <SettingRow
                  title="Large file threshold"
                  description="The default minimum size on the Large Files screen."
                  control={
                    <Select
                      label="Large file threshold"
                      value={String(draft.large_file_threshold_bytes)}
                      onValueChange={(v) => update({ large_file_threshold_bytes: Number(v) })}
                      options={SIZE_OPTIONS}
                    />
                  }
                />
                <SettingRow
                  title="Duplicate minimum size"
                  description="Smaller files are ignored. There are usually thousands of them and they reclaim almost nothing."
                  control={
                    <Select
                      label="Duplicate minimum size"
                      value={String(draft.duplicate_min_bytes)}
                      onValueChange={(v) => update({ duplicate_min_bytes: Number(v) })}
                      options={DUPLICATE_SIZES}
                    />
                  }
                />
              </div>
            </Panel>
          ) : null}

          {section === "cleanup" ? (
            <>
              <Panel>
                <PanelHeader
                  title="Automatic cleanup"
                  description={`Only categories whose data ${w.os} rebuilds by itself can ever run unattended.`}
                />
                <div className="divide-y divide-[var(--color-line)]">
                  <SettingRow
                    title="Clean safe categories automatically"
                    description="Runs in the background when free space falls below the threshold below."
                    control={
                      <Switch
                        checked={draft.auto_clean_enabled}
                        onCheckedChange={(v) => update({ auto_clean_enabled: v })}
                        label="Automatic cleanup"
                      />
                    }
                  />
                  <SettingRow
                    title="Run when free space falls below"
                    disabled={!draft.auto_clean_enabled}
                    description={`Measured on the drive that holds ${w.os}.`}
                    control={
                      <NumberField
                        label="Free space threshold"
                        value={draft.auto_clean_free_space_percent}
                        min={5}
                        max={50}
                        suffix="%"
                        onChange={(v) => update({ auto_clean_free_space_percent: v })}
                      />
                    }
                  />
                </div>

                <div className="border-t border-[var(--color-line)] p-4">
                  <p className="text-xs font-medium text-[var(--color-ink)]">
                    Categories automatic cleanup may use
                  </p>
                  <p className="mt-0.5 text-2xs text-[var(--color-ink-muted)]">
                    This list is fixed in AllInsight itself. Downloads, documents, media, duplicates
                    and the {w.trash} can never appear here, whatever is selected.
                  </p>
                  <div className="mt-3 space-y-2">
                    {autoCleanEligible.map((category) => (
                      <label
                        key={category.id}
                        className="flex cursor-pointer items-start gap-2.5"
                      >
                        <Checkbox
                          checked={draft.auto_clean_categories.includes(category.id)}
                          disabled={!draft.auto_clean_enabled}
                          onCheckedChange={(value) =>
                            update({
                              auto_clean_categories: value
                                ? [...draft.auto_clean_categories, category.id]
                                : draft.auto_clean_categories.filter((c) => c !== category.id),
                            })
                          }
                          label={category.name}
                        />
                        <span className="min-w-0">
                          <span className="block text-xs text-[var(--color-ink)]">
                            {category.name}
                          </span>
                          <span className="block text-2xs text-[var(--color-ink-muted)]">
                            {category.description}
                          </span>
                        </span>
                      </label>
                    ))}
                  </div>
                </div>
              </Panel>

              <Panel>
                <PanelHeader
                  title="Where each category is allowed to look"
                  description="Read back from the backend. These are the only folders a cleanup can touch."
                />
                <div className="divide-y divide-[var(--color-line)]">
                  {(categories.data ?? []).map((category) => (
                    <div key={category.id} className="px-4 py-3">
                      <div className="flex items-center gap-2">
                        <span className="text-xs font-medium text-[var(--color-ink)]">
                          {category.name}
                        </span>
                        {category.requires_elevation ? <Badge tone="neutral">Admin</Badge> : null}
                        {!category.present ? <Badge tone="unknown">Not on this PC</Badge> : null}
                      </div>
                      {category.roots.length > 0 ? (
                        <ul className="mt-1.5 space-y-0.5">
                          {category.roots.map((root) => (
                            <li
                              key={root}
                              data-selectable
                              className="break-all font-mono text-2xs text-[var(--color-ink-muted)]"
                            >
                              {root}
                            </li>
                          ))}
                        </ul>
                      ) : (
                        <p className="mt-1 text-2xs text-[var(--color-ink-subtle)]">
                          Handled by {w.os} rather than by folder.
                        </p>
                      )}
                    </div>
                  ))}
                </div>
              </Panel>
            </>
          ) : null}

          {section === "notifications" ? (
            <Panel>
              <PanelHeader title="Notifications" />
              <div className="divide-y divide-[var(--color-line)]">
                <SettingRow
                  title="Show notifications"
                  description={w.isWindows ? "Uses the Windows notification centre." : "Uses your desktop's notifications."}
                  control={
                    <Switch
                      checked={draft.notifications_enabled}
                      onCheckedChange={(v) => update({ notifications_enabled: v })}
                      label="Show notifications"
                    />
                  }
                />
                <SettingRow
                  title="Storage alert thresholds"
                  disabled={!draft.notifications_enabled}
                  description="AllInsight tells you once when usage crosses each of these, not repeatedly."
                  control={
                    <div className="flex gap-1.5">
                      {[70, 80, 90, 95].map((percent) => {
                        const active = draft.alert_at_percent.includes(percent);
                        return (
                          <button
                            key={percent}
                            disabled={!draft.notifications_enabled}
                            onClick={() =>
                              update({
                                alert_at_percent: active
                                  ? draft.alert_at_percent.filter((p) => p !== percent)
                                  : [...draft.alert_at_percent, percent],
                              })
                            }
                            className={cn(
                              "rounded-md border px-2 py-1 text-2xs transition-quick",
                              active
                                ? "border-[var(--color-accent)] bg-[var(--color-accent-soft)] text-[var(--color-accent)]"
                                : "border-[var(--color-line-strong)] text-[var(--color-ink-muted)]",
                            )}
                          >
                            {percent}%
                          </button>
                        );
                      })}
                    </div>
                  }
                />
                <SettingRow
                  title="Drive health warnings"
                  disabled={!draft.notifications_enabled}
                  description="Only when a drive actually reports a problem."
                  control={
                    <Switch
                      checked={draft.notify_drive_health}
                      onCheckedChange={(v) => update({ notify_drive_health: v })}
                      label="Drive health warnings"
                    />
                  }
                />
                <SettingRow
                  title="Minimum gap between notifications"
                  disabled={!draft.notifications_enabled}
                  description="Keeps AllInsight from repeating itself."
                  control={
                    <NumberField
                      label="Quiet period"
                      value={draft.notification_quiet_minutes}
                      min={0}
                      max={1440}
                      step={30}
                      suffix="min"
                      onChange={(v) => update({ notification_quiet_minutes: v })}
                    />
                  }
                />
              </div>
            </Panel>
          ) : null}

          {section === "ai" ? (
            <Panel>
              <PanelHeader
                title="Local AI"
                description="Optional. AllInsight works fully without a model."
              />
              <div className="divide-y divide-[var(--color-line)]">
                <SettingRow
                  title="Enable the local assistant"
                  description="When off, the assistant screen still answers from measurements."
                  control={
                    <Switch
                      checked={draft.ai_enabled}
                      onCheckedChange={(v) => update({ ai_enabled: v })}
                      label="Enable the local assistant"
                    />
                  }
                />
                <SettingRow
                  title="Context size"
                  disabled={!draft.ai_enabled}
                  description="Tokens the model can consider at once. Larger uses more memory."
                  control={
                    <NumberField
                      label="Context size"
                      value={draft.ai_context_size}
                      min={512}
                      max={32768}
                      step={512}
                      onChange={(v) => update({ ai_context_size: v })}
                    />
                  }
                />
                <SettingRow
                  title="CPU threads"
                  disabled={!draft.ai_enabled}
                  description="Zero lets AllInsight choose, leaving one core for the interface."
                  control={
                    <NumberField
                      label="CPU threads"
                      value={draft.ai_threads}
                      min={0}
                      max={64}
                      onChange={(v) => update({ ai_threads: v })}
                    />
                  }
                />
                <SettingRow
                  title="GPU layers"
                  disabled={!draft.ai_enabled}
                  description="Zero keeps inference entirely on the processor. Raise it only if your graphics card has spare memory."
                  control={
                    <NumberField
                      label="GPU layers"
                      value={draft.ai_gpu_layers}
                      min={0}
                      max={200}
                      onChange={(v) => update({ ai_gpu_layers: v })}
                    />
                  }
                />
                <SettingRow
                  title="Load the model when AllInsight starts"
                  disabled={!draft.ai_enabled}
                  description="Off by default, so a large model never delays the first screen."
                  control={
                    <Switch
                      checked={draft.ai_load_automatically}
                      onCheckedChange={(v) => update({ ai_load_automatically: v })}
                      label="Load the model at startup"
                    />
                  }
                />
                <SettingRow
                  title="Keep the model in memory"
                  disabled={!draft.ai_enabled}
                  description="Answers arrive faster, at the cost of several gigabytes held permanently."
                  control={
                    <Switch
                      checked={draft.ai_keep_loaded}
                      onCheckedChange={(v) => update({ ai_keep_loaded: v })}
                      label="Keep the model loaded"
                    />
                  }
                />
              </div>
            </Panel>
          ) : null}

          {section === "privacy" ? (
            <>
              <Panel className="border-[color-mix(in_srgb,var(--color-accent)_35%,transparent)]">
                <div className="flex items-start gap-3 p-4">
                  <ShieldCheck className="mt-0.5 size-5 shrink-0 text-[var(--color-accent)]" />
                  <div>
                    <p className="text-sm font-semibold text-[var(--color-ink)]">
                      Everything stays on this device.
                    </p>
                    <p className="mt-1 text-xs leading-relaxed text-[var(--color-ink-muted)]">
                      AllInsight contains no code that sends data anywhere. There is no account, no
                      server, and no analytics. The switches below are shown so you can verify
                      that, not so you can turn them on.
                    </p>
                  </div>
                </div>
              </Panel>

              <Panel>
                <PanelHeader title="Data collection" />
                <div className="divide-y divide-[var(--color-line)]">
                  {[
                    {
                      title: "Cloud services",
                      description: "No part of AllInsight contacts a remote service.",
                    },
                    {
                      title: "Telemetry",
                      description: "No usage data is collected, aggregated or transmitted.",
                    },
                    {
                      title: "Crash reporting",
                      description:
                        "Crashes are written to the local log only, and never uploaded.",
                    },
                  ].map((item) => (
                    <div key={item.title} className="flex items-start justify-between gap-6 px-4 py-3">
                      <div>
                        <p className="text-xs font-medium text-[var(--color-ink)]">{item.title}</p>
                        <p className="mt-0.5 text-2xs text-[var(--color-ink-muted)]">
                          {item.description}
                        </p>
                      </div>
                      <Badge tone="neutral">
                        <CircleSlash className="size-2.5" />
                        Off
                      </Badge>
                    </div>
                  ))}
                </div>
              </Panel>

              <Panel>
                <PanelHeader title="What AllInsight stores locally" />
                <div className="space-y-2 p-4 text-xs leading-relaxed text-[var(--color-ink-muted)]">
                  <p>
                    <span className="text-[var(--color-ink)]">Settings and history.</span> Your
                    preferences, cleanup totals, and volume capacity over time. Cleanup history
                    records sizes and category names, never file names.
                  </p>
                  <p>
                    <span className="text-[var(--color-ink)]">Scan results.</span> Folder sizes,
                    held in memory for the session. They are not written to disk.
                  </p>
                  <p>
                    <span className="text-[var(--color-ink)]">Never stored.</span> File contents,
                    file hashes, your questions to the assistant, or its answers.
                  </p>
                  <p className="pt-1 font-mono text-2xs" data-selectable>
                    {environment?.data_directory}
                  </p>
                </div>
              </Panel>
            </>
          ) : null}

          {section === "performance" ? (
            <Panel>
              <PanelHeader title="Performance" />
              <div className="divide-y divide-[var(--color-line)]">
                <SettingRow
                  title="Background monitoring"
                  description="Checks capacity and drive health on an interval. It reads volume totals only; it never walks the filesystem."
                  control={
                    <Switch
                      checked={draft.background_monitoring}
                      onCheckedChange={(v) => update({ background_monitoring: v })}
                      label="Background monitoring"
                    />
                  }
                />
                <SettingRow
                  title="Check every"
                  disabled={!draft.background_monitoring}
                  control={
                    <NumberField
                      label="Monitor interval"
                      value={Math.round(draft.monitor_interval_seconds / 60)}
                      min={1}
                      max={60}
                      suffix="min"
                      onChange={(v) => update({ monitor_interval_seconds: v * 60 })}
                    />
                  }
                />
                <SettingRow
                  title="Scan worker threads"
                  description="Zero matches the number of cores on this machine."
                  control={
                    <NumberField
                      label="Scan threads"
                      value={draft.scan_threads}
                      min={0}
                      max={64}
                      onChange={(v) => update({ scan_threads: v })}
                    />
                  }
                />
              </div>
            </Panel>
          ) : null}

          {section === "security" ? (
            <>
              <Panel>
                <PanelHeader
                  title="Protected folders"
                  description="AllInsight refuses to remove anything inside these, whatever else is true."
                  actions={
                    <Button
                      size="sm"
                      variant="secondary"
                      icon={<FolderPlus className="size-3.5" />}
                      onClick={addProtectedFolder}
                    >
                      Add a folder
                    </Button>
                  }
                />
                <div className="p-3">
                  <SearchInput
                    value={protectedFilter}
                    onChange={setProtectedFilter}
                    placeholder="Filter the protected list"
                  />
                </div>
                <div className="max-h-96 divide-y divide-[var(--color-line)] overflow-y-auto">
                  {protectedPaths.loading ? (
                    <div className="space-y-2 p-4">
                      {Array.from({ length: 6 }).map((_, i) => (
                        <Skeleton key={i} className="h-8" />
                      ))}
                    </div>
                  ) : filteredProtected.length === 0 ? (
                    <EmptyState title="Nothing matched that filter" />
                  ) : (
                    filteredProtected.map((entry) => (
                      <div
                        key={entry.path}
                        className="flex items-center gap-3 px-4 py-2"
                      >
                        <Lock className="size-3 shrink-0 text-[var(--color-ink-subtle)]" />
                        <div className="min-w-0 flex-1">
                          <p
                            data-selectable
                            className="truncate font-mono text-2xs text-[var(--color-ink)]"
                            title={entry.path}
                          >
                            {entry.path}
                          </p>
                          <p className="text-2xs text-[var(--color-ink-subtle)]">
                            {entry.explanation}
                          </p>
                        </div>
                        {entry.user_added ? (
                          <Button
                            size="sm"
                            variant="ghost"
                            aria-label="Remove from the protected list"
                            icon={<X className="size-3.5" />}
                            onClick={async () => {
                              try {
                                await api.removeProtectedPath(entry.path);
                                protectedPaths.reload();
                              } catch (e) {
                                reportError(e);
                              }
                            }}
                          />
                        ) : (
                          <Badge tone="neutral">Built in</Badge>
                        )}
                      </div>
                    ))
                  )}
                </div>
              </Panel>

              <Panel>
                <PanelHeader
                  title="Exceptions inside protected folders"
                  description="The only places inside a protected root that a cleanup category may be pointed at. This list is compiled into AllInsight and cannot be changed from the interface."
                />
                <div className="space-y-1 p-4">
                  {(exceptions.data ?? []).map((path) => (
                    <p
                      key={path}
                      data-selectable
                      className="break-all font-mono text-2xs text-[var(--color-ink-muted)]"
                    >
                      {path}
                    </p>
                  ))}
                </div>
              </Panel>

              <Panel>
                <PanelHeader title="Process safety" />
                <SettingRow
                  title="Confirm before ending a process"
                  description={`Processes ${w.os} needs are refused outright, whatever this is set to.`}
                  control={
                    <Switch
                      checked={draft.require_confirmation_for_processes}
                      onCheckedChange={(v) => update({ require_confirmation_for_processes: v })}
                      label="Confirm before ending a process"
                    />
                  }
                />
              </Panel>
            </>
          ) : null}

          {section === "backup" ? <ConfigTransfer /> : null}

          {section === "advanced" ? (
            <Panel>
              <PanelHeader title="Advanced" />
              <div className="divide-y divide-[var(--color-line)]">
                <SettingRow
                  title="Export diagnostics"
                  description="Writes a JSON file with settings, storage totals and drive information. It contains no file names, and nothing is transmitted."
                  control={
                    <Button size="sm" variant="secondary" onClick={exportDiagnostics}>
                      Export
                    </Button>
                  }
                />
                <SettingRow
                  title="Data folder"
                  description={environment?.data_directory}
                  control={
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={() =>
                        environment &&
                        api.showInExplorer(environment.data_directory).catch((e) => reportError(e))
                      }
                    >
                      Open
                    </Button>
                  }
                />
                <SettingRow
                  title="Logs"
                  description={environment?.log_directory}
                  control={
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={() =>
                        environment &&
                        api.showInExplorer(environment.log_directory).catch((e) => reportError(e))
                      }
                    >
                      Open
                    </Button>
                  }
                />
                {w.canElevate ? (
                <SettingRow
                  title="Administrator permission"
                  description={
                    environment?.elevated
                      ? "AllInsight is running with administrator permission."
                      : "AllInsight runs without elevation by default. Restart elevated to read full drive health and machine-wide startup entries."
                  }
                  control={
                    <Button
                      size="sm"
                      variant="secondary"
                      disabled={environment?.elevated}
                      onClick={() => api.restartElevated().catch((e) => reportError(e))}
                    >
                      Restart as administrator
                    </Button>
                  }
                />
                ) : null}
              </div>
            </Panel>
          ) : null}

          {section === "about" ? (
            <Panel>
              <div className="space-y-4 p-6">
                <div>
                  <h2 className="font-display text-xl font-semibold tracking-[0.01em] text-[var(--color-ink)]">
                    AllInsight
                  </h2>
                  <p className="mt-1 text-xs text-[var(--color-ink-muted)]">
                    Your device, understood.
                  </p>
                  <p className="mt-0.5 text-2xs text-[var(--color-ink-subtle)]">
                    Private by design. Intelligent by default.
                  </p>
                </div>

                <Divider />

                <dl className="space-y-1.5 text-xs">
                  <div className="flex justify-between">
                    <dt className="text-[var(--color-ink-muted)]">Version</dt>
                    <dd className="numeric text-[var(--color-ink)]">
                      {environment?.app_version ?? "1.0.0"}
                    </dd>
                  </div>
                  <div className="flex justify-between">
                    <dt className="text-[var(--color-ink-muted)]">Operating system</dt>
                    <dd className="text-[var(--color-ink)]">{environment?.os_name ?? w.os}</dd>
                  </div>
                  <div className="flex justify-between">
                    <dt className="text-[var(--color-ink-muted)]">Network access</dt>
                    <dd className="text-[var(--color-ink)]">None</dd>
                  </div>
                </dl>

                <Divider />

                <p className="text-xs leading-relaxed text-[var(--color-ink-muted)]">
                  AllInsight analyses storage, monitors device health and explains what it finds,
                  entirely on this computer. It has no account, no server and no telemetry, and it
                  works exactly the same with the network disconnected.
                </p>

                <ThirdPartyLicences />
              </div>
            </Panel>
          ) : null}
        </div>
      </div>
    </div>
  );
}

/**
 * The licence texts of every third-party package AllInsight ships, which their
 * licences require to travel with the binary.
 *
 * The file is generated at build time by scripts/generate-licenses.mjs and
 * imported as a separate chunk, so the 1.5 MB of text is only loaded when this
 * dialog opens. It is imported rather than fetched: the content security policy
 * allows no fetch of the application's own assets, and that stays true.
 */
function ThirdPartyLicences() {
  const [open, setOpen] = React.useState(false);
  const [text, setText] = React.useState<string | null>(null);
  const [failed, setFailed] = React.useState(false);

  React.useEffect(() => {
    if (!open || text !== null) return;
    let cancelled = false;
    import("@/generated/third-party-licenses.txt?raw")
      .then((module) => {
        if (!cancelled) setText(module.default);
      })
      .catch(() => {
        if (!cancelled) setFailed(true);
      });
    return () => {
      cancelled = true;
    };
  }, [open, text]);

  return (
    <>
      <Button variant="secondary" size="sm" onClick={() => setOpen(true)}>
        Third-party licences
      </Button>
      <Dialog
        open={open}
        onOpenChange={setOpen}
        title="Third-party licences"
        description="AllInsight is built on open-source packages. Their licences are reproduced here, as they require."
        width="lg"
      >
        <div className="px-5 pb-5 pt-3">
          {failed ? (
            <p className="text-xs text-[var(--color-ink-muted)]">
              The licence file could not be read. It is also published at allinsight.biz.
            </p>
          ) : text === null ? (
            <Skeleton className="h-64 w-full" />
          ) : (
            <pre
              data-selectable
              className="max-h-[60vh] overflow-auto whitespace-pre-wrap rounded-md border border-[var(--color-line)] bg-[var(--color-canvas)] p-3 font-mono text-2xs leading-relaxed text-[var(--color-ink-muted)]"
            >
              {text}
            </pre>
          )}
        </div>
      </Dialog>
    </>
  );
}
