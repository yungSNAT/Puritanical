import { useCallback } from "react";
import { TASKS } from "./compare-data";

interface Props {
  selected: string;
  onSelect: (id: string) => void;
}

export default function TaskPicker({ selected, onSelect }: Props): React.JSX.Element {
  const choose = useCallback(
    (event: React.MouseEvent<HTMLButtonElement>) => {
      onSelect(event.currentTarget.value);
    },
    [onSelect],
  );
  return (
    <div className="mt-8 flex flex-wrap gap-2">
      {TASKS.map((t) => (
        <button
          key={t.id}
          type="button"
          value={t.id}
          onClick={choose}
          aria-pressed={t.id === selected}
          className={`rounded-md border px-3 py-1.5 font-mono text-xs transition-colors ${
            t.id === selected
              ? "border-brand-hover bg-accent-soft text-accent"
              : "border-line text-dim hover:border-brand-hover/50 hover:text-fg"
          }`}
        >
          {t.id}
          <span className="ml-1.5 opacity-60">{t.kind}</span>
        </button>
      ))}
    </div>
  );
}
