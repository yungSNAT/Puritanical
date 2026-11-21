/* eslint react/react-in-jsx-scope: off -- Next.js uses the automatic JSX runtime. */
"use client";

import { useCallback } from "react";
import { useCopyText } from "./CopyCmd";
import { Glyph } from "./icons";

const INSTALL = "npx chisle";

// The install card: header row with a hint and copy button, then the command
// with its two most useful companions as comments. Copy takes the command only.
export default function InstallCard(): React.JSX.Element {
  const [copied, copyText] = useCopyText();
  const copyInstall = useCallback((): void => { copyText(INSTALL); }, [copyText]);
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
