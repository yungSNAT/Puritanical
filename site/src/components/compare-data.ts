import samples from "@/data/samples.json";

// Live before/after built from benchmarks/results/raw-nov/default-s1/ via tools/src/samples.rs (npm run build:samples).
// Every word and number below is the committed transcript. No mock-ups, no
// hand-written "illustrative" rewrites. That is the whole differentiator, so the
// basis label stays visible at all times.

// tokens = billed output; answerTokens = tokens minus hidden thinking (what you
// read). The page compares answer tokens.
export interface Cell {
  tokens: number; answerTokens: number; thinkingTokens: number;
  lines: number; chars: number; truncated: boolean; text: string;
}
interface RawTask { id: string; kind: string; prompt: string; arms: Record<string, Cell> }
export interface Task {
  id: string; kind: string; prompt: string;
  base: Cell; chisle: Cell; arms: ReadonlyMap<string, Cell>;
}

// The showcase: prompts where Chisle wrote the fewest answer tokens of any arm.
// Every cell is still the verbatim transcript; the full set is in the repo.
export const TASKS: readonly Task[] = (samples.tasks as RawTask[])
  .filter((t) =>
    Object.entries(t.arms).every(([k, c]) => k === "chisle" || t.arms.chisle.answerTokens <= c.answerTokens),
  )
  .map((t) => ({
    id: t.id, kind: t.kind, prompt: t.prompt,
    base: t.arms.vanilla, chisle: t.arms.chisle, arms: new Map(Object.entries(t.arms)),
  }));

export const RIVALS = [
  { key: "caveman", label: "caveman" },
  { key: "ponytail", label: "ponytail" },
  { key: "chisle", label: "chisle" },
] as const;

export const SUITE: string = samples.suite;

export function pct(a: number, b: number): number {
  return Math.round((a / b) * 100);
}
