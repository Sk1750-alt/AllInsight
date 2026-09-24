/**
 * Form controls.
 *
 * Radix supplies the behaviour (keyboard handling, focus management, ARIA);
 * the styling here keeps them looking like Windows controls rather than web
 * ones: small, square-ish, and quiet until you interact with them.
 */
import * as React from "react";
import * as SwitchPrimitive from "@radix-ui/react-switch";
import * as SelectPrimitive from "@radix-ui/react-select";
import * as CheckboxPrimitive from "@radix-ui/react-checkbox";
import { Check, ChevronDown, Search } from "lucide-react";
import { cn } from "@/lib/utils";

// ------------------------------------------------------------------ Switch

export function Switch({
  checked,
  onCheckedChange,
  disabled,
  label,
  id,
}: {
  checked: boolean;
  onCheckedChange: (value: boolean) => void;
  disabled?: boolean;
  label: string;
  id?: string;
}) {
  return (
    <SwitchPrimitive.Root
      id={id}
      checked={checked}
      onCheckedChange={onCheckedChange}
      disabled={disabled}
      aria-label={label}
      className={cn(
        "relative h-[22px] w-[38px] shrink-0 rounded-full p-[2px] transition-colors duration-300",
        "disabled:opacity-40 disabled:pointer-events-none",
        checked ? "bg-[var(--color-accent)]" : "bg-[var(--color-line-strong)]",
      )}
      style={{ transitionTimingFunction: "var(--ease-out)" }}
    >
      <SwitchPrimitive.Thumb
        className={cn(
          "block size-[18px] rounded-full bg-white shadow-[0_1px_3px_rgba(0,0,0,0.25),0_0_0_0.5px_rgba(0,0,0,0.06)]",
          "transition-transform duration-300",
          checked ? "translate-x-4" : "translate-x-0",
        )}
        style={{ transitionTimingFunction: "var(--ease-spring)" }}
      />
    </SwitchPrimitive.Root>
  );
}

/** A labelled setting row: title, explanation, and the control on the right. */
export function SettingRow({
  title,
  description,
  control,
  disabled,
}: {
  title: string;
  description?: React.ReactNode;
  control: React.ReactNode;
  disabled?: boolean;
}) {
  return (
    <div
      className={cn(
        "flex items-start justify-between gap-6 px-4 py-3",
        disabled && "opacity-50",
      )}
    >
      <div className="min-w-0">
        <p className="text-xs font-medium text-[var(--color-ink)]">{title}</p>
        {description ? (
          <p className="mt-0.5 text-xs leading-relaxed text-[var(--color-ink-muted)]">
            {description}
          </p>
        ) : null}
      </div>
      <div className="flex shrink-0 items-center pt-0.5">{control}</div>
    </div>
  );
}

// ---------------------------------------------------------------- Checkbox

export function Checkbox({
  checked,
  onCheckedChange,
  disabled,
  label,
}: {
  checked: boolean | "indeterminate";
  onCheckedChange: (value: boolean) => void;
  disabled?: boolean;
  label: string;
}) {
  return (
    <CheckboxPrimitive.Root
      checked={checked}
      onCheckedChange={(v) => onCheckedChange(v === true)}
      disabled={disabled}
      aria-label={label}
      className={cn(
        "flex size-4 shrink-0 items-center justify-center rounded-[3px] border transition-quick",
        "disabled:opacity-40 disabled:pointer-events-none",
        checked
          ? "border-[var(--color-accent)] bg-[var(--color-accent)] text-[var(--color-accent-ink)]"
          : "border-[var(--color-line-strong)] bg-[var(--color-surface)]",
      )}
    >
      <CheckboxPrimitive.Indicator>
        <Check className="size-3" strokeWidth={3} />
      </CheckboxPrimitive.Indicator>
    </CheckboxPrimitive.Root>
  );
}

// ------------------------------------------------------------------ Select

export function Select<T extends string>({
  value,
  onValueChange,
  options,
  label,
  className,
  disabled,
}: {
  value: T;
  onValueChange: (value: T) => void;
  options: { value: T; label: string; hint?: string }[];
  label: string;
  className?: string;
  disabled?: boolean;
}) {
  return (
    <SelectPrimitive.Root value={value} onValueChange={(v) => onValueChange(v as T)} disabled={disabled}>
      <SelectPrimitive.Trigger
        aria-label={label}
        className={cn(
          "inline-flex h-8 items-center justify-between gap-2 rounded-md border border-[var(--color-line-strong)]",
          "bg-[var(--color-surface-raised)] px-2.5 text-xs text-[var(--color-ink)] transition-quick",
          "hover:bg-[var(--color-surface-hover)] disabled:opacity-45",
          className,
        )}
      >
        <SelectPrimitive.Value />
        <ChevronDown className="size-3.5 text-[var(--color-ink-subtle)]" />
      </SelectPrimitive.Trigger>
      <SelectPrimitive.Portal>
        <SelectPrimitive.Content
          position="popper"
          sideOffset={4}
          className="z-50 overflow-hidden rounded-md border border-[var(--color-line-strong)] bg-[var(--color-surface-raised)] shadow-xl"
        >
          <SelectPrimitive.Viewport className="p-1">
            {options.map((option) => (
              <SelectPrimitive.Item
                key={option.value}
                value={option.value}
                className={cn(
                  "flex cursor-default select-none items-center justify-between gap-3 rounded px-2 py-1.5 text-xs",
                  "text-[var(--color-ink)] outline-none data-[highlighted]:bg-[var(--color-surface-hover)]",
                )}
              >
                <div>
                  <SelectPrimitive.ItemText>{option.label}</SelectPrimitive.ItemText>
                  {option.hint ? (
                    <span className="ml-2 text-[var(--color-ink-subtle)]">{option.hint}</span>
                  ) : null}
                </div>
                <SelectPrimitive.ItemIndicator>
                  <Check className="size-3 text-[var(--color-accent)]" />
                </SelectPrimitive.ItemIndicator>
              </SelectPrimitive.Item>
            ))}
          </SelectPrimitive.Viewport>
        </SelectPrimitive.Content>
      </SelectPrimitive.Portal>
    </SelectPrimitive.Root>
  );
}

// ------------------------------------------------------------------- Input

export const Input = React.forwardRef<HTMLInputElement, React.InputHTMLAttributes<HTMLInputElement>>(
  function Input({ className, ...rest }, ref) {
    return (
      <input
        ref={ref}
        className={cn(
          "h-8 rounded-md border border-[var(--color-line-strong)] bg-[var(--color-surface)] px-2.5",
          "text-xs text-[var(--color-ink)] placeholder:text-[var(--color-ink-subtle)]",
          "transition-quick focus:border-[var(--color-accent)] focus:outline-none",
          "disabled:opacity-45",
          className,
        )}
        {...rest}
      />
    );
  },
);

export function SearchInput({
  value,
  onChange,
  placeholder = "Search",
  className,
}: {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  className?: string;
}) {
  return (
    <div className={cn("relative", className)}>
      <Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-[var(--color-ink-subtle)]" />
      <Input
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        className="w-full pl-8"
        type="search"
      />
    </div>
  );
}

/**
 * A numeric field that commits on blur or Enter rather than per keystroke.
 *
 * Committing per keystroke is wrong for a persisted, clamped value: typing
 * `125` would save `1`, the backend would clamp it to the minimum, and the
 * clamped value would flow back and overwrite the digits still being typed.
 * The field therefore holds its own text while it is being edited, and only
 * hands a number upwards when the edit is finished.
 */
export function NumberField({
  value,
  onChange,
  min,
  max,
  step = 1,
  suffix,
  label,
  className,
}: {
  value: number;
  onChange: (value: number) => void;
  min: number;
  max: number;
  step?: number;
  suffix?: string;
  label: string;
  className?: string;
}) {
  const [draft, setDraft] = React.useState(String(value));
  const [editing, setEditing] = React.useState(false);

  // Follow the outside value whenever the field is not being edited, so a
  // change made elsewhere still shows up here.
  React.useEffect(() => {
    if (!editing) setDraft(String(value));
  }, [value, editing]);

  const commit = () => {
    setEditing(false);
    const parsed = Number(draft);
    if (!Number.isFinite(parsed)) {
      setDraft(String(value));
      return;
    }
    const clamped = Math.min(max, Math.max(min, Math.round(parsed / step) * step));
    setDraft(String(clamped));
    if (clamped !== value) onChange(clamped);
  };

  return (
    <div className={cn("flex items-center gap-1.5", className)}>
      <Input
        type="number"
        aria-label={label}
        value={draft}
        min={min}
        max={max}
        step={step}
        onFocus={() => setEditing(true)}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            (e.target as HTMLInputElement).blur();
          }
          if (e.key === "Escape") {
            setDraft(String(value));
            setEditing(false);
            (e.target as HTMLInputElement).blur();
          }
        }}
        className="w-20 text-right"
      />
      {suffix ? (
        <span className="text-xs text-[var(--color-ink-muted)]">{suffix}</span>
      ) : null}
    </div>
  );
}

// ------------------------------------------------------- Segmented control

export function SegmentedControl<T extends string>({
  value,
  onChange,
  options,
  label,
  className,
}: {
  value: T;
  onChange: (value: T) => void;
  options: { value: T; label: string }[];
  label: string;
  className?: string;
}) {
  return (
    <div
      role="radiogroup"
      aria-label={label}
      className={cn(
        "inline-flex items-center gap-0.5 rounded-md border border-[var(--color-line)] bg-[var(--color-surface)] p-0.5",
        className,
      )}
    >
      {options.map((option) => {
        const active = option.value === value;
        return (
          <button
            key={option.value}
            role="radio"
            aria-checked={active}
            onClick={() => onChange(option.value)}
            className={cn(
              "rounded px-2.5 py-1 text-xs transition-quick",
              active
                ? "bg-[var(--color-surface-hover)] font-medium text-[var(--color-ink)]"
                : "text-[var(--color-ink-muted)] hover:text-[var(--color-ink)]",
            )}
          >
            {option.label}
          </button>
        );
      })}
    </div>
  );
}
