/* eslint react/react-in-jsx-scope: off -- Next.js uses the automatic JSX runtime. */
import { Glyph } from "./icons";

// A feature glyph on its tinted tile.
export default function Icon({ name }: Readonly<{ name: string }>): React.JSX.Element {
  return (
    <span className="grid size-10 place-items-center rounded-lg border border-brand-hover/40 bg-accent-soft text-accent">
      <Glyph name={name} />
    </span>
  );
}
