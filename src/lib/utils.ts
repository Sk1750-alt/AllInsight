import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

/** Merge Tailwind classes, letting later classes win over earlier ones. */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

/** State colour for a health-like value, used by badges, rings and rows. */
export function toneClasses(
  tone: "ok" | "warn" | "danger" | "unknown" | "accent" | "neutral",
): { text: string; bg: string; border: string; dot: string } {
  switch (tone) {
    case "ok":
      return {
        text: "text-[var(--color-ok)]",
        bg: "bg-[var(--color-ok-soft)]",
        border: "border-[color-mix(in_srgb,var(--color-ok)_35%,transparent)]",
        dot: "bg-[var(--color-ok)]",
      };
    case "warn":
      return {
        text: "text-[var(--color-warn)]",
        bg: "bg-[var(--color-warn-soft)]",
        border: "border-[color-mix(in_srgb,var(--color-warn)_35%,transparent)]",
        dot: "bg-[var(--color-warn)]",
      };
    case "danger":
      return {
        text: "text-[var(--color-danger)]",
        bg: "bg-[var(--color-danger-soft)]",
        border: "border-[color-mix(in_srgb,var(--color-danger)_35%,transparent)]",
        dot: "bg-[var(--color-danger)]",
      };
    case "accent":
      return {
        text: "text-[var(--color-accent)]",
        bg: "bg-[var(--color-accent-soft)]",
        border: "border-[color-mix(in_srgb,var(--color-accent)_35%,transparent)]",
        dot: "bg-[var(--color-accent)]",
      };
    case "neutral":
      return {
        text: "text-[var(--color-ink-muted)]",
        bg: "bg-[var(--color-surface-hover)]",
        border: "border-[var(--color-line)]",
        dot: "bg-[var(--color-ink-subtle)]",
      };
    default:
      return {
        text: "text-[var(--color-unknown)]",
        bg: "bg-[var(--color-unknown-soft)]",
        border: "border-[var(--color-line)]",
        dot: "bg-[var(--color-unknown)]",
      };
  }
}

/** How full a volume is, expressed as a tone rather than a raw number. */
export function usageTone(percent: number): "ok" | "warn" | "danger" {
  if (percent >= 90) return "danger";
  if (percent >= 80) return "warn";
  return "ok";
}

export function healthTone(state: string): "ok" | "warn" | "danger" | "unknown" {
  switch (state) {
    case "healthy":
      return "ok";
    case "warning":
      return "warn";
    case "critical":
      return "danger";
    default:
      return "unknown";
  }
}

export function severityTone(
  severity: string,
): "ok" | "warn" | "danger" | "unknown" | "accent" | "neutral" {
  switch (severity) {
    case "critical":
      return "danger";
    case "warning":
      return "warn";
    case "advice":
      return "accent";
    case "positive":
      return "ok";
    default:
      return "neutral";
  }
}

/** Wait, used to keep spinners on screen long enough to be readable. */
export function delay(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
