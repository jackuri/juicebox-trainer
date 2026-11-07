/** Small line icons for navigation (24×24, stroke = currentColor). Drawn for OpenTrainer. */
const PATHS = {
  games: "M4 5h7v7H4zM13 5h7v7h-7zM4 14h7v5H4zM13 14h7v5h-7z",
  cheats: "M5 7h9M18 7h1M5 12h3M12 12h7M5 17h11M20 17h-1M16 5v4M10 10v4M18 15v4",
  packs: "M12 3 4 7v10l8 4 8-4V7zM4 7l8 4 8-4M12 11v10",
  saves: "M5 4h11l3 3v13H5zM8 4v5h7V4M8 20v-6h8v6",
  ai: "M12 3l1.8 4.7L18.5 9.5l-4.7 1.8L12 16l-1.8-4.7L5.5 9.5l4.7-1.8zM18 15l.9 2.1L21 18l-2.1.9L18 21l-.9-2.1L15 18l2.1-.9z",
  about: "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 11v6M12 7.5v.5",
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

/** OpenTrainer's mark: three sliders on an accent tile (same drawing as the app icon). */
export function Logo({ className = "h-8 w-8" }: { className?: string }) {
  return (
    <svg viewBox="0 0 32 32" className={className} aria-hidden="true">
      <defs>
        <linearGradient id="ot-logo" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#6a63ff" />
          <stop offset="1" stopColor="#4f8bff" />
        </linearGradient>
      </defs>
      <rect width="32" height="32" rx="7" fill="url(#ot-logo)" />
      <g stroke="#eef0ff" strokeWidth="2" strokeLinecap="round" opacity="0.55">
        <path d="M9.5 7.5v17M16 7.5v17M22.5 7.5v17" />
      </g>
      <g fill="#fff" stroke="#3f38ae" strokeWidth="0.6">
        <circle cx="9.5" cy="18.8" r="2.9" />
        <circle cx="16" cy="11.3" r="2.9" />
        <circle cx="22.5" cy="16.3" r="2.9" />
      </g>
    </svg>
  );
}
