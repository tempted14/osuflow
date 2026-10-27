/** The osu!flow mark, the same as the app icon: a hit circle (a ring and a dot) in the accent colour on a flat tile. */
export function Logo({ size = 44 }: { size?: number }) {
  return (
    <svg className="logo" width={size} height={size} viewBox="0 0 64 64" role="img" aria-label="osu!flow">
      <rect width="64" height="64" rx="14.25" fill="#1d1d21" />
      <circle cx="32" cy="32" r="17.125" fill="none" stroke="#e8689b" strokeWidth="6.25" />
      <circle cx="32" cy="32" r="6.75" fill="#e8689b" />
    </svg>
  );
}
