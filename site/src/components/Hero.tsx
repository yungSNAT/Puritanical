/* eslint react/react-in-jsx-scope: off -- Next.js uses the automatic JSX runtime. */
"use client";

import { motion } from "motion/react";
import { Glyph } from "./icons";
import InstallCard from "./InstallCard";
import TileGrid from "./TileGrid";

const TAGS = [
  { icon: "persona", label: "Terse persona" },
  { icon: "compress", label: "Output compressor" },
  { icon: "diet", label: "Context diet" },
  { icon: "deps", label: "Zero dependencies" },
  { icon: "offline", label: "No network calls" },
  { icon: "open", label: "MIT licensed" },
];

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
