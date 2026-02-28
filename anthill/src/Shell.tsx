import { useState } from "react";
import { Outlet, Link, useLocation } from "react-router-dom";
import { Workflow, GitCommit } from "lucide-react";
import { ThemeCtx, THEMES, THEME_ORDER, type ThemeKey } from "./lib/theme";
import { MONO, SANS } from "./lib/format";
import { MOCK_SCENARIOS, MOCK_COMMITS } from "./lib/data";

export default function Shell() {
  const [themeKey, setThemeKey] = useState<ThemeKey>("warm");
  const theme = THEMES[themeKey];
  const C = theme.C;
  const location = useLocation();
  const latestCommit = MOCK_COMMITS[MOCK_COMMITS.length - 1];
  const onCommitPage = location.pathname.startsWith("/commit");

  return (
    <ThemeCtx.Provider value={theme}>
      <div
        style={{
          width: "100vw",
          height: "100vh",
          background: C.bg,
          color: C.text,
          fontFamily: SANS,
          display: "flex",
          flexDirection: "column",
          overflow: "hidden",
        }}
      >
        {/* ── Top bar ──────────────────────────────────────── */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            padding: "0 16px",
            height: 40,
            minHeight: 40,
            borderBottom: `1px solid ${C.border}`,
            background: C.surface,
            justifyContent: "space-between",
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: 14 }}>
            <Link
              to="/"
              style={{
                display: "flex",
                alignItems: "center",
                gap: 8,
                textDecoration: "none",
              }}
            >
              <Workflow size={18} color={C.accent} strokeWidth={2.5} />
              <span
                style={{
                  fontSize: 15,
                  fontWeight: 800,
                  color: C.accent,
                  letterSpacing: -0.5,
                }}
              >
                wezel
              </span>
            </Link>
            <div style={{ width: 1, height: 16, background: C.border }} />
            <Link
              to="/"
              style={{
                fontSize: 10,
                fontFamily: MONO,
                fontWeight: 600,
                color: !onCommitPage ? C.accent : C.textDim,
                textDecoration: "none",
                letterSpacing: 0.3,
                textTransform: "uppercase",
              }}
            >
              scenarios
            </Link>
            <Link
              to={`/commit/${latestCommit.shortSha}`}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 4,
                fontSize: 10,
                fontFamily: MONO,
                fontWeight: 600,
                color: onCommitPage ? C.accent : C.textDim,
                textDecoration: "none",
                letterSpacing: 0.3,
                textTransform: "uppercase",
              }}
            >
              <GitCommit size={11} />
              commits
              {latestCommit.status === "running" && (
                <span
                  style={{
                    width: 6,
                    height: 6,
                    borderRadius: 3,
                    background: C.amber,
                    display: "inline-block",
                  }}
                />
              )}
            </Link>
          </div>
          <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
            <div style={{ fontSize: 10, color: C.textDim, fontFamily: MONO }}>
              {MOCK_SCENARIOS.length} commands ·{" "}
              {MOCK_SCENARIOS.filter((s) => s.pinned).length} tracked
            </div>
            <button
              onClick={() =>
                setThemeKey((k) => {
                  const i = THEME_ORDER.indexOf(k);
                  return THEME_ORDER[(i + 1) % THEME_ORDER.length];
                })
              }
              style={{
                background: C.surface2,
                border: `1px solid ${C.border}`,
                borderRadius: 4,
                padding: "2px 8px",
                cursor: "pointer",
                color: C.textMid,
                fontSize: 10,
                fontFamily: MONO,
              }}
            >
              {themeKey}
            </button>
          </div>
        </div>

        {/* ── Page content ──────────────────────────────────── */}
        <div style={{ flex: 1, display: "flex", overflow: "hidden" }}>
          <Outlet />
        </div>
      </div>
    </ThemeCtx.Provider>
  );
}
