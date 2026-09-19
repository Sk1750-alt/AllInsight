/**
 * The AllInsight mark, inline so it inherits the theme and never flashes while an
 * image loads. The SVG in assets/logo is the same artwork and remains the
 * source of truth for the application icon.
 */
export function Logo({ size = 24, className }: { size?: number; className?: string }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 256 256"
      className={className}
      role="img"
      aria-label="AllInsight"
    >
      <defs>
        <clipPath id="allinsight-logo-core">
          <path d="M128 20 214 70v116l-86 50-86-50V70z" />
        </clipPath>
      </defs>
      <g
        fill="none"
        stroke="var(--color-accent)"
        strokeWidth="7"
        strokeLinecap="round"
        opacity="0.5"
      >
        <path d="M196 42a112 112 0 0 1 26 63" />
        <path d="M60 214a112 112 0 0 1-26-63" />
      </g>
      <path
        d="M128 20 214 70v116l-86 50-86-50V70z"
        fill="var(--color-surface)"
        stroke="var(--color-accent)"
        strokeWidth="11"
        strokeLinejoin="round"
      />
      <g
        clipPath="url(#allinsight-logo-core)"
        fill="none"
        strokeWidth="15"
        strokeLinecap="round"
      >
        <path d="M169.3 143.1A44 44 0 1 1 150 89.9" stroke="var(--color-ink)" />
        <path d="M159.1 96.9A44 44 0 0 1 171.8 131.8" stroke="var(--color-accent)" />
      </g>
      <circle
        cx="128"
        cy="128"
        r="14"
        fill="var(--color-ink)"
        clipPath="url(#allinsight-logo-core)"
      />
    </svg>
  );
}

export function Wordmark({ className }: { className?: string }) {
  return (
    <div className={className}>
      {/* Camel case at ten characters: the wide tracking the old six-capital
          name carried pulls "AllInsight" apart into separate letters. */}
      <div className="font-display text-base font-semibold tracking-[0.01em] text-[var(--color-ink)]">
        AllInsight
      </div>
      <div className="text-2xs text-[var(--color-ink-subtle)]">Local device intelligence</div>
    </div>
  );
}
