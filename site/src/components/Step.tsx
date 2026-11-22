/* eslint react/react-in-jsx-scope: off -- Next.js uses the automatic JSX runtime. */

// One numbered step on the Get started rail.
export default function Step({ n, title, children }: Readonly<{ n: number; title: string; children: React.ReactNode }>): React.JSX.Element {
  return (
    <li className="relative grid gap-4 pb-12 pl-14 last:pb-0">
      <span
        aria-hidden
        className="absolute left-0 top-0 grid size-9 place-items-center rounded-full border border-brand-hover/60 bg-accent-soft font-mono text-sm font-semibold text-accent"
      >
        {n}
      </span>
      <h3 className="pt-1 text-lg font-semibold tracking-tight">{title}</h3>
      {children}
    </li>
  );
}
