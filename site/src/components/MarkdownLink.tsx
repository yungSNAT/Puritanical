/* eslint react/react-in-jsx-scope: off -- Next.js uses the automatic JSX runtime. */
import { useContext } from "react";
import { FootnoteLabel } from "./footnote-label";

interface FootnoteLinkProps extends React.ComponentPropsWithoutRef<"a"> {
  "data-footnote-ref"?: boolean;
  "data-footnote-backref"?: boolean;
}

// Transcript link: external links open in a new tab, footnote links stay in
// place and point at the scoped footnote heading.
export default function MarkdownLink({
  href, title, id, className, children,
  "aria-describedby": describedBy,
  "aria-label": label,
  "data-footnote-ref": footnoteRef,
  "data-footnote-backref": footnoteBackref,
}: Readonly<FootnoteLinkProps>): React.JSX.Element {
  const footnoteLabel = useContext(FootnoteLabel);
  return (
    <a
      className={className ?? "text-accent underline-offset-2 hover:underline"}
      target={href?.startsWith("#") === true ? undefined : "_blank"}
      rel="noreferrer"
      href={href}
      title={title}
      id={id}
      aria-describedby={describedBy === "footnote-label" ? footnoteLabel : describedBy}
      aria-label={label}
      data-footnote-ref={footnoteRef}
      data-footnote-backref={footnoteBackref}
    >
      {children}
    </a>
  );
}
