/* eslint react/react-in-jsx-scope: off -- Next.js uses the automatic JSX runtime. */
"use client";

import { motion } from "motion/react";
import { useCopyText } from "./CopyCmd";
import { Glyph } from "./icons";

const TAGS = [
  { icon: "persona", label: "Terse persona" },
  { icon: "compress", label: "Output compressor" },
  { icon: "diet", label: "Context diet" },
  { icon: "deps", label: "Zero dependencies" },
  { icon: "offline", label: "No network calls" },
  { icon: "open", label: "MIT licensed" },
];

const INSTALL = "npx chisle";

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

function TileGrid(): React.JSX.Element {
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

// The install card: header row with a hint and copy button, then the command
// with its two most useful companions as comments. Copy takes the command only.
function InstallCard(): React.JSX.Element {
  const [copied, copyText] = useCopyText();
  const copyInstall = (): void => { copyText(INSTALL); };
  return (
    <div className="neon w-full max-w-[640px]">
      <div className="overflow-hidden rounded-[9px] bg-term text-left">
        <div className="flex items-center gap-2.5 border-b border-term-line bg-white/[0.02] px-4 py-3">
          <span aria-hidden className="grid size-5 place-items-center rounded bg-gradient-to-br from-brand to-[#ffc56b] font-mono text-[10px] font-bold text-on-brand">
            C
          </span>
          <span className="flex-1 font-mono text-sm font-medium text-term-fg">Run this. Done.</span>
          <button
            type="button"
            onClick={copyInstall}
            aria-label={copied ? "Copied" : `Copy ${INSTALL}`}
            className={`rounded p-1 transition-colors ${copied ? "text-[#86c06c]" : "text-term-dim hover:text-term-fg"}`}
          >
            <Glyph name={copied ? "check" : "copy"} size={16} />
          </button>
        </div>
        <pre className="pane-scroll overflow-x-auto px-6 py-5 font-mono text-sm leading-[1.8]">
          <code>
            <span className="text-term-accent">npx</span> <span className="text-term-fg">chisle</span>
            {"\n"}
            <span className="text-term-dim"># auto-detects all 13 agents · zero deps · zero network calls</span>
            {"\n"}
            <span className="text-term-dim"># preview: </span>
            <span className="text-term-fg/80">npx chisle</span> <span className="text-[#ffc56b]">--dry-run</span>
            <span className="text-term-dim">   undo: </span>
            <span className="text-term-fg/80">npx chisle</span> <span className="text-[#ffc56b]">--uninstall</span>
          </code>
        </pre>
      </div>
    </div>
  );
}

const ease = [0.16, 1, 0.3, 1] as const;
const intro = {
  start: { opacity: 0, x: -24 },
  end: { opacity: 1, x: 0 },
  transition: { duration: 0.6, ease },
};
const tags = {
  start: { opacity: 0, x: 24 },
  end: intro.end,
  transition: { duration: 0.6, delay: 0.1, ease },
};
const install = {
  start: { opacity: 0, y: 24 },
  end: { opacity: 1, y: 0 },
  transition: { duration: 0.6, delay: 0.2, ease },
};
const links = {
  start: { opacity: 0, scale: 0.94 },
  end: { opacity: 1, scale: 1 },
  transition: { duration: 0.5, delay: 0.3, ease },
};

export default function Hero(): React.JSX.Element {
  return (
    <header id="top" className="hero-wash relative overflow-hidden px-5 pt-40 pb-20 sm:pb-24">
      <TileGrid />
      <div aria-hidden className="aurora" />

      <div className="relative mx-auto flex max-w-6xl flex-col items-center gap-10 sm:gap-16">
        <div className="flex flex-col items-center gap-7 sm:gap-10">
          <motion.div
            initial={intro.start}
            animate={intro.end}
            transition={intro.transition}
            className="flex flex-col items-center gap-5 text-center"
          >
            <h1 className="text-4xl font-semibold leading-[1.08] tracking-tight sm:text-6xl lg:text-7xl">
              Shorter answers than caveman and ponytail.
              <br />
              <span className="text-gradient">No accuracy lost.</span>
            </h1>
            <p className="max-w-2xl text-base leading-relaxed text-fg/85 sm:text-lg">
              Built for Claude Code. One command, thirteen agents.
              <br className="hidden sm:block" />{" "}
              Answers about half as long, same pass rate as the bare model. Every number traces to a
              committed transcript.
            </p>
          </motion.div>

          <motion.ul
            initial={tags.start}
            animate={tags.end}
            transition={tags.transition}
            className="flex max-w-4xl flex-wrap justify-center gap-2"
          >
            {TAGS.map((t) => (
              <li
                key={t.label}
                className="flex items-center gap-1.5 whitespace-nowrap rounded-full border border-fg/10 bg-fg/[0.04] px-3.5 py-1.5 text-xs text-fg/70 transition-colors hover:border-brand/40 hover:bg-accent-soft hover:text-fg"
              >
                <span className="text-accent">
                  <Glyph name={t.icon} size={12} />
                </span>
                {t.label}
              </li>
            ))}
          </motion.ul>
        </div>

        <motion.div
          initial={install.start}
          animate={install.end}
          transition={install.transition}
          className="flex w-full justify-center"
        >
          <InstallCard />
        </motion.div>

        <motion.div
          initial={links.start}
          animate={links.end}
          transition={links.transition}
          className="flex w-full flex-col gap-3 sm:w-auto sm:flex-row sm:gap-4"
        >
          <a
            href="#get-started"
            className="rounded-md bg-brand px-7 py-3 text-center text-base font-medium text-on-brand transition-colors hover:bg-brand-hover"
          >
            Get started
          </a>
          <a
            href="https://github.com/JayPokale/Chisle"
            target="_blank"
            rel="noopener noreferrer"
            className="rounded-md border border-fg/25 px-7 py-3 text-center text-base font-medium text-fg transition-colors hover:border-fg/60 hover:bg-fg/5"
          >
            View on GitHub
          </a>
        </motion.div>
      </div>
    </header>
  );
}
