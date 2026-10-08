/**
 * First run.
 *
 * Four short steps, no account, and nothing enabled without being asked for.
 * The privacy step is second rather than buried, because it is the reason
 * someone would choose this over the alternatives.
 */
import * as React from "react";
import { ArrowRight, Check, CircleSlash, Cpu, HardDrive, ShieldCheck, WifiOff } from "lucide-react";

import { usePlatformWords } from "@/lib/platform";
import { api } from "@/lib/api";
import { useStore } from "@/app/store";
import { Logo } from "@/components/Logo";
import { Badge, Button, Hint, Panel } from "@/components/ui/primitives";
import { SettingRow, Switch } from "@/components/ui/controls";
import { cn } from "@/lib/utils";
import type { Settings } from "@/lib/types";

const STEPS = ["Welcome", "Privacy", "Monitoring", "Ready"] as const;

export function FirstRunView({ onDone }: { onDone: () => void }) {
  const { settings, saveSettings, reportError } = useStore();
  const w = usePlatformWords();
  const [step, setStep] = React.useState(0);
  const [draft, setDraft] = React.useState<Settings | null>(settings);
  const [finishing, setFinishing] = React.useState(false);
  const [scanNow, setScanNow] = React.useState(false);

  React.useEffect(() => {
    if (settings && !draft) setDraft(settings);
  }, [settings, draft]);

  if (!draft) return null;

  const update = (patch: Partial<Settings>) => setDraft({ ...draft, ...patch });

  const finish = async () => {
    setFinishing(true);
    try {
      await saveSettings(draft);
      await api.completeFirstRun();
      if (scanNow) {
        const overview = await api.getStorageOverview();
        const root = overview.system_volume ?? overview.volumes[0]?.mount_point;
        if (root) await api.scanDirectory(root);
      }
      onDone();
    } catch (e) {
      reportError(e, "Setup could not be completed.");
    } finally {
      setFinishing(false);
    }
  };

  return (
    <div className="flex h-full items-center justify-center bg-[var(--color-canvas)] p-6">
      <div className="w-full max-w-xl">
        <div className="mb-6 flex items-center gap-3">
          <Logo size={34} />
          <div>
            {/* Tight tracking: the wide setting belonged to a six-capital
                name and pulls this one apart into separate letters. */}
            <h1 className="font-display text-xl font-semibold tracking-[0.01em] text-[var(--color-ink)]">
              AllInsight
            </h1>
            <p className="text-xs text-[var(--color-ink-muted)]">Your device, understood.</p>
          </div>
        </div>

        <div className="mb-4 flex items-center gap-1.5">
          {STEPS.map((label, index) => (
            <div key={label} className="flex flex-1 items-center gap-1.5">
              <div
                className={cn(
                  "h-1 flex-1 rounded-full transition-quick",
                  index <= step ? "bg-[var(--color-accent)]" : "bg-[var(--color-surface-hover)]",
                )}
              />
            </div>
          ))}
        </div>

        <Panel className="overflow-hidden">
          {step === 0 ? (
            <div className="space-y-4 p-6">
              <h2 className="text-base font-semibold text-[var(--color-ink)]">
                Welcome to AllInsight
              </h2>
              <p className="text-sm leading-relaxed text-[var(--color-ink-muted)]">
                AllInsight analyses your storage, watches your drives, and explains what it finds. It
                does all of that on this computer.
              </p>
              <div className="space-y-2.5 rounded-md border border-[var(--color-line)] p-4">
                {[
                  { icon: HardDrive, text: "See exactly where your storage has gone." },
                  { icon: ShieldCheck, text: "Clean only what is genuinely safe to remove." },
                  { icon: Cpu, text: "Ask questions about this device, answered locally." },
                ].map(({ icon: Icon, text }) => (
                  <div key={text} className="flex items-start gap-2.5">
                    <Icon className="mt-0.5 size-4 shrink-0 text-[var(--color-accent)]" />
                    <p className="text-xs text-[var(--color-ink)]">{text}</p>
                  </div>
                ))}
              </div>
              <Hint>There is no account to create, and nothing to sign in to.</Hint>
            </div>
          ) : null}

          {step === 1 ? (
            <div className="space-y-4 p-6">
              <h2 className="text-base font-semibold text-[var(--color-ink)]">
                Your device stays yours
              </h2>
              <p className="text-sm leading-relaxed text-[var(--color-ink-muted)]">
                AllInsight has no account and no analytics. It contains no code that sends your data
                anywhere, which is why the switches below have no on position. It goes online only to
                check for updates, and only when you ask it to.
              </p>

              <div className="divide-y divide-[var(--color-line)] rounded-md border border-[var(--color-line)]">
                {["Cloud services", "Telemetry", "Crash reporting"].map((label) => (
                  <div key={label} className="flex items-center justify-between px-4 py-2.5">
                    <span className="text-xs text-[var(--color-ink)]">{label}</span>
                    <Badge tone="neutral">
                      <CircleSlash className="size-2.5" />
                      Off
                    </Badge>
                  </div>
                ))}
              </div>

              <div className="flex gap-2 rounded-md border border-[var(--color-line)] p-3">
                <WifiOff className="mt-0.5 size-3.5 shrink-0 text-[var(--color-ink-subtle)]" />
                <Hint>
                  Disconnect this machine from the network and every feature keeps working exactly
                  as it does now.
                </Hint>
              </div>
            </div>
          ) : null}

          {step === 2 ? (
            <div className="space-y-1 p-2">
              <div className="px-4 pt-4">
                <h2 className="text-base font-semibold text-[var(--color-ink)]">
                  Choose what AllInsight watches
                </h2>
                <p className="mt-1 text-xs text-[var(--color-ink-muted)]">
                  All of this can be changed later in Settings.
                </p>
              </div>

              <div className="divide-y divide-[var(--color-line)]">
                <SettingRow
                  title="Watch storage in the background"
                  description="Checks free space and drive health every few minutes. It reads volume totals only and never walks your files."
                  control={
                    <Switch
                      checked={draft.background_monitoring}
                      onCheckedChange={(v) => update({ background_monitoring: v })}
                      label="Background monitoring"
                    />
                  }
                />
                <SettingRow
                  title="Tell me when storage runs low"
                  description={`A ${w.os} notification at 80, 90 and 95 percent. Once each, not repeatedly.`}
                  control={
                    <Switch
                      checked={draft.notifications_enabled}
                      onCheckedChange={(v) => update({ notifications_enabled: v })}
                      label="Storage notifications"
                    />
                  }
                />
                <SettingRow
                  title="Start AllInsight when I sign in"
                  description="Only for your user account."
                  control={
                    <Switch
                      checked={draft.launch_at_startup}
                      onCheckedChange={(v) => update({ launch_at_startup: v })}
                      label="Start at sign in"
                    />
                  }
                />
                <SettingRow
                  title="Run a storage scan now"
                  description="Reads file sizes to build the storage map. It never opens a file and changes nothing."
                  control={
                    <Switch
                      checked={scanNow}
                      onCheckedChange={setScanNow}
                      label="Scan storage now"
                    />
                  }
                />
              </div>
            </div>
          ) : null}

          {step === 3 ? (
            <div className="space-y-4 p-6">
              <div className="flex size-10 items-center justify-center rounded-full bg-[var(--color-accent-soft)] text-[var(--color-accent)]">
                <Check className="size-5" />
              </div>
              <h2 className="text-base font-semibold text-[var(--color-ink)]">Ready</h2>
              <p className="text-sm leading-relaxed text-[var(--color-ink-muted)]">
                AllInsight is set up. The Overview will show what it can measure straight away, and
                fill in the rest as scans finish.
              </p>

              <div className="rounded-md border border-[var(--color-line)] p-4">
                <p className="text-xs font-medium text-[var(--color-ink)]">
                  The local assistant is optional
                </p>
                <p className="mt-1 text-xs leading-relaxed text-[var(--color-ink-muted)]">
                  AllInsight ships without a language model and never downloads one. Every insight and
                  recommendation works without it. If you want conversation as well, import a GGUF
                  file from the AllInsight AI screen.
                </p>
              </div>
            </div>
          ) : null}

          <div className="flex items-center justify-between border-t border-[var(--color-line)] px-6 py-3">
            <Button
              variant="ghost"
              disabled={step === 0}
              onClick={() => setStep((s) => Math.max(0, s - 1))}
            >
              Back
            </Button>

            {step < STEPS.length - 1 ? (
              <Button
                variant="primary"
                icon={<ArrowRight className="size-3.5" />}
                onClick={() => setStep((s) => s + 1)}
              >
                Continue
              </Button>
            ) : (
              <Button variant="primary" loading={finishing} onClick={finish}>
                Open AllInsight
              </Button>
            )}
          </div>
        </Panel>
      </div>
    </div>
  );
}
