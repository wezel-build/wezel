import { useState, useRef, useEffect } from "react";
import { Outlet, Link, useLocation, useNavigate } from "react-router-dom";
import { Workflow, GitCommit, ChevronDown, Plus } from "lucide-react";
import { ThemeCtx, THEMES, THEME_ORDER, type ThemeKey } from "./lib/theme";
import { MONO, SANS } from "./lib/format";
import { useOverview } from "./lib/hooks";
import { useProject } from "./lib/useProject";

export default function Shell() {
  const [themeKey, setThemeKey] = useState<ThemeKey>(
    () => (localStorage.getItem("themeKey") as ThemeKey) || "warm",
  );
  const setThemeKeyPersist = (fn: (k: ThemeKey) => ThemeKey) => {
    setThemeKey((k) => {
      const next = fn(k);
      localStorage.setItem("themeKey", next);
      return next;
    });
  };
  const theme = THEMES[themeKey];
  const C = theme.C;
  const location = useLocation();
  const navigate = useNavigate();

  const { projects, current, setCurrent, loaded } = useProject();
  const [projectOpen, setProjectOpen] = useState(false);
  const dropRef = useRef<HTMLDivElement>(null);

  // Redirect to /new when loaded with no projects (and not already there)
  useEffect(() => {
    if (
      loaded &&
      projects.length === 0 &&
      location.pathname !== "/projects/create"
    ) {
      navigate("/projects/create", { replace: true });
    }
  }, [loaded, projects.length, location.pathname, navigate]);

  useEffect(() => {
    if (!projectOpen) return;
    const handler = (e: MouseEvent) => {
      if (dropRef.current && !dropRef.current.contains(e.target as Node))
        setProjectOpen(false);
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [projectOpen]);

  const { overview } = useOverview();
  const onCommitPage = location.pathname.startsWith("/commit");
  const onNewPage = location.pathname === "/projects/create";

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

            {current && !onNewPage && (
              <>
                <div style={{ width: 1, height: 16, background: C.border }} />
                {/* ── Project switcher ── */}
                <div ref={dropRef} style={{ position: "relative" }}>
                  <button
                    onClick={() => setProjectOpen((o) => !o)}
                    style={{
                      display: "flex",
                      alignItems: "center",
                      gap: 4,
                      background: "none",
                      border: "none",
                      cursor: "pointer",
                      fontSize: 12,
                      fontFamily: MONO,
                      fontWeight: 600,
                      color: C.text,
                      padding: "2px 6px",
                      borderRadius: 4,
                    }}
                  >
                    {current.name}
                    <ChevronDown size={12} color={C.textDim} />
                  </button>
                  {projectOpen && (
                    <div
                      style={{
                        position: "absolute",
                        top: "calc(100% + 4px)",
                        left: 0,
                        background: C.surface,
                        border: `1px solid ${C.border}`,
                        borderRadius: 6,
                        padding: "4px 0",
                        zIndex: 100,
                        minWidth: 200,
                        boxShadow: "0 4px 12px rgba(0,0,0,.3)",
                      }}
                    >
                      {projects.map((p) => (
                        <button
                          key={p.id}
                          onClick={() => {
                            setCurrent(p);
                            setProjectOpen(false);
                          }}
                          style={{
                            display: "block",
                            width: "100%",
                            textAlign: "left",
                            background:
                              p.id === current?.id ? C.surface2 : "transparent",
                            border: "none",
                            cursor: "pointer",
                            padding: "6px 12px",
                            color: p.id === current?.id ? C.accent : C.text,
                            fontFamily: MONO,
                            fontSize: 12,
                          }}
                        >
                          <div style={{ fontWeight: 600 }}>{p.name}</div>
                          <div
                            style={{
                              fontSize: 10,
                              color: C.textDim,
                              marginTop: 1,
                            }}
                          >
                            {p.upstream}
                          </div>
                        </button>
                      ))}
                      <div
                        style={{
                          borderTop: `1px solid ${C.border}`,
                          padding: "4px 0",
                        }}
                      >
                        <button
                          onClick={() => {
                            setProjectOpen(false);
                            navigate("/projects/create");
                          }}
                          style={{
                            display: "flex",
                            alignItems: "center",
                            gap: 6,
                            width: "100%",
                            textAlign: "left",
                            background: "transparent",
                            border: "none",
                            cursor: "pointer",
                            padding: "6px 12px",
                            color: C.textDim,
                            fontFamily: MONO,
                            fontSize: 11,
                          }}
                        >
                          <Plus size={12} /> New project
                        </button>
                      </div>
                    </div>
                  )}
                </div>
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
                  to={
                    overview.latestCommitShortSha
                      ? `/commit/${overview.latestCommitShortSha}`
                      : "/"
                  }
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
                  {overview.latestCommitStatus === "running" && (
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
              </>
            )}
          </div>
          <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
            {current && !onNewPage && (
              <div style={{ fontSize: 10, color: C.textDim, fontFamily: MONO }}>
                {overview.scenarioCount} commands · {overview.trackedCount}{" "}
                tracked
              </div>
            )}
            <button
              onClick={() =>
                setThemeKeyPersist((k) => {
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
