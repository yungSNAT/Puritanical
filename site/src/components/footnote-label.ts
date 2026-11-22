import { createContext } from "react";

// The footnote heading id for one rendered transcript, scoped by Markdown so
// several transcripts on a page never share an id.
export const FootnoteLabel = createContext("footnote-label");
