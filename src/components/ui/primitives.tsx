/**
 * The base interface pieces.
 *
 * Kept small and unstyled-by-default so screens compose them rather than
 * inventing one-off variants. Anything that appears on three screens lives
 * here; anything that appears on one lives with that screen.
 */
import * as React from "react";
import { Loader2 } from "lucide-react";
import { cn, toneClasses } from "@/lib/utils";

// ------------------------------------------------------------------ Button

type ButtonVariant = "primary" | "secondary" | "ghost" | "danger" | "subtle";
type ButtonSize = "sm" | "md";

export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  loading?: boolean;
  icon?: React.ReactNode;
}

const VARIANTS: Record<ButtonVariant, string> = {
  primary:
    "bg-[var(--color-accent)] text-[var(--color-accent-ink)] hover:bg-[var(--color-accent-hover)] border border-transparent font-medium",
  secondary:
    "bg-[var(--color-surface-raised)] text-[var(--color-ink)] border border-[var(--color-line-strong)] hover:bg-[var(--color-surface-hover)]",
  ghost:
    "bg-transparent text-[var(--color-ink-muted)] border border-transparent hover:bg-[var(--color-surface-hover)] hover:text-[var(--color-ink)]",
  danger:
    "bg-[var(--color-danger)] text-white border border-transparent hover:opacity-90 font-medium",
  subtle:
    "bg-[var(--color-surface-hover)] text-[var(--color-ink)] border border-[var(--color-line)] hover:border-[var(--color-line-strong)]",
};

const SIZES: Record<ButtonSize, string> = {
  sm: "h-7 px-2.5 text-xs gap-1.5 rounded-md",
  md: "h-8 px-3 text-sm gap-2 rounded-md",
};

export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  { variant = "secondary", size = "md", loading, icon, className, children, disabled, ...rest },
  ref,
) {
  return (
    <button
      ref={ref}
      disabled={disabled || loading}
      className={cn(
        "inline-flex items-center justify-center whitespace-nowrap transition-quick",
        "disabled:opacity-45 disabled:pointer-events-none",
        VARIANTS[variant],
        SIZES[size],
        className,
      )}
      {...rest}
    >
      {loading ? <Loader2 className="size-3.5 animate-spin" aria-hidden /> : icon}
      {children}
    </button>
  );
});

/** A square button holding a single icon. Always needs an accessible label. */
export const IconButton = React.forwardRef<
  HTMLButtonElement,
  ButtonProps & { label: string }
>(function IconButton({ label, className, size = "md", ...rest }, ref) {
  return (
    <Button
      ref={ref}
      aria-label={label}
      title={label}
      size={size}
      className={cn(size === "sm" ? "w-7 px-0" : "w-8 px-0", className)}
      {...rest}
    />
  );
});

// ------------------------------------------------------------------- Panel

export function Panel({
  className,
  children,
  ...rest
}: React.HTMLAttributes<HTMLDivElement>) {
  return (
    <section className={cn("panel", className)} {...rest}>
      {children}
    </section>
  );
}

export function PanelHeader({
  title,
  description,
  actions,
  className,
}: {
  title: React.ReactNode;
  description?: React.ReactNode;
  actions?: React.ReactNode;
  className?: string;
}) {
  return (
    <header
      className={cn(
        "flex items-start justify-between gap-4 px-4 py-3 hairline",
        className,
      )}
    >
      <div className="min-w-0">
        <h2 className="text-sm font-semibold text-[var(--color-ink)] truncate">{title}</h2>
        {description ? (
          <p className="mt-0.5 text-xs text-[var(--color-ink-muted)]">{description}</p>
        ) : null}
      </div>
      {actions ? <div className="flex shrink-0 items-center gap-2">{actions}</div> : null}
    </header>
  );
}

export function PanelBody({
  className,
  children,
}: {
  className?: string;
  children: React.ReactNode;
}) {
  return <div className={cn("p-4", className)}>{children}</div>;
}

// ------------------------------------------------------------------- Badge

export function Badge({
  tone = "neutral",
  children,
  className,
  dot,
}: {
  tone?: "ok" | "warn" | "danger" | "unknown" | "accent" | "neutral";
  children: React.ReactNode;
  className?: string;
  dot?: boolean;
}) {
  const t = toneClasses(tone);
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-2xs font-medium",
        t.bg,
        t.text,
        t.border,
        className,
      )}
    >
      {dot ? <span className={cn("size-1.5 rounded-full", t.dot)} aria-hidden /> : null}
      {children}
    </span>
  );
}

// ------------------------------------------------------------- Page header

export function PageHeader({
  title,
  subtitle,
  actions,
}: {
  title: string;
  subtitle?: React.ReactNode;
  actions?: React.ReactNode;
}) {
  return (
    <div className="flex items-end justify-between gap-6 pb-4">
      <div className="min-w-0">
        <h1 className="text-xl font-semibold tracking-tight text-[var(--color-ink)]">{title}</h1>
        {subtitle ? (
          <p className="mt-1 text-sm text-[var(--color-ink-muted)]">{subtitle}</p>
        ) : null}
      </div>
      {actions ? <div className="flex shrink-0 items-center gap-2">{actions}</div> : null}
    </div>
  );
}

// ------------------------------------------------------------ Empty states

export function EmptyState({
  icon,
  title,
  description,
  action,
  className,
}: {
  icon?: React.ReactNode;
  title: string;
  description?: React.ReactNode;
  action?: React.ReactNode;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "flex flex-col items-center justify-center gap-3 px-6 py-14 text-center",
        className,
      )}
    >
      {icon ? (
        <div className="flex size-10 items-center justify-center rounded-full bg-[var(--color-surface-hover)] text-[var(--color-ink-subtle)]">
          {icon}
        </div>
      ) : null}
      <div>
        <p className="text-sm font-medium text-[var(--color-ink)]">{title}</p>
        {description ? (
          <p className="mx-auto mt-1 max-w-md text-xs leading-relaxed text-[var(--color-ink-muted)]">
            {description}
          </p>
        ) : null}
      </div>
      {action}
    </div>
  );
}

export function Spinner({ className }: { className?: string }) {
  return <Loader2 className={cn("size-4 animate-spin", className)} aria-hidden />;
}

export function LoadingBlock({ label = "Loading" }: { label?: string }) {
  return (
    <div className="flex items-center justify-center gap-2 px-6 py-12 text-xs text-[var(--color-ink-muted)]">
      <Spinner />
      {label}
    </div>
  );
}

/** A skeleton line, so a panel keeps its shape while data arrives. */
export function Skeleton({ className }: { className?: string }) {
  return (
    <div
      className={cn(
        "relative overflow-hidden rounded bg-[var(--color-surface-hover)]",
        className,
      )}
      aria-hidden
    >
      <div className="absolute inset-y-0 -left-full w-1/3 animate-indeterminate bg-[color-mix(in_srgb,var(--color-ink)_6%,transparent)]" />
    </div>
  );
}

// ------------------------------------------------------------------ Layout

export function KeyValue({
  label,
  value,
  hint,
}: {
  label: string;
  value: React.ReactNode;
  hint?: string;
}) {
  return (
    <div className="flex items-baseline justify-between gap-4 py-1.5">
      <dt className="text-xs text-[var(--color-ink-muted)]">{label}</dt>
      <dd
        className="numeric text-right text-xs font-medium text-[var(--color-ink)]"
        title={hint}
      >
        {value}
      </dd>
    </div>
  );
}

export function Divider({ className }: { className?: string }) {
  return <hr className={cn("border-t border-[var(--color-line)]", className)} />;
}

/** Short explanatory text used under headings and beside controls. */
export function Hint({ children, className }: { children: React.ReactNode; className?: string }) {
  return (
    <p className={cn("text-xs leading-relaxed text-[var(--color-ink-muted)]", className)}>
      {children}
    </p>
  );
}
