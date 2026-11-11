/** Small line icons for navigation (24×24, stroke = currentColor). Drawn for OpenTrainer. */
const PATHS = {
  games: "M4 5h7v7H4zM13 5h7v7h-7zM4 14h7v5H4zM13 14h7v5h-7z",
  cheats: "M5 7h9M18 7h1M5 12h3M12 12h7M5 17h11M20 17h-1M16 5v4M10 10v4M18 15v4",
  packs: "M12 3 4 7v10l8 4 8-4V7zM4 7l8 4 8-4M12 11v10",
  saves: "M5 4h11l3 3v13H5zM8 4v5h7V4M8 20v-6h8v6",
  ai: "M12 3l1.8 4.7L18.5 9.5l-4.7 1.8L12 16l-1.8-4.7L5.5 9.5l4.7-1.8zM18 15l.9 2.1L21 18l-2.1.9L18 21l-.9-2.1L15 18l2.1-.9z",
  themes:
    "M12 3a9 9 0 0 0 0 18c1.2 0 1.8-.8 1.8-1.7 0-1.3-1-1.7-1-2.8 0-1 .8-1.7 1.8-1.7H17a4 4 0 0 0 4-4C21 6.6 17 3 12 3zM7.5 12.5h.01M9 8h.01M14 7h.01M17.5 10h.01",
  web: "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM3.5 9h17M3.5 15h17M12 3c2.6 2.4 3.9 5.4 3.9 9s-1.3 6.6-3.9 9c-2.6-2.4-3.9-5.4-3.9-9S9.4 5.4 12 3z",
  about: "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 11v6M12 7.5v.5",
  help: "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM9.5 9.3a2.6 2.6 0 0 1 5 .9c0 1.7-2.5 2.3-2.5 3.8M12 17v.5",
} as const;

export type IconName = keyof typeof PATHS;

export function Icon({ name, className = "h-5 w-5" }: { name: IconName; className?: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.8}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden="true"
    >
      <path d={PATHS[name]} />
    </svg>
  );
}

/** OpenTrainer's mark: three sliders on an accent tile (same drawing as the app icon, in the theme's colours). */
export function Logo({ className = "h-8 w-8" }: { className?: string }) {
  return (
    <svg viewBox="0 0 32 32" className={className} aria-hidden="true">
      <defs>
        <linearGradient id="ot-logo" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" style={{ stopColor: "var(--accent-from)" }} />
          <stop offset="1" style={{ stopColor: "var(--accent-to)" }} />
        </linearGradient>
      </defs>
      <rect width="32" height="32" rx="7" fill="url(#ot-logo)" />
      <g stroke="#eef0ff" strokeWidth="2" strokeLinecap="round" opacity="0.55">
        <path d="M9.5 7.5v17M16 7.5v17M22.5 7.5v17" />
      </g>
      <g fill="#fff" style={{ stroke: "var(--logo-line)" }} strokeWidth="0.6">
        <circle cx="9.5" cy="18.8" r="2.9" />
        <circle cx="16" cy="11.3" r="2.9" />
        <circle cx="22.5" cy="16.3" r="2.9" />
      </g>
    </svg>
  );
}
