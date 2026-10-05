/**
 * The AllInsight mark: an A with a lowercase i standing inside it, read as
 * "A·i". Inline so it inherits the theme and never flashes while an image
 * loads. The A and the i take the ink colour, so the mark works on the light
 * and dark themes; the dot is always Insight Amber.
 *
 * Below 32 px the brand kit drops the i-stem and draws a heavier stroke and a
 * larger dot, so the mark still reads at sidebar size. The SVGs in assets/logo
 * are the same artwork and remain the source of truth for the application icon.
 */
const AMBER = "#F5A524";

export function Logo({ size = 24, className }: { size?: number; className?: string }) {
  const small = size < 32;
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 100 100"
      className={className}
      role="img"
      aria-label="AllInsight"
    >
      {small ? (
        <>
          <path
            d="M14 82 L50 20 L86 82"
            fill="none"
            stroke="var(--color-ink)"
            strokeWidth="16"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
          <circle cx="50" cy="66" r="9" fill={AMBER} />
        </>
      ) : (
        <>
          <path
            d="M12 84 L50 18 L88 84"
            fill="none"
            stroke="var(--color-ink)"
            strokeWidth="12"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
          <line
            x1="50"
            y1="72"
            x2="50"
            y2="84"
            stroke="var(--color-ink)"
            strokeWidth="12"
            strokeLinecap="round"
          />
          <circle cx="50" cy="54" r="7" fill={AMBER} />
        </>
      )}
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
