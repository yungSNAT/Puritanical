import Markdown from "./Markdown";
import type { Cell } from "./compare-data";

interface Props {
  readonly children: React.ReactNode;
  readonly cell: Cell;
  readonly frameClass: string;
  readonly headerClass: string;
  readonly bodyClass: string;
}

// One transcript: its header (the children), the verbatim text and, when the page shortened
// it, a note that the token counts still cover the full response.
export default function TranscriptPane({ children, cell, frameClass, headerClass, bodyClass }: Readonly<Props>): React.JSX.Element {
  return (
    <div className={`flex min-w-0 flex-col rounded-xl bg-surface ${frameClass}`}>
      <header className={`flex items-baseline justify-between border-b border-line px-4 py-3 ${headerClass}`}>
        {children}
      </header>
      <div className={`pane-scroll max-h-96 flex-1 overflow-auto px-4 py-3 text-[12px] leading-relaxed ${bodyClass}`}>
        <Markdown>{cell.text}</Markdown>
        {cell.truncated ? (
          <p className="mt-3 border-t border-line pt-2 font-mono text-[10.5px] text-dim">
            … truncated for the page; token counts are the full response
          </p>
        ) : null}
      </div>
    </div>
  );
}
