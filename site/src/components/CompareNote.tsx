import { SUITE } from "./compare-data";

export default function CompareNote({ taskId }: { taskId: string }): React.JSX.Element {
  return (
    <p className="mt-5 max-w-3xl text-xs text-dim">
      <span className="mr-2 rounded bg-accent-soft px-1.5 py-0.5 font-mono text-[10px] text-accent">
        basis: measured
      </span>
      {SUITE} Percentages are answer tokens against the bare model&apos;s.{" "}
      {taskId === "cache" ? (
        <>
          <strong className="text-fg">Read this one carefully:</strong> the prompt ships with
          no codebase attached, so every arm, the bare model included, asks for the stack
          instead of writing a cache. This cell measures how briefly each one asks, not how
          much code it writes.{" "}
        </>
      ) : null}
      Browse every cell in{" "}
      <a
        className="text-accent underline-offset-4 hover:underline"
        href="https://github.com/JayPokale/Chisle/tree/main/benchmarks/results/raw-nov/default-s1"
      >
        benchmarks/results/raw-nov/default-s1
      </a>.
    </p>
  );
}
