/* eslint react/react-in-jsx-scope: off -- Next.js uses the automatic JSX runtime. */
import { ImageResponse } from "next/og";

export const size = { width: 1200, height: 630 };
export const contentType = "image/png";
export const alt = "Chisle: cut your agent's token bill on three axes";

const FRAME = {
  width: "100%",
  height: "100%",
  display: "flex",
  flexDirection: "column",
  justifyContent: "space-between",
  background: "#0e0e10",
  color: "#f4f4f6",
  padding: 72,
  fontFamily: "monospace",
} as const;
const BADGE_ROW = { display: "flex", alignItems: "center", gap: 16 };
const BADGE = {
  background: "rgba(215,135,0,0.25)",
  color: "#ffa028",
  padding: "6px 14px",
  borderRadius: 8,
  fontSize: 32,
};
const HEADLINE = { display: "flex", flexDirection: "column" } as const;
const LINE = { fontSize: 84, fontWeight: 700, lineHeight: 1.05 };
const LINE_ACCENT = { ...LINE, color: "#ffa028" };
const FOOTER = { display: "flex", justifyContent: "space-between", fontSize: 28, color: "#a09fa8" };
const COMMAND = { color: "#d78700" };

export default function OG(): ImageResponse {
  return new ImageResponse(
    (
      <div style={FRAME}>
        <div style={BADGE_ROW}>
          <div style={BADGE}>
            [CHISLE]
          </div>
        </div>
        <div style={HEADLINE}>
          <div style={LINE}>
            Write less. Ship less.
          </div>
          <div style={LINE_ACCENT}>
            Mean more.
          </div>
        </div>
        <div style={FOOTER}>
          <div>Claude Code · coding answers about half as long</div>
          <div style={COMMAND}>npx chisle</div>
        </div>
      </div>
    ),
    size
  );
}
