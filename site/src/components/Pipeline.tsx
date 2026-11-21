/* eslint react/react-in-jsx-scope: off -- Next.js uses the automatic JSX runtime. */
"use client";

import { useCallback, useState, type SyntheticEvent } from "react";
import PipelineDiagram from "./PipelineDiagram";
import Reveal from "./Reveal";
import SectionHead from "./SectionHead";

// Where each axis attaches to a session: the SVG twin of the README mermaid
// diagram. Inline SVG rather than a diagram library: see the note in Ladder.tsx.
//
// The point of the picture is the loop. Tool output is re-billed on every later
// request in the session, so compressing it once pays repeatedly, and Read/Edit
// sit deliberately outside it, because their exact bytes feed later edits.

const HOOKS = [
  {
    id: "session",
    label: "SessionStart",
    body: "Injects the ruleset once, on a genuinely new session. Resume, clear and compact get a 27-token reactivation line instead; re-sending the full ruleset every time was most of the plugin's own overhead.",
  },
  {
    id: "prompt",
    label: "UserPromptSubmit",
    body: "A ~50-token reminder each turn, so the mode survives context compression. Cheap enough not to be worth optimising, and it is what lets SessionStart stay quiet on resume.",
  },
  {
    id: "post",
    label: "PostToolUse",
    body: "Filter, scrub, elide, dedup. Command-aware filters drop git hints, passing tests and install progress. Deterministic, no LLM, no network. Rebuilds the result into the tool's own response shape, because a bare string gets rejected by the harness. Read and Edit are never touched.",
  },
] as const;

// eslint-disable-next-line @typescript-eslint/no-type-alias -- A union derived from HOOKS; an interface cannot express it.
type HookId = (typeof HOOKS)[number]["id"];

const NOTE = (
  <Reveal delay={0.12}>
    <p className="mx-auto mt-6 max-w-2xl text-center text-xs text-dim">
      Tool output is billed again on <em>every</em> later request in the session, so shrinking
      it once pays repeatedly. The compressor rebuilds its result into the tool&apos;s own
      response shape; returning a bare string gets the replacement rejected, which is exactly
      the bug that made this axis a no-op before v2.0.0.
    </p>
  </Reveal>
);

export default function Pipeline(): React.JSX.Element {
  const [hook, setHook] = useState<HookId | null>(null);

  const showHook = useCallback((event: SyntheticEvent<SVGGElement | HTMLButtonElement>): void => {
    const id = event.currentTarget.dataset.hook;
    setHook(HOOKS.find((item) => item.id === id)?.id ?? null);
  }, []);

  const clearHook = useCallback((): void => {
    setHook(null);
  }, []);

  return (
    <section className="border-y border-line bg-surface py-28">
      <div className="mx-auto max-w-6xl px-5">
        <SectionHead eyebrow="How it works" title="Three hooks, one session" center>
          Two hooks shape what the model writes. The third shrinks what it reads, and that one
          is a loop.
        </SectionHead>

        <Reveal delay={0.08}>
          <div className="mt-10 overflow-x-auto">
            <PipelineDiagram hook={hook} onShow={showHook} onClear={clearHook} />
          </div>
        </Reveal>

        <div className="mx-auto mt-6 flex max-w-3xl flex-wrap justify-center gap-2">
          {HOOKS.map((h) => (
            <button
              type="button"
              key={h.id}
              data-hook={h.id}
              onMouseEnter={showHook}
              onMouseLeave={clearHook}
              onFocus={showHook}
              onBlur={clearHook}
              aria-pressed={hook === h.id}
              className={`rounded border px-3 py-1 font-mono text-[11px] transition-all ${
                hook === h.id
                  ? "-translate-y-0.5 border-brand-hover bg-accent-soft text-accent shadow-sm"
                  : "border-line text-dim hover:border-brand-hover/50 hover:text-fg"
              }`}
            >
              {h.label}
            </button>
          ))}
        </div>

        <p className="mx-auto mt-3 min-h-[3.5rem] max-w-2xl text-center text-xs leading-relaxed text-dim">
          {HOOKS.find((h) => h.id === hook)?.body ?? "Hover a hook, in the diagram or below it."}
        </p>

        {NOTE}
      </div>
    </section>
  );
}
