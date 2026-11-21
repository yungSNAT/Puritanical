/* eslint react/react-in-jsx-scope: off -- Next.js uses the automatic JSX runtime. */
import { useContext } from "react";
import { FootnoteLabel } from "./footnote-label";

// Transcript h2, demoted to h5 inside the page; the footnote heading gets its scoped id.
export default function MarkdownHeading({ id, className, children }: Readonly<React.ComponentPropsWithoutRef<"h2">>): React.JSX.Element {
  const footnoteLabel = useContext(FootnoteLabel);
  return (
    <h5 id={id === "footnote-label" ? footnoteLabel : id} className={className ?? "mt-4 mb-1.5 text-[12.5px] font-bold first:mt-0"}>
      {children}
    </h5>
  );
}
