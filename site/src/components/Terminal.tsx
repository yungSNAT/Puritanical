/* eslint react/react-in-jsx-scope: off -- Next.js uses the automatic JSX runtime. */
"use client";

import { useEffect, useRef, useState } from "react";
import { useInView } from "motion/react";
import Reveal from "./Reveal";
import SectionHead from "./SectionHead";

// The one dark element on the page: the product doing its job. Every number
// is the real hook run on a 411-line build log: 37,745 chars in, 8,756 out,
// 312 lines elided, both mid-log warnings salvaged. A successful command: a
// failed one is only shrunk when chisle-run.js wrapped it (bypass mode).
const HEAD = [
  "$ npm run build",
  "• Packages in scope: @shop/api, @shop/ui, @shop/web",
  "@shop/api:build: transforming...",
];
const NOISE_COUNT = 312;
const LINES_PER_FRAME = 41;
const FRAME_MS = 100;
const SUMMARY_DELAY_MS = 700;
const LINES_PER_PREVIEW = 60;
const PREVIEW_ROWS = [
  "@shop/api:build: dist/assets/chunk-1000.js  4.20 kB",
  "@shop/ui:build: dist/assets/chunk-2eef.js  7.90 kB",
  "@shop/web:build: dist/assets/chunk-4dde.js  11.60 kB",
  "@shop/api:build: dist/assets/chunk-6ccd.js  15.30 kB",
  "@shop/ui:build: dist/assets/chunk-8bbc.js  19.00 kB",
  "@shop/web:build: dist/assets/chunk-aaab.js  22.70 kB",
  "@shop/api:build: dist/assets/chunk-c99a.js  26.40 kB",
];
const SALVAGED = [
  "@shop/api:build: warning: src/billing/invoice.ts:41 'roundCents' is declared but never used",
  "@shop/ui:build: WARNING in asset size limit: dist/ui.js (612 KiB) exceeds the recommended limit",
];
const TAIL = [" Tasks:    3 successful, 3 total"];

export default function Terminal(): React.JSX.Element {
  const ref = useRef<HTMLDivElement>(null);
  const inView: boolean = useInView(ref, { once: true, margin: "-120px" });
  const [summaryVisible, setSummaryVisible] = useState(false);
  const [flood, setFlood] = useState<number>(0);
  const phase = !inView ? "idle" : summaryVisible ? "done" : "flood";

  useEffect(() => {
    if (!inView) return undefined;
    let n = 0;
    let summaryTimer: ReturnType<typeof setTimeout> | undefined = undefined;
    const tick = setInterval(() => {
      n += LINES_PER_FRAME;
      if (n >= NOISE_COUNT) {
        clearInterval(tick);
        setFlood(NOISE_COUNT);
        summaryTimer = setTimeout(() => {
          setSummaryVisible(true);
        }, SUMMARY_DELAY_MS);
      } else {
        setFlood(n);
      }
    }, FRAME_MS);
    return (): void => {
      clearInterval(tick);
      clearTimeout(summaryTimer);
    };
  }, [inView]);

  return (
    <section className="mx-auto max-w-6xl px-5 py-28">
      <SectionHead eyebrow="The compressor, live" title="400 lines of build output. Two matter.">
        Tool results re-bill on every later turn. The compressor keeps the head, the tail, and
        any error or warning lines from the middle. The rest never reaches the model.
      </SectionHead>

      <Reveal delay={0.1}>
        <div
          ref={ref}
          className="mt-10 overflow-hidden rounded-xl border border-term-line bg-term font-mono"
        >
          <div className="flex items-center border-b border-term-line px-4 py-2.5">
            <span className="text-xs text-term-dim">PostToolUse · Bash</span>
            <span className="ml-auto rounded bg-term-accent/15 px-1.5 py-0.5 text-xs text-term-accent">
              [CHISLE]{phase === "done" ? " ⇣7k tok" : ""}
            </span>
          </div>
          <div className="h-64 overflow-hidden p-4 text-xs leading-relaxed">
            {HEAD.map((l) => (
              <div key={l} className="text-term-dim">{l}</div>
            ))}
            {phase === "flood" && (
              <div className="text-term-dim/50">
                {PREVIEW_ROWS.slice(0, Math.ceil(flood / LINES_PER_PREVIEW)).map((row) => (
                  <div key={row}>{row}</div>
                ))}
                <div>… {flood} lines and counting …</div>
              </div>
            )}
            {phase === "done" && (
              <>
                <div className="my-2 w-fit rounded border border-dashed border-term-accent/50 px-3 py-1.5 text-term-accent">
                  ⋯ 312 lines elided — kept first 60, last 40, and 2 error-like lines ⋯
                </div>
                {SALVAGED.map((l) => (
                  <div key={l} className="text-[#ffc56b]">{l}</div>
                ))}
                {TAIL.map((l) => (
                  <div key={l} className="text-term-fg">{l}</div>
                ))}
                <div className="mt-2 text-[#86c06c]">✓ 37,745 chars → 8,756 · billed once, saved every turn after</div>
              </>
            )}
            {phase !== "done" && <span className="caret" />}
          </div>
        </div>
      </Reveal>

      <Reveal delay={0.15}>
        <p className="mt-4 text-xs text-dim">
          Deterministic: no LLM calls, no network, no dependencies. Allowlist: Bash, Agent,
          WebFetch, WebSearch, Grep, Glob, mcp__*. Never Read/Edit/Write. Claude Code sends a
          failed command to PostToolUseFailure, which no hook can rewrite, so since 3.14 test,
          build and install commands run through a small wrapper that filters them before they
          exit (in bypass-permissions mode).
        </p>
      </Reveal>
    </section>
  );
}
