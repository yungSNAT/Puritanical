/* eslint react/react-in-jsx-scope: off -- Next.js uses the automatic JSX runtime. */

export default function Logo(): React.JSX.Element {
  return (
    <span className="flex items-center gap-2 font-semibold tracking-tight">
      <span
        aria-hidden
        className="grid size-7 place-items-center rounded-md bg-gradient-to-br from-brand to-[#ffc56b] font-mono text-xs text-on-brand"
      >
        C
      </span>chisle
    </span>
  );
}
