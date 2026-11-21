/* eslint react/react-in-jsx-scope: off -- Next.js uses the automatic JSX runtime. */

// Tile grid behind the copy: faint lines in three soft patches, plus a few lit
// cells. Drawn in currentColor so it follows the theme.
const CELL = 60;
const CELL_INSET = 2;
const TILE_SIZE = CELL - CELL_INSET * 2;
const LIT = [
  { column: 0, row: 0, opacity: 0.03 },
  { column: 2, row: 1, opacity: 0.045 },
  { column: 13, row: 0, opacity: 0.04 },
  { column: 15, row: 0, opacity: 0.03 },
  { column: 12, row: 1, opacity: 0.045 },
  { column: 14, row: 1, opacity: 0.035 },
  { column: 4, row: 6, opacity: 0.03 },
  { column: 6, row: 6, opacity: 0.05 },
  { column: 3, row: 8, opacity: 0.025 },
  { column: 4, row: 9, opacity: 0.04 },
  { column: 6, row: 9, opacity: 0.05 },
  { column: 5, row: 10, opacity: 0.025 },
  { column: 22, row: 6, opacity: 0.03 },
  { column: 24, row: 7, opacity: 0.05 },
  { column: 23, row: 8, opacity: 0.04 },
  { column: 22, row: 9, opacity: 0.045 },
  { column: 27, row: 6, opacity: 0.025 },
  { column: 28, row: 7, opacity: 0.04 },
];

export default function TileGrid(): React.JSX.Element {
  return (
    <svg
      aria-hidden
      viewBox="0 0 1720 820"
      preserveAspectRatio="xMidYMin slice"
      className="pointer-events-none absolute left-1/2 top-0 size-full max-w-[1720px] -translate-x-1/2 text-fg"
    >
      <defs>
        <pattern id="tg-lines" width={CELL} height={CELL} patternUnits="userSpaceOnUse">
          <path d={`M${CELL} 0H0V${CELL}`} fill="none" stroke="currentColor" />
        </pattern>
        <radialGradient id="tg-fade">
          <stop offset="0" stopColor="#fff" />
          <stop offset="1" stopColor="#000" />
        </radialGradient>
        <mask id="tg-mask">
          <ellipse cx="360" cy="460" rx="380" ry="260" fill="url(#tg-fade)" />
          <ellipse cx="1420" cy="440" rx="380" ry="260" fill="url(#tg-fade)" />
          <ellipse cx="860" cy="40" rx="280" ry="150" fill="url(#tg-fade)" />
        </mask>
        <filter id="tg-soft">
          <feGaussianBlur stdDeviation="4" />
        </filter>
      </defs>
      <rect width="1720" height="820" fill="url(#tg-lines)" mask="url(#tg-mask)" opacity="0.08" />
      <g fill="currentColor" filter="url(#tg-soft)">
        {LIT.map(({ column, row, opacity }) => (
          <rect
            key={`${column}-${row}`}
            x={column * CELL + CELL_INSET}
            y={row * CELL + CELL_INSET}
            width={TILE_SIZE}
            height={TILE_SIZE}
            opacity={opacity}
          />
        ))}
      </g>
    </svg>
  );
}
