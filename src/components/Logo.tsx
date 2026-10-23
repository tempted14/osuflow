/** The osu!flow mark: a hit circle with a short trail, in the brand gradient. */
export function Logo({ size = 44 }: { size?: number }) {
  return (
    <svg className="logo" width={size} height={size} viewBox="0 0 64 64" role="img" aria-label="osu!flow">
      <defs>
        <linearGradient id="flow-gradient" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#ff66aa" />
          <stop offset="1" stopColor="#a77bff" />
        </linearGradient>
      </defs>
      <rect width="64" height="64" rx="15" fill="rgba(255,255,255,0.07)" />
      <circle cx="24" cy="42" r="2.6" fill="#c98bd9" opacity="0.35" />
      <circle cx="29" cy="38" r="3.4" fill="#c98bd9" opacity="0.5" />
      <circle cx="37" cy="29" r="14" fill="none" stroke="url(#flow-gradient)" strokeWidth="5" />
      <circle cx="37" cy="29" r="6.2" fill="url(#flow-gradient)" />
    </svg>
  );
}
