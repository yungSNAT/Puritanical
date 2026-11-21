/* eslint react/react-in-jsx-scope: off -- Next.js uses the automatic JSX runtime. */
"use client";

import { useEffect, useRef, useState } from "react";

const COPY_NOTICE_MS = 1600;

export function useCopyText(noticeMs = COPY_NOTICE_MS): readonly [boolean, (text: string) => void] {
  const [copied, setCopied] = useState(false);
  const reset = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const attempt = useRef(0);

  useEffect(() => () => {
    attempt.current++;
    clearTimeout(reset.current);
  }, []);

  function copyText(text: string): void {
    clearTimeout(reset.current);
    const request = ++attempt.current;
    void (async () => {
      try {
        await navigator.clipboard.writeText(text);
        if (request !== attempt.current) return;
        setCopied(true);
        reset.current = setTimeout(() => { setCopied(false); }, noticeMs);
      } catch {
        if (request === attempt.current) setCopied(false);
      }
    })();
  }
  return [copied, copyText];
}

// One-click copy for a shell command. Multi-line commands keep their breaks.
export default function CopyCmd({ cmd, label }: Readonly<{ cmd: string; label: string }>): React.JSX.Element {
  const [copied, copyText] = useCopyText();
  const copyCommand = (): void => { copyText(cmd); };
  return (
    <button
      type="button"
      onClick={copyCommand}
      className="group flex w-full items-start gap-3 rounded-lg border border-line bg-surface/90 px-4 py-3 text-left font-mono text-sm backdrop-blur transition-colors hover:border-brand-hover"
      aria-label={`Copy ${label} command`}
    >
      <span className="shrink-0 select-none text-accent">$</span>
      <span className="min-w-0 flex-1 whitespace-pre-wrap break-all">{cmd}</span>
      <span className="shrink-0 rounded border border-line px-1.5 py-0.5 text-[11px] text-dim transition-colors group-hover:text-fg">
        {copied ? "copied" : "copy"}
      </span>
    </button>
  );
}
