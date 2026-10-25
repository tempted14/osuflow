/** The osu!flow mark, the same as the app icon: a hit circle (a ring and a dot) in the brand gradient on a flat tile. */
export function Logo({ size = 44 }: { size?: number }) {
  return (
    <svg className="logo" width={size} height={size} viewBox="0 0 64 64" role="img" aria-label="osu!flow">
      <defs>
        <linearGradient id="flow-gradient" x1="0.12" y1="0.08" x2="0.88" y2="0.92">
          <stop offset="0" stopColor="#ff66aa" />
          <stop offset="1" stopColor="#a77bff" />
        </linearGradient>
      </defs>
      <rect width="64" height="64" rx="14.25" fill="#1a1630" />
      <circle cx="32" cy="32" r="17.125" fill="none" stroke="url(#flow-gradient)" strokeWidth="6.25" />
      <circle cx="32" cy="32" r="6.75" fill="url(#flow-gradient)" />
    </svg>
  );
}
