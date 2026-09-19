/**
 * AllInsight AI.
 *
 * Two things are always true on this screen and both are visible in it: the
 * assistant answers from measurements AllInsight took, and it cannot act. Every
 * button under an answer is a link to a screen, chosen by the backend from the
 * same measurements, never parsed out of what the model wrote.
 */
import * as React from "react";
import {
  Brain,
  CircleSlash,
  Cpu,
  Eye,
  FileDown,
  Send,
  ShieldCheck,
  Trash2,
  WifiOff,
} from "lucide-react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";

import { api } from "@/lib/api";
import { useAsync, useStore } from "@/app/store";
import { routeForAction } from "@/app/navigation";
import {
  Badge,
  Button,
  EmptyState,
  Hint,
  Panel,
  PanelHeader,
  PageHeader,
  Spinner,
} from "@/components/ui/primitives";
import { Dialog } from "@/components/ui/overlay";
import { formatBytes } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { AiAnswer, EngineStatus, ModelInventory } from "@/lib/types";

interface Turn {
  id: number;
  role: "user" | "assistant";
  text: string;
  answer?: AiAnswer;
}

const SUGGESTIONS = [
  "Why is my laptop storage full?",
  "What is safe to clean right now?",
  "Is my drive healthy?",
  "What should I review before deleting anything?",
];

let turnId = 0;

export function AssistantView() {
  const { navigate, toast, reportError } = useStore();
  const status = useAsync<EngineStatus>(() => api.getAiStatus(), []);
  const models = useAsync<ModelInventory>(() => api.getLocalModels(), []);

  const [turns, setTurns] = React.useState<Turn[]>([]);
  const [question, setQuestion] = React.useState("");
  const [thinking, setThinking] = React.useState(false);
  const [loadingModel, setLoadingModel] = React.useState(false);
  const [contextOpen, setContextOpen] = React.useState(false);
  const context = useAsync<string>(() => api.getAiContext(), [contextOpen]);
  const scrollRef = React.useRef<HTMLDivElement>(null);

  React.useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight, behavior: "smooth" });
  }, [turns, thinking]);

  const ask = async (text: string) => {
    const trimmed = text.trim();
    if (!trimmed || thinking) return;
    setQuestion("");
    setTurns((current) => [...current, { id: ++turnId, role: "user", text: trimmed }]);
    setThinking(true);
    try {
      const answer = await api.askAi(trimmed);
      setTurns((current) => [
        ...current,
        { id: ++turnId, role: "assistant", text: answer.text, answer },
      ]);
    } catch (e) {
      reportError(e, "The assistant could not answer that.");
    } finally {
      setThinking(false);
    }
  };

  const importModel = async () => {
    try {
      const selected = await openFileDialog({
        multiple: false,
        title: "Choose a GGUF model",
        filters: [{ name: "GGUF model", extensions: ["gguf"] }],
      });
      if (typeof selected !== "string") return;
      const model = await api.importLocalModel(selected);
      toast({
        tone: "success",
        title: `${model.name} imported`,
        body: `${model.size_label} · needs about ${model.estimated_ram_label} of memory.`,
      });
      models.reload();
    } catch (e) {
      reportError(e, "That model could not be imported.");
    }
  };

  const loadModel = async (path?: string) => {
    setLoadingModel(true);
    try {
      const next = await api.loadAiModel(path);
      toast({ tone: "success", title: next.message });
      status.reload();
    } catch (e) {
      reportError(e, "The local model could not be loaded.");
    } finally {
      setLoadingModel(false);
    }
  };

  const unload = async () => {
    await api.unloadAiModel();
    status.reload();
    toast({ tone: "info", title: "The local model was unloaded and its memory released." });
  };

  const ready = status.data?.state === "ready";

  return (
    <div className="view-enter grid gap-5 lg:grid-cols-[1fr_320px]">
      <div className="flex min-h-[calc(100vh-140px)] flex-col">
        <PageHeader
          title="AllInsight AI"
          subtitle="Private by design. It reads what AllInsight measured, and nothing else."
          actions={
            <>
              <Button
                variant="ghost"
                icon={<Eye className="size-3.5" />}
                onClick={() => setContextOpen(true)}
              >
                See what it is given
              </Button>
              {turns.length > 0 ? (
                <Button
                  variant="ghost"
                  icon={<Trash2 className="size-3.5" />}
                  onClick={() => setTurns([])}
                >
                  Clear
                </Button>
              ) : null}
            </>
          }
        />

        <Panel className="flex min-h-0 flex-1 flex-col">
          <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto p-4">
            {turns.length === 0 ? (
              <div className="flex h-full flex-col items-center justify-center gap-5 text-center">
                <div className="flex size-11 items-center justify-center rounded-full bg-[var(--color-accent-soft)] text-[var(--color-accent)]">
                  <Brain className="size-5" />
                </div>
                <div className="max-w-md">
                  <p className="text-sm font-medium text-[var(--color-ink)]">
                    Ask about this device
                  </p>
                  <p className="mt-1.5 text-xs leading-relaxed text-[var(--color-ink-muted)]">
                    Answers come from measurements taken on this computer. Nothing you type
                    leaves the device, with or without a local model loaded.
                  </p>
                </div>
                <div className="flex flex-wrap justify-center gap-2">
                  {SUGGESTIONS.map((suggestion) => (
                    <Button
                      key={suggestion}
                      size="sm"
                      variant="subtle"
                      onClick={() => ask(suggestion)}
                    >
                      {suggestion}
                    </Button>
                  ))}
                </div>
              </div>
            ) : (
              <div className="space-y-4">
                {turns.map((turn) =>
                  turn.role === "user" ? (
                    <div key={turn.id} className="flex justify-end">
                      <p className="max-w-[80%] rounded-lg rounded-br-sm bg-[var(--color-accent-soft)] px-3 py-2 text-xs text-[var(--color-ink)]">
                        {turn.text}
                      </p>
                    </div>
                  ) : (
                    <div key={turn.id} className="max-w-[85%] space-y-2.5">
                      <div className="rounded-lg rounded-bl-sm border border-[var(--color-line)] bg-[var(--color-surface-raised)] px-3.5 py-3">
                        <p
                          data-selectable
                          className="whitespace-pre-line text-xs leading-relaxed text-[var(--color-ink)]"
                        >
                          {turn.text}
                        </p>
                        <div className="mt-2.5 flex items-center gap-2 border-t border-[var(--color-line)] pt-2">
                          <Badge tone={turn.answer?.from_model ? "accent" : "neutral"} dot>
                            {turn.answer?.from_model
                              ? (turn.answer.model_name ?? "Local model")
                              : "From measurements"}
                          </Badge>
                          <span className="text-2xs text-[var(--color-ink-subtle)]">
                            AllInsight AI cannot change anything on this device.
                          </span>
                        </div>
                      </div>

                      {turn.answer && turn.answer.actions.length > 0 ? (
                        <div className="flex flex-wrap gap-2">
                          {turn.answer.actions.map((action) => {
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
                      ) : null}
                    </div>
                  ),
                )}

                {thinking ? (
                  <div className="flex items-center gap-2 text-xs text-[var(--color-ink-muted)]">
                    <Spinner />
                    {ready ? "The local model is thinking..." : "Reading the measurements..."}
                  </div>
                ) : null}
              </div>
            )}
          </div>

          <form
            onSubmit={(e) => {
              e.preventDefault();
              ask(question);
            }}
            className="flex items-center gap-2 border-t border-[var(--color-line)] p-3"
          >
            <input
              value={question}
              onChange={(e) => setQuestion(e.target.value)}
              placeholder="Ask about storage, performance, drives or cleanup"
              className="h-9 flex-1 rounded-md border border-[var(--color-line-strong)] bg-[var(--color-surface)] px-3 text-xs text-[var(--color-ink)] outline-none transition-quick placeholder:text-[var(--color-ink-subtle)] focus:border-[var(--color-accent)]"
            />
            <Button
              type="submit"
              variant="primary"
              disabled={!question.trim() || thinking}
              icon={<Send className="size-3.5" />}
            >
              Ask
            </Button>
          </form>
        </Panel>
      </div>

      {/* The engine panel. */}
      <div className="space-y-5">
        <Panel>
          <PanelHeader
            title="Local model"
            actions={
              <Badge tone={ready ? "ok" : status.data?.state === "failed" ? "danger" : "neutral"} dot>
                {ready
                  ? "Loaded"
                  : status.data?.state === "starting"
                    ? "Loading"
                    : status.data?.state === "failed"
                      ? "Failed"
                      : "Not loaded"}
              </Badge>
            }
          />
          <div className="space-y-3 p-4">
            <p className="text-xs leading-relaxed text-[var(--color-ink-muted)]">
              {status.data?.message}
            </p>

            {ready ? (
              <Button size="sm" variant="secondary" className="w-full" onClick={unload}>
                Unload and free memory
              </Button>
            ) : models.data?.models.length ? (
              <div className="space-y-2">
                {models.data.models.map((model) => (
                  <div
                    key={model.path}
                    className="rounded-md border border-[var(--color-line)] p-2.5"
                  >
                    <div className="flex items-start justify-between gap-2">
                      <div className="min-w-0">
                        <p className="truncate text-xs font-medium text-[var(--color-ink)]">
                          {model.name}
                        </p>
                        <p className="mt-0.5 text-2xs text-[var(--color-ink-subtle)]">
                          {model.size_label}
                          {model.quantisation ? ` · ${model.quantisation}` : ""}
                          {model.parameter_label ? ` · ${model.parameter_label}` : ""}
                        </p>
                        <p
                          className={cn(
                            "mt-0.5 text-2xs",
                            model.fits_in_memory
                              ? "text-[var(--color-ink-subtle)]"
                              : "text-[var(--color-warn)]",
                          )}
                        >
                          Needs about {model.estimated_ram_label} of memory
                          {model.fits_in_memory ? "" : " - more than this device has free"}
                        </p>
                      </div>
                    </div>
                    <div className="mt-2 flex gap-1.5">
                      <Button
                        size="sm"
                        variant="primary"
                        loading={loadingModel}
                        disabled={!models.data?.engine_present || !model.is_valid_gguf}
                        onClick={() => loadModel(model.path)}
                      >
                        Load
                      </Button>
                      <Button
                        size="sm"
                        variant="ghost"
                        onClick={async () => {
                          try {
                            await api.removeLocalModel(model.path);
                            models.reload();
                          } catch (e) {
                            reportError(e);
                          }
                        }}
                      >
                        Remove
                      </Button>
                    </div>
                  </div>
                ))}
              </div>
            ) : (
              <EmptyState
                className="py-6"
                icon={<Cpu className="size-4" />}
                title="No model installed"
                description="AllInsight ships without a model and never downloads one. Import a GGUF file to enable conversation."
              />
            )}

            <Button
              size="sm"
              variant="secondary"
              className="w-full"
              icon={<FileDown className="size-3.5" />}
              onClick={importModel}
            >
              Import a GGUF model
            </Button>

            {models.data && !models.data.engine_present ? (
              <div className="rounded-md border border-[color-mix(in_srgb,var(--color-warn)_35%,transparent)] bg-[var(--color-warn-soft)] p-2.5">
                <p className="text-2xs leading-relaxed text-[var(--color-ink)]">
                  The inference engine was not found. Put <code>llama-server.exe</code> in{" "}
                  <span className="font-mono">
                    {models.data.directory.replace(/models$/, "engine")}
                  </span>
                  , or choose it in Settings.
                </p>
              </div>
            ) : null}

            {models.data ? (
              <div className="space-y-2 rounded-md border border-[var(--color-line)] p-2.5">
                <p className="text-2xs font-semibold uppercase tracking-wider text-[var(--color-ink-subtle)]">
                  What fits on this device
                </p>
                <p className="text-2xs leading-relaxed text-[var(--color-ink-muted)]">
                  {models.data.recommendation}
                </p>
                <div className="flex justify-between text-2xs text-[var(--color-ink-subtle)]">
                  <span>{formatBytes(models.data.total_memory_bytes)} memory</span>
                  <span>{formatBytes(models.data.free_disk_bytes)} free on disk</span>
                </div>
                {models.data.suggested_tier ? (
                  <div className="flex items-baseline justify-between border-t border-[var(--color-line)] pt-2">
                    <span className="text-xs font-medium text-[var(--color-ink)]">
                      {models.data.suggested_tier.parameters} ·{" "}
                      {models.data.suggested_tier.quantisation}
                    </span>
                    <span className="numeric text-2xs text-[var(--color-accent)]">
                      ~{formatBytes(models.data.suggested_tier.disk_bytes)}
                    </span>
                  </div>
                ) : null}
              </div>
            ) : null}
          </div>
        </Panel>

        <Panel>
          <PanelHeader title="What the assistant can and cannot do" />
          <div className="space-y-2.5 p-4">
            {[
              { icon: ShieldCheck, text: "Reads measurements AllInsight already took." },
              { icon: WifiOff, text: "Runs entirely on this device, with no network access." },
              { icon: CircleSlash, text: "Cannot delete, move or change anything." },
              { icon: CircleSlash, text: "Cannot run commands, and its replies are never executed." },
            ].map(({ icon: Icon, text }) => (
              <div key={text} className="flex gap-2">
                <Icon className="mt-0.5 size-3.5 shrink-0 text-[var(--color-ink-subtle)]" />
                <p className="text-xs leading-relaxed text-[var(--color-ink-muted)]">{text}</p>
              </div>
            ))}
          </div>
        </Panel>

        <Hint>
          Questions and answers are not written to disk. Clearing the conversation removes it
          entirely.
        </Hint>
      </div>

      <Dialog
        open={contextOpen}
        onOpenChange={setContextOpen}
        title="What the assistant is given"
        description="Exactly this text, and nothing else, is placed in front of the local model. No file contents, no file names, no paths beyond the folders shown here."
        width="lg"
        footer={
          <Button variant="secondary" onClick={() => setContextOpen(false)}>
            Close
          </Button>
        }
      >
        <pre
          data-selectable
          className="max-h-80 overflow-auto whitespace-pre-wrap rounded-md border border-[var(--color-line)] bg-[var(--color-canvas)] p-3 font-mono text-2xs leading-relaxed text-[var(--color-ink-muted)]"
        >
          {context.loading ? "Reading measurements..." : (context.data ?? "Nothing measured yet.")}
        </pre>
      </Dialog>
    </div>
  );
}
