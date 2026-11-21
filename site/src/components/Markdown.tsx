/* eslint react/react-in-jsx-scope: off -- Next.js uses the automatic JSX runtime. */
"use client";

import { createContext, useContext, useId, useRef } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import { useCopyText } from "./CopyCmd";

// Renders the benchmark transcripts, which are markdown: fenced code, tables,
// nested lists, bold, inline code.
//
// Raw HTML is deliberately NOT enabled (no rehype-raw). These strings are model
// output, and while they are ours and committed, rendering arbitrary generated
// HTML into the page buys nothing and costs a class of injection bug. The few
// stray tags in the corpus render as visible text, which is the honest result
// anyway: it is what the model actually emitted.
//
// No syntax highlighter: that is another dependency an order of magnitude
// larger, to colour six committed samples. Monospace on a panel is enough.

const CODE_COPY_NOTICE_MS = 1400;
const FootnoteLabel = createContext("footnote-label");

function CodeBlock({ children }: Readonly<{ children: React.ReactNode }>): React.JSX.Element {
  const [copied, copyText] = useCopyText(CODE_COPY_NOTICE_MS);
  const code = useRef<HTMLPreElement>(null);
  const copyCode = (): void => {
    if (code.current) copyText(code.current.innerText);
  };
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

type FootnoteLinkProps = Readonly<React.ComponentPropsWithoutRef<"a"> & {
  "data-footnote-ref"?: boolean;
  "data-footnote-backref"?: boolean;
}>;

function MarkdownHeading({ id, className, children }: Readonly<React.ComponentPropsWithoutRef<"h2">>): React.JSX.Element {
  const footnoteLabel = useContext(FootnoteLabel);
  return (
    <h5 id={id === "footnote-label" ? footnoteLabel : id} className={className ?? "mt-4 mb-1.5 text-[12.5px] font-bold first:mt-0"}>
      {children}
    </h5>
  );
}

function MarkdownLink({
  href, title, id, className, children,
  "aria-describedby": describedBy,
  "aria-label": label,
  "data-footnote-ref": footnoteRef,
  "data-footnote-backref": footnoteBackref,
}: FootnoteLinkProps): React.JSX.Element {
  const footnoteLabel = useContext(FootnoteLabel);
  return (
    <a
      className={className ?? "text-accent underline-offset-2 hover:underline"}
      target={href?.startsWith("#") ? undefined : "_blank"}
      rel="noreferrer"
      href={href}
      title={title}
      id={id}
      aria-describedby={describedBy === "footnote-label" ? footnoteLabel : describedBy}
      aria-label={label}
      data-footnote-ref={footnoteRef}
      data-footnote-backref={footnoteBackref}
    >
      {children}
    </a>
  );
}

const markdownComponents: Components = {
  h1: ({ children }) => <h4 className="mt-4 mb-1.5 text-[13px] font-bold first:mt-0">{children}</h4>,
  h2: MarkdownHeading,
  h3: ({ children }) => <h6 className="mt-3 mb-1 text-[12px] font-semibold first:mt-0">{children}</h6>,
  h4: ({ children }) => <h6 className="mt-3 mb-1 text-[12px] font-semibold first:mt-0">{children}</h6>,
  p: ({ children }) => <p className="my-2 first:mt-0 last:mb-0">{children}</p>,
  ul: ({ className, children }) => (
    <ul className={className ?? "my-2 list-disc space-y-0.5 pl-5 marker:text-accent"}>{children}</ul>
  ),
  ol: ({ className, start, children }) => (
    <ol className={className ?? "my-2 list-decimal space-y-0.5 pl-5 marker:text-accent"} start={start}>{children}</ol>
  ),
  li: ({ id, className, children }) => <li id={id} className={className ?? "pl-0.5"}>{children}</li>,
  strong: ({ children }) => <strong className="font-semibold text-fg">{children}</strong>,
  em: ({ children }) => <em className="italic">{children}</em>,
  hr: () => <hr className="my-3 border-line" />,
  a: MarkdownLink,
  blockquote: ({ children }) => <blockquote className="my-2 border-l-2 border-line pl-3 text-dim">{children}</blockquote>,
  code: ({ className, children }) => (
    <code className={className?.startsWith("language-")
      ? "block font-mono text-[11px] leading-relaxed"
      : "rounded border border-line bg-accent-soft/60 px-1 py-px font-mono text-[10.5px] text-accent"}
    >
      {children}
    </code>
  ),
  pre: ({ children }) => <CodeBlock>{children}</CodeBlock>,
  table: ({ children }) => (
    <div className="pane-scroll my-2 overflow-x-auto">
      <table className="w-full border-collapse text-[11px]">{children}</table>
    </div>
  ),
  thead: ({ children }) => <thead className="text-dim">{children}</thead>,
  th: ({ style, children }) => (
    <th className="border border-line px-2 py-1 text-left font-semibold" style={style}>{children}</th>
  ),
  td: ({ style, children }) => <td className="border border-line px-2 py-1 align-top" style={style}>{children}</td>,
};

const remarkPlugins = [remarkGfm];

export default function Markdown({ children }: Readonly<{ children: string }>): React.JSX.Element {
  const scope = useId();
  return (
    <FootnoteLabel.Provider value={`${scope}-footnote-label`}>
      <div>
        <ReactMarkdown
          key={children}
          remarkPlugins={remarkPlugins}
          remarkRehypeOptions={{ clobberPrefix: `${scope}-` }}
          components={markdownComponents}
        >
          {children}
        </ReactMarkdown>
      </div>
    </FootnoteLabel.Provider>
  );
}
