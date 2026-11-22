/* eslint react/react-in-jsx-scope: off -- Next.js uses the automatic JSX runtime. */
import PipelineHook, { type HookNode } from "./PipelineHook";

// The session diagram behind Pipeline. viewBox is cropped to the drawn content
// (x 8→708, y 34→280). The earlier 860-wide box left ~150px of dead space on
// the right, which read as the whole diagram being off-centre.
//
// Static parts are built once at module level; SVG paints in document order,
// so they stay interleaved with the hover-dependent parts exactly as drawn.

const ACCENT = "var(--color-accent)";
const LINE = "var(--color-line)";

const SESSION: HookNode = {
  id: "session", x: 8, y: 34, width: 188, height: 52, scale: "scale(1.04)",
  label: "SessionStart", labelY: 55, labelSize: 11, labelWeight: 600,
  note: "~900 tok · new sessions only", noteY: 72,
  stroke: LINE, activeStroke: ACCENT, strokeWidth: 1, activeStrokeWidth: 1.8,
};
const PROMPT: HookNode = {
  id: "prompt", x: 8, y: 106, width: 188, height: 52, scale: "scale(1.04)",
  label: "UserPromptSubmit", labelY: 127, labelSize: 11, labelWeight: 600,
  note: "~50 tok · every turn", noteY: 144,
  stroke: LINE, activeStroke: ACCENT, strokeWidth: 1, activeStrokeWidth: 1.8,
};
const POST: HookNode = {
  id: "post", x: 508, y: 222, width: 200, height: 58, scale: "scale(1.03)",
  label: "PostToolUse", labelY: 244, labelSize: 12, labelWeight: 700,
  note: "filter → scrub → elide → dedup", noteY: 262,
  stroke: ACCENT, activeStroke: ACCENT, strokeWidth: 1.5, activeStrokeWidth: 2.4,
};

const DEFS = (
  <defs>
    <marker id="ar" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
      <path d="M 0 0 L 10 5 L 0 10 z" fill="var(--color-dim)" />
    </marker>
    <marker id="ar-amber" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
      <path d="M 0 0 L 10 5 L 0 10 z" fill="var(--color-accent)" />
    </marker>
    <style>{`
      svg [data-hook] rect { transition: stroke 0.2s, stroke-width 0.2s; }
      .ring { transition: opacity 0.25s, transform 0.25s; transform-box: fill-box; transform-origin: center; }
    `}</style>
  </defs>
);

const MODEL = (
  <>
    <rect x="286" y="58" width="132" height="76" rx="38" fill="var(--color-raised)" stroke="var(--color-accent)" strokeWidth="1.5" />
    <text x="352" y="102" textAnchor="middle" fontSize="15" fontWeight="700" fill="currentColor">model</text>
  </>
);

const OUTPUT_AND_TOOLS = (
  <>
    {/* output */}
    <rect x="508" y="46" width="200" height="60" rx="6" fill="var(--color-raised)" stroke="var(--color-accent)" />
    <text x="608" y="70" textAnchor="middle" fontSize="12" fontWeight="600" fill="currentColor">terser prose</text>
    <text x="608" y="88" textAnchor="middle" fontSize="12" fontWeight="600" fill="currentColor">YAGNI-first code</text>
    <path d="M 418 82 L 504 76" stroke="var(--color-accent)" fill="none" markerEnd="url(#ar-amber)" />
    <text x="461" y="66" textAnchor="middle" fontSize="10" fill="var(--color-dim)">writes</text>

    {/* tool call */}
    <rect x="508" y="150" width="200" height="46" rx="6" fill="var(--color-raised)" stroke="var(--color-line)" />
    <text x="608" y="178" textAnchor="middle" fontSize="11.5" fill="currentColor">Bash · Grep · WebFetch · mcp__*</text>
    <path d="M 400 134 L 504 166" stroke="var(--color-dim)" fill="none" markerEnd="url(#ar)" />
    <text x="446" y="163" textAnchor="middle" fontSize="10" fill="var(--color-dim)">calls</text>
  </>
);

const TOOL_TO_HOOK = <path d="M 608 196 L 608 218" stroke="var(--color-dim)" fill="none" markerEnd="url(#ar)" />;

const LOOP_NOTE_AND_EXCLUDED = (
  <>
    <text x="430" y="243" textAnchor="middle" fontSize="10" fill="var(--color-accent)">compressed, in the tool&apos;s own shape</text>

    {/* excluded */}
    <rect x="8" y="222" width="188" height="58" rx="6" fill="none" stroke="var(--color-line)" strokeDasharray="4 3" />
    <text x="102" y="244" textAnchor="middle" fontSize="12" fontWeight="600" fill="currentColor">Read · Edit</text>
    <text x="102" y="262" textAnchor="middle" fontSize="10.5" fill="var(--color-dim)">never touched</text>
    <path d="M 196 244 L 330 138" stroke="var(--color-line)" fill="none" strokeDasharray="4 3" markerEnd="url(#ar)" />
  </>
);

const LOOP_PULSE = <animate attributeName="stroke-dashoffset" from="20" to="0" dur="0.8s" repeatCount="indefinite" />;

const arrowStroke = (active: boolean): string => (active ? "var(--color-accent)" : "var(--color-dim)");
const arrowEnd = (active: boolean): string => (active ? "url(#ar-amber)" : "url(#ar)");

interface PipelineDiagramProps {
  hook: string | null;
  onShow: React.MouseEventHandler<SVGGElement>;
  onClear: () => void;
}

export default function PipelineDiagram({ hook, onShow, onClear }: Readonly<PipelineDiagramProps>): React.JSX.Element {
  const session = hook === SESSION.id;
  const prompt = hook === PROMPT.id;
  const post = hook === POST.id;
  return (
    // eslint-disable-next-line jsx-a11y/prefer-tag-over-role -- An inline SVG with hover handlers cannot be an <img>; role="img" with aria-label is the accessible pattern.
    <svg
      viewBox="0 20 716 274"
      className="mx-auto block w-full min-w-[620px] max-w-[760px]"
      role="img"
      aria-label="Session diagram. SessionStart injects about 900 tokens on new sessions only. UserPromptSubmit adds a 50-token reminder each turn. Both feed the model, which either writes output or calls a tool. Tool output returns through the PostToolUse hook, which filters, scrubs, elides and dedups it before the model reads it. Read and Edit results bypass the hook untouched."
    >
      {DEFS}
      <PipelineHook node={SESSION} active={session} onShow={onShow} onClear={onClear} />
      <PipelineHook node={PROMPT} active={prompt} onShow={onShow} onClear={onClear} />
      {MODEL}
      <path d="M 196 60 L 282 88" stroke={arrowStroke(session)} fill="none" markerEnd={arrowEnd(session)} />
      <path d="M 196 132 L 282 106" stroke={arrowStroke(prompt)} fill="none" markerEnd={arrowEnd(prompt)} />
      {OUTPUT_AND_TOOLS}
      <PipelineHook node={POST} active={post} onShow={onShow} onClear={onClear} />
      {TOOL_TO_HOOK}
      {/* the loop back, the whole point of the picture */}
      <path
        d="M 508 251 L 352 251 L 352 138"
        stroke="var(--color-accent)" fill="none"
        strokeWidth={post ? 2.4 : 1.5}
        markerEnd="url(#ar-amber)"
        strokeDasharray={post ? "6 4" : undefined}
      >
        {post ? LOOP_PULSE : null}
      </path>
      {LOOP_NOTE_AND_EXCLUDED}
    </svg>
  );
}
