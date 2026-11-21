/* eslint react/react-in-jsx-scope: off -- Next.js uses the automatic JSX runtime. */

export interface HookNode {
  id: string;
  x: number;
  y: number;
  width: number;
  height: number;
  /** Ring scale while the hook is hovered. */
  scale: string;
  label: string;
  labelY: number;
  labelSize: number;
  labelWeight: number;
  note: string;
  noteY: number;
  stroke: string;
  activeStroke: string;
  strokeWidth: number;
  activeStrokeWidth: number;
}

interface PipelineHookProps {
  node: HookNode;
  active: boolean;
  onShow: React.MouseEventHandler<SVGGElement>;
  onClear: () => void;
}

// One hoverable hook box in the Pipeline diagram. Hovering lifts the element
// itself rather than dimming its neighbours: a thicker accent stroke and a
// soft ring, so attention is added, not removed.
export default function PipelineHook({ node, active, onShow, onClear }: Readonly<PipelineHookProps>): React.JSX.Element {
  return (
    <g className="cursor-pointer" data-hook={node.id} onMouseEnter={onShow} onMouseLeave={onClear}>
      <rect
        className="ring"
        x={node.x - 4} y={node.y - 4} width={node.width + 8} height={node.height + 8} rx="9"
        fill="none" stroke="var(--color-accent)"
        opacity={active ? 0.35 : 0}
        transform={active ? node.scale : "scale(0.98)"}
      />
      <rect x={node.x} y={node.y} width={node.width} height={node.height} rx="6" fill="var(--color-accent-soft)"
        stroke={active ? node.activeStroke : node.stroke}
        strokeWidth={active ? node.activeStrokeWidth : node.strokeWidth} />
      <text x={node.x + node.width / 2} y={node.labelY} textAnchor="middle" fontSize={node.labelSize} fontWeight={node.labelWeight} fill="currentColor">{node.label}</text>
      <text x={node.x + node.width / 2} y={node.noteY} textAnchor="middle" fontSize="11" fill="var(--color-dim)">{node.note}</text>
    </g>
  );
}
