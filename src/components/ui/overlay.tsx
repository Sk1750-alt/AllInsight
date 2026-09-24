/**
 * Dialogs and tooltips.
 *
 * The confirm dialog is the one the safety model depends on, so it is not a
 * generic yes/no box: it takes explicit "what will happen" and "what is not
 * affected" text and shows both, because that is what the user needs to decide.
 */
import * as React from "react";
import * as DialogPrimitive from "@radix-ui/react-dialog";
import * as TooltipPrimitive from "@radix-ui/react-tooltip";
import { X } from "lucide-react";
import { cn } from "@/lib/utils";
import { Button } from "./primitives";

// ------------------------------------------------------------------ Dialog

export function Dialog({
  open,
  onOpenChange,
  title,
  description,
  children,
  footer,
  width = "md",
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description?: React.ReactNode;
  children?: React.ReactNode;
  footer?: React.ReactNode;
  width?: "sm" | "md" | "lg";
}) {
  const widths = { sm: "max-w-sm", md: "max-w-lg", lg: "max-w-2xl" };
  return (
    <DialogPrimitive.Root open={open} onOpenChange={onOpenChange}>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay className="dialog-overlay fixed inset-0 z-50 bg-[rgba(23,24,26,0.28)] backdrop-blur-[6px]" />
        <DialogPrimitive.Content
          className={cn(
            "fixed left-1/2 top-1/2 z-50 w-[calc(100vw-3rem)] -translate-x-1/2 -translate-y-1/2",
            "rounded-2xl border border-[var(--color-line)] bg-[var(--color-surface-raised)] shadow-[var(--shadow-float)]",
            "dialog-enter",
            widths[width],
          )}
        >
          <div className="flex items-start justify-between gap-4 px-5 pt-4">
            <div className="min-w-0">
              <DialogPrimitive.Title className="text-sm font-semibold text-[var(--color-ink)]">
                {title}
              </DialogPrimitive.Title>
              {description ? (
                <DialogPrimitive.Description className="mt-1 text-xs leading-relaxed text-[var(--color-ink-muted)]">
                  {description}
                </DialogPrimitive.Description>
              ) : null}
            </div>
            <DialogPrimitive.Close
              aria-label="Close"
              className="rounded p-1 text-[var(--color-ink-subtle)] transition-quick hover:bg-[var(--color-surface-hover)] hover:text-[var(--color-ink)]"
            >
              <X className="size-4" />
            </DialogPrimitive.Close>
          </div>

          {children ? <div className="px-5 py-4">{children}</div> : <div className="h-2" />}

          {footer ? (
            <div className="flex items-center justify-end gap-2 border-t border-[var(--color-line)] px-5 py-3">
              {footer}
            </div>
          ) : null}
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}

/**
 * The confirmation used before anything is removed.
 *
 * It always shows three things: what will happen, what will not be affected,
 * and how much space is expected back. That trio is what turns a destructive
 * button into an informed decision.
 */
export function ConfirmDialog({
  open,
  onOpenChange,
  title,
  whatHappens,
  whatIsUntouched,
  estimate,
  confirmLabel,
  destructive = true,
  loading,
  onConfirm,
  extra,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  whatHappens: string;
  whatIsUntouched: string;
  estimate?: React.ReactNode;
  confirmLabel: string;
  destructive?: boolean;
  loading?: boolean;
  onConfirm: () => void;
  extra?: React.ReactNode;
}) {
  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title={title}
      footer={
        <>
          <Button variant="ghost" onClick={() => onOpenChange(false)} disabled={loading}>
            Cancel
          </Button>
          <Button
            variant={destructive ? "danger" : "primary"}
            onClick={onConfirm}
            loading={loading}
          >
            {confirmLabel}
          </Button>
        </>
      }
    >
      <div className="space-y-3.5">
        {estimate ? (
          <div className="rounded-md border border-[var(--color-line)] bg-[var(--color-surface)] px-3 py-2.5">
            {estimate}
          </div>
        ) : null}

        <div>
          <p className="text-2xs font-semibold uppercase tracking-wider text-[var(--color-ink-subtle)]">
            What will happen
          </p>
          <p className="mt-1 text-xs leading-relaxed text-[var(--color-ink)]">{whatHappens}</p>
        </div>

        <div>
          <p className="text-2xs font-semibold uppercase tracking-wider text-[var(--color-ink-subtle)]">
            What will not be affected
          </p>
          <p className="mt-1 text-xs leading-relaxed text-[var(--color-ink-muted)]">
            {whatIsUntouched}
          </p>
        </div>

        {extra}
      </div>
    </Dialog>
  );
}

// ----------------------------------------------------------------- Tooltip

export function TooltipProvider({ children }: { children: React.ReactNode }) {
  return (
    <TooltipPrimitive.Provider delayDuration={400} skipDelayDuration={200}>
      {children}
    </TooltipPrimitive.Provider>
  );
}

export function Tooltip({
  content,
  children,
  side = "top",
}: {
  content: React.ReactNode;
  children: React.ReactNode;
  side?: "top" | "right" | "bottom" | "left";
}) {
  if (!content) return <>{children}</>;
  return (
    <TooltipPrimitive.Root>
      <TooltipPrimitive.Trigger asChild>{children}</TooltipPrimitive.Trigger>
      <TooltipPrimitive.Portal>
        <TooltipPrimitive.Content
          side={side}
          sideOffset={6}
          className={cn(
            "z-50 max-w-xs rounded-md border border-[var(--color-line-strong)] bg-[var(--color-surface-raised)]",
            "px-2.5 py-1.5 text-xs leading-relaxed text-[var(--color-ink)] shadow-xl",
          )}
        >
          {content}
        </TooltipPrimitive.Content>
      </TooltipPrimitive.Portal>
    </TooltipPrimitive.Root>
  );
}
