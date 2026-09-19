/**
 * Data display: tables, meters, and the small charts.
 *
 * The charts are hand-drawn SVG rather than a charting library. They are
 * simple shapes, they need to match the palette exactly, and a dependency
 * that ships its own theme would fight the design tokens on every screen.
 */
import * as React from "react";
import { cn, toneClasses } from "@/lib/utils";
import { formatBytesShort, formatPercent } from "@/lib/format";

// ------------------------------------------------------------------- Table

export function Table({
  children,
  className,
}: {
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <div className="w-full overflow-x-auto">
      <table className={cn("w-full border-collapse text-xs", className)}>{children}</table>
    </div>
  );
}

export function Th({
  children,
  className,
  align = "left",
  sortable,
  active,
  direction,
  onClick,
}: {
  children: React.ReactNode;
  className?: string;
  align?: "left" | "right" | "center";
  sortable?: boolean;
  active?: boolean;
  direction?: "asc" | "desc";
  onClick?: () => void;
}) {
  const content = (
    <span className="inline-flex items-center gap-1">
      {children}
      {sortable && active ? (
        <span aria-hidden className="text-[var(--color-accent)]">
          {direction === "asc" ? "↑" : "↓"}
        </span>
      ) : null}
    </span>
  );
  return (
    <th
      scope="col"
      aria-sort={active ? (direction === "asc" ? "ascending" : "descending") : undefined}
      className={cn(
        "sticky top-0 z-10 border-b border-[var(--color-line)] bg-[var(--color-surface)]",
        "px-3 py-2 text-2xs font-semibold uppercase tracking-wider text-[var(--color-ink-subtle)]",
        align === "right" && "text-right",
        align === "center" && "text-center",
        sortable && "cursor-pointer select-none hover:text-[var(--color-ink)]",
        className,
      )}
      onClick={onClick}
    >
      {sortable ? (
        <button type="button" className="w-full text-inherit" onClick={onClick}>
          {content}
        </button>
      ) : (
        content
      )}
    </th>
  );
}

export function Td({
  children,
  className,
  align = "left",
  title,
}: {
  children: React.ReactNode;
  className?: string;
  align?: "left" | "right" | "center";
  title?: string;
}) {
  return (
    <td
      title={title}
      className={cn(
        "border-b border-[var(--color-line)] px-3 py-2 text-[var(--color-ink)]",
        align === "right" && "text-right numeric",
        align === "center" && "text-center",
        className,
      )}
    >
      {children}
    </td>
  );
}

export function Tr({
  children,
  className,
  selected,
  onClick,
}: {
  children: React.ReactNode;
  className?: string;
  selected?: boolean;
  onClick?: () => void;
}) {
  return (
    <tr
      onClick={onClick}
      className={cn(
        "transition-quick",
        onClick && "cursor-pointer",
        selected
          ? "bg-[var(--color-accent-soft)]"
          : "hover:bg-[var(--color-surface-hover)]",
        className,
      )}
    >
      {children}
    </tr>
  );
}

// -------------------------------------------------------------- Progress

export function ProgressBar({
  value,
  tone = "accent",
  className,
  label,
  height = 6,
}: {
  /** 0-100. */
  value: number;
  tone?: "ok" | "warn" | "danger" | "unknown" | "accent" | "neutral";
  className?: string;
  label?: string;
  height?: number;
}) {
  const clamped = Math.max(0, Math.min(100, value));
  const t = toneClasses(tone);
  return (
    <div
      role="progressbar"
      aria-valuenow={Math.round(clamped)}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-label={label}
      className={cn("w-full overflow-hidden rounded-full bg-[var(--color-surface-hover)]", className)}
      style={{ height }}
    >
      <div
        className={cn("h-full rounded-full transition-quick", t.dot)}
        style={{ width: `${clamped}%` }}
      />
    </div>
  );
}

/** Progress with no known end, used while a scan is walking the disk. */
export function IndeterminateBar({ className }: { className?: string }) {
  return (
    <div
      className={cn(
        "relative h-1 w-full overflow-hidden rounded-full bg-[var(--color-surface-hover)]",
        className,
      )}
      role="progressbar"
      aria-label="Working"
    >
      <div className="absolute inset-y-0 -left-1/4 w-1/4 animate-indeterminate rounded-full bg-[var(--color-accent)]" />
    </div>
  );
}

/**
 * The circular device score.
 *
 * A partial reading is drawn with a dashed track so "we could not measure
 * everything" is visible at a glance rather than buried in a footnote.
 */
export function ScoreRing({
  score,
  label,
  partial,
  size = 132,
}: {
  score: number;
  label: string;
  partial?: boolean;
  size?: number;
}) {
  const stroke = 9;
  const radius = (size - stroke) / 2;
  const circumference = 2 * Math.PI * radius;
  const clamped = Math.max(0, Math.min(100, score));
  const offset = circumference - (clamped / 100) * circumference;
  const tone = clamped >= 85 ? "ok" : clamped >= 65 ? "accent" : clamped >= 45 ? "warn" : "danger";
  const strokeColor = {
    ok: "var(--color-ok)",
    accent: "var(--color-accent)",
    warn: "var(--color-warn)",
    danger: "var(--color-danger)",
  }[tone];

  return (
    <div className="relative inline-flex items-center justify-center" style={{ width: size, height: size }}>
      <svg width={size} height={size} className="-rotate-90" aria-hidden>
        <circle
          cx={size / 2}
          cy={size / 2}
          r={radius}
          fill="none"
          stroke="var(--color-surface-hover)"
          strokeWidth={stroke}
          strokeDasharray={partial ? "4 6" : undefined}
        />
        <circle
          cx={size / 2}
          cy={size / 2}
          r={radius}
          fill="none"
          stroke={strokeColor}
          strokeWidth={stroke}
          strokeLinecap="round"
          strokeDasharray={circumference}
          strokeDashoffset={offset}
          style={{ transition: "stroke-dashoffset 400ms cubic-bezier(0.2,0,0.2,1)" }}
        />
      </svg>
      <div className="absolute inset-0 flex flex-col items-center justify-center">
        <span
          className="numeric font-display text-3xl font-semibold leading-none"
          style={{ color: strokeColor }}
        >
          {Math.round(clamped)}
        </span>
        <span className="mt-1 text-2xs uppercase tracking-wider text-[var(--color-ink-muted)]">
          {label}
        </span>
      </div>
    </div>
  );
}

// -------------------------------------------------------------- Sparkline

/**
 * A filled area chart for a metric over time. Points are normalised to the
 * range given rather than to the data, so a flat quiet line stays flat instead
 * of being stretched into drama.
 */
export function Sparkline({
  values,
  max = 100,
  height = 44,
  color = "var(--color-accent)",
  className,
  showLast,
  formatValue,
}: {
  values: number[];
  max?: number;
  height?: number;
  color?: string;
  className?: string;
  showLast?: boolean;
  formatValue?: (value: number) => string;
}) {
  const width = 240;
  if (values.length === 0) {
    return (
      <div
        className={cn("flex items-center justify-center text-2xs text-[var(--color-ink-subtle)]", className)}
        style={{ height }}
      >
        No samples yet
      </div>
    );
  }

  const points = values.slice(-120);
  const step = points.length > 1 ? width / (points.length - 1) : width;
  const scale = (v: number) => height - Math.max(0, Math.min(1, v / max)) * (height - 2) - 1;

  const line = points.map((v, i) => `${i * step},${scale(v)}`).join(" ");
  const area = `0,${height} ${line} ${(points.length - 1) * step},${height}`;
  const last = points[points.length - 1];

  return (
    <div className={cn("relative", className)}>
      <svg
        viewBox={`0 0 ${width} ${height}`}
        preserveAspectRatio="none"
        className="w-full"
        style={{ height }}
        aria-hidden
      >
        <polygon points={area} fill={color} opacity={0.12} />
        <polyline
          points={line}
          fill="none"
          stroke={color}
          strokeWidth={1.5}
          vectorEffect="non-scaling-stroke"
          strokeLinejoin="round"
        />
      </svg>
      {showLast ? (
        <span className="numeric absolute right-0 top-0 text-2xs text-[var(--color-ink-muted)]">
          {formatValue ? formatValue(last) : formatPercent(last)}
        </span>
      ) : null}
    </div>
  );
}

// ------------------------------------------------------------ Category bar

/** A single stacked bar showing how a total splits between categories. */
export function StackedBar({
  segments,
  className,
  height = 8,
}: {
  segments: { label: string; value: number; color: string }[];
  className?: string;
  height?: number;
}) {
  const total = segments.reduce((sum, s) => sum + s.value, 0) || 1;
  return (
    <div
      className={cn("flex w-full overflow-hidden rounded-full bg-[var(--color-surface-hover)]", className)}
      style={{ height }}
    >
      {segments.map((segment) => (
        <div
          key={segment.label}
          title={`${segment.label}: ${formatBytesShort(segment.value)}`}
          style={{ width: `${(segment.value / total) * 100}%`, background: segment.color }}
          className="h-full first:rounded-l-full last:rounded-r-full"
        />
      ))}
    </div>
  );
}

/** The colour ramp used for category breakdowns, ordered by prominence. */
export const CATEGORY_COLORS = [
  "#31b0c6",
  "#5c8ed6",
  "#8a7fd4",
  "#c07ac0",
  "#d4795f",
  "#c9a227",
  "#78a860",
  "#4fa39a",
  "#6f7f8a",
  "#4c5b66",
];

export function categoryColor(index: number): string {
  return CATEGORY_COLORS[index % CATEGORY_COLORS.length];
}
