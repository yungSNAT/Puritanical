import { useCallback } from "react";
import { RIVALS, type Task } from "./compare-data";

interface Props {
  task: Task;
  selected: string;
  onSelect: (key: string) => void;
}

export default function ArmPicker({ task, selected, onSelect }: Props): React.JSX.Element {
  const choose = useCallback(
    (event: React.MouseEvent<HTMLButtonElement>) => {
      onSelect(event.currentTarget.value);
    },
    [onSelect],
  );
  return (
    <div className="flex gap-1.5">
      {RIVALS.filter((r) => task.arms.has(r.key)).map((r) => (
        <button
          key={r.key}
          type="button"
          value={r.key}
          onClick={choose}
          aria-pressed={r.key === selected}
          className={`rounded px-2 py-0.5 font-mono text-xs transition-colors ${
            r.key === selected ? "bg-accent-soft text-accent" : "text-dim hover:text-fg"
          }`}
        >
          {r.label}
        </button>
      ))}
    </div>
  );
}
