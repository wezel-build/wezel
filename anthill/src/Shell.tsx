import { useState, useRef, useEffect } from "react";
import { Outlet, Link, useLocation, useNavigate } from "react-router-dom";
import { GitCommit, ChevronDown, LogOut, Settings } from "lucide-react";
import { ThemeCtx, THEMES, THEME_ORDER, type ThemeKey } from "./lib/theme";
import { useOverview } from "./lib/hooks";
import { useProject } from "./lib/useProject";
import { useAuth } from "./lib/AuthContext";

export default function Shell() {
  const { user, logout } = useAuth();
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
  const location = useLocation();
  const navigate = useNavigate();

  const { C: themeColors } = theme;
  useEffect(() => {
    const root = document.documentElement;
    root.style.setProperty("--c-bg", themeColors.bg);
    root.style.setProperty("--c-surface", themeColors.surface);
    root.style.setProperty("--c-surface2", themeColors.surface2);
    root.style.setProperty("--c-surface3", themeColors.surface3);
    root.style.setProperty("--c-border", themeColors.border);
    root.style.setProperty("--c-text", themeColors.text);
    root.style.setProperty("--c-text-mid", themeColors.textMid);
    root.style.setProperty("--c-text-dim", themeColors.textDim);
    root.style.setProperty("--c-accent", themeColors.accent);
    root.style.setProperty("--c-green", themeColors.green);
    root.style.setProperty("--c-amber", themeColors.amber);
    root.style.setProperty("--c-red", themeColors.red);
    root.style.setProperty("--c-pink", themeColors.pink);
    root.style.setProperty("--c-cyan", themeColors.cyan);
  }, [themeColors]);

  const { projects, current, setCurrent, loaded } = useProject();

  useEffect(() => {
    document.title = current ? `wezel · ${current.name}` : "wezel";
  }, [current]);
  const [projectOpen, setProjectOpen] = useState(false);
  const dropRef = useRef<HTMLDivElement>(null);

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
  const section =
    current != null
      ? location.pathname.includes("/branch/")
        ? "timeline"
        : location.pathname.includes("/bisections")
          ? "bisections"
          : location.pathname.includes("/observation")
            ? "observations"
            : "commits"
      : null;
  const onNewPage = location.pathname === "/projects/create";
  const onAdminPage = location.pathname.startsWith("/admin");

  return (
    <ThemeCtx.Provider
      value={{ heatColor: theme.heatColor, dark: theme.dark, colors: theme.C }}
    >
      <div className="w-screen h-screen bg-bg text-fg font-sans flex flex-col overflow-hidden">
        {/* ── Top bar ──────────────────────────────────────── */}
        <div className="flex items-center px-4 h-[40px] min-h-[40px] border-b border-[var(--c-border)] bg-surface justify-between">
          <div className="flex items-center gap-[14px]">
            <Link to="/" className="flex items-center gap-[8px] no-underline">
              <img src="/wezel.svg" width={18} height={18} alt="wezel" />
              <span className="text-[15px] font-extrabold text-accent tracking-[-0.5px]">
                wezel
              </span>
            </Link>

            {current && !onNewPage && (
              <>
                <div className="w-[1px] h-[16px] bg-[var(--c-border)]" />
                {/* ── Project switcher ── */}
                <div ref={dropRef} className="relative">
                  <button
                    onClick={() => setProjectOpen((o) => !o)}
                    className="flex items-center gap-[4px] bg-transparent border-none cursor-pointer text-xs font-mono font-semibold text-fg px-[6px] py-[2px] rounded"
                  >
                    {current.name}
                    <ChevronDown size={12} color="var(--c-text-dim)" />
                  </button>
                  {projectOpen && (
                    <div className="absolute top-[calc(100%+4px)] left-0 bg-surface border border-[var(--c-border)] rounded-md py-[4px] z-[100] min-w-[200px] shadow-[0_4px_12px_rgba(0,0,0,0.3)]">
                      {projects.map((p) => (
                        <button
                          key={p.id}
                          onClick={() => {
                            setCurrent(p);
                            setProjectOpen(false);
                            navigate(`/project/${p.id}`);
                          }}
                          className="block w-full text-left bg-transparent border-none cursor-pointer py-[6px] pr-[8px] pl-[12px] font-mono text-xs"
                          style={{
                            color:
                              p.id === current?.id
                                ? "var(--c-accent)"
                                : "var(--c-text)",
                            background:
                              p.id === current?.id
                                ? "var(--c-surface2)"
                                : "transparent",
                          }}
                        >
                          <div className="font-semibold">{p.name}</div>
                          <div className="text-[10px] text-dim mt-[1px]">
                            {p.upstream}
                          </div>
                        </button>
                      ))}
                    </div>
                  )}
                </div>
                <div className="w-[1px] h-[16px] bg-[var(--c-border)]" />
                <Link
                  to={current ? `/project/${current.id}` : "/"}
                  className="flex items-center gap-[4px] no-underline font-mono text-[11px] font-semibold tracking-[0.3px] uppercase"
                  style={{
                    color:
                      section === "commits"
                        ? "var(--c-accent)"
                        : "var(--c-text-dim)",
                  }}
                >
                  <GitCommit size={11} />
                  commits
                </Link>
                <Link
                  to={current ? `/project/${current.id}/observations` : "/"}
                  className="no-underline font-mono text-[11px] font-semibold tracking-[0.3px] uppercase"
                  style={{
                    color:
                      section === "observations"
                        ? "var(--c-accent)"
                        : "var(--c-text-dim)",
                  }}
                >
                  observations
                </Link>
                <Link
                  to={
                    current
                      ? `/project/${current.id}/branch/main/timeline`
                      : "/"
                  }
                  className="no-underline font-mono text-[11px] font-semibold tracking-[0.3px] uppercase"
                  style={{
                    color:
                      section === "timeline"
                        ? "var(--c-accent)"
                        : "var(--c-text-dim)",
                  }}
                >
                  timeline
                </Link>
                <Link
                  to={current ? `/project/${current.id}/bisections` : "/"}
                  className="no-underline font-mono text-[11px] font-semibold tracking-[0.3px] uppercase"
                  style={{
                    color:
                      section === "bisections"
                        ? "var(--c-accent)"
                        : "var(--c-text-dim)",
                  }}
                >
                  bisections
                </Link>
              </>
            )}
          </div>
          <div className="flex items-center gap-[12px]">
            {current && !onNewPage && (
              <div className="text-[10px] text-dim font-mono">
                {overview.observationCount} commands · {overview.trackedCount}{" "}
                tracked
              </div>
            )}
            <Link
              to="/admin/pheromones"
              title="Pheromone admin"
              className="flex items-center no-underline"
              style={{
                color: onAdminPage ? "var(--c-accent)" : "var(--c-text-dim)",
              }}
            >
              <Settings size={13} />
            </Link>
            <button
              onClick={() =>
                setThemeKeyPersist((k) => {
                  const i = THEME_ORDER.indexOf(k);
                  return THEME_ORDER[(i + 1) % THEME_ORDER.length];
                })
              }
              className="bg-surface2 border border-[var(--c-border)] rounded px-2 py-[4px] cursor-pointer text-mid text-[10px] font-mono"
            >
              {themeKey}
            </button>
            {user && (
              <>
                <div className="w-[1px] h-[16px] bg-[var(--c-border)]" />
                <img
                  src={`https://github.com/${user.login}.png?size=24`}
                  alt={user.login}
                  width={20}
                  height={20}
                  className="rounded-[50%] block"
                />
                <span className="text-[11px] font-mono text-mid">
                  {user.login}
                </span>
                <button
                  onClick={logout}
                  title="Sign out"
                  className="bg-transparent border-none cursor-pointer text-dim p-[2px] flex items-center"
                >
                  <LogOut size={13} />
                </button>
              </>
            )}
          </div>
        </div>

        {/* ── Page content ──────────────────────────────────── */}
        <div className="flex-1 flex overflow-hidden">
          <Outlet />
        </div>
      </div>
    </ThemeCtx.Provider>
  );
}
