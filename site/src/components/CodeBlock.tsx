/* eslint react/react-in-jsx-scope: off -- Next.js uses the automatic JSX runtime. */
"use client";

import { useCallback, useRef } from "react";
import { useCopyText } from "./CopyCmd";

const CODE_COPY_NOTICE_MS = 1400;

// A fenced block in a rendered transcript, with a copy button on hover.
export default function CodeBlock({ children }: Readonly<{ children: React.ReactNode }>): React.JSX.Element {
  const [copied, copyText] = useCopyText(CODE_COPY_NOTICE_MS);
  const code = useRef<HTMLPreElement>(null);
  const copyCode = useCallback((): void => {
    if (code.current) copyText(code.current.innerText);
  }, [copyText]);
  return (
    <div className="group relative">
      <pre ref={code} className="pane-scroll my-2 overflow-x-auto rounded border border-line bg-bg/70 p-2.5">
        {children}
      </pre>
      <button
        type="button"
        onClick={copyCode}
        className="absolute right-1.5 top-3.5 rounded border border-line bg-surface px-1.5 py-0.5 font-mono text-[9.5px] text-dim opacity-0 transition-opacity hover:text-accent focus:opacity-100 group-hover:opacity-100"
        aria-label="Copy code"
      >
        {copied ? "copied" : "copy"}
      </button>
    </div>
  );
}
