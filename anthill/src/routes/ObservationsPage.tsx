import { useState, useCallback, useMemo, useEffect, useRef } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { Pin, PinOff } from "lucide-react";
import { C, alpha } from "../lib/colors";
import { MONO } from "../lib/format";
import type { ObservationSummary } from "../lib/api";
import { useObservations } from "../lib/hooks";
import { FilterBar } from "../components/FilterBar";
import { FreqBar } from "../components/FreqBar";
import { Badge } from "../components/Badge";
import { PanelHandle } from "../components/PanelHandle";
import { DetailView } from "../components/DetailView";
import { useKeyboardNav } from "../lib/useKeyboardNav";
import fuzzysort from "fuzzysort";
import { useProject } from "../lib/useProject";

export default function ObservationsPage() {
  const navigate = useNavigate();
  const { id } = useParams();
  const selectedId = id ? Number(id) : null;
  const { current } = useProject();

  const { observations, error, togglePin: apiTogglePin } = useObservations();
  const [search, setSearch] = useState("");
  const [userFilter, setUserFilter] = useState<string[]>([]);
  const [profileFilter, setProfileFilter] = useState<string | null>(null);
  const [hoveredId, setHoveredId] = useState<number | null>(null);
  const [listWidth, setListWidth] = useState(380);
  const rowsRef = useRef<HTMLDivElement>(null);
  const [hlIdx, setHlIdx] = useState(-1);
  const [focusPanel, setFocusPanel] = useState<"list" | "runs">("list");

  const GRID_CLS =
    "grid grid-cols-[minmax(140px,3fr)_50px_70px_minmax(80px,1fr)_56px] gap-[6px]";

  const togglePin = useCallback(
    (sid: number) => {
      apiTogglePin(sid);
    },
    [apiTogglePin],
  );

  const getFreq = useCallback(
    (s: ObservationSummary) => {
      if (userFilter.length === 0) return s.runs.length;
      return s.runs.filter((r) => userFilter.includes(r.user)).length;
    },
    [userFilter],
  );

  const filtered = useMemo(() => {
    let list: {
      observation: ObservationSummary;
      result: Fuzzysort.Result | null;
    }[];
    if (search) {
      const results = fuzzysort.go(search, observations, {
        key: "name",
        all: true,
      });
      list = results.map((r) => ({ observation: r.obj, result: r }));
    } else {
      list = observations.map((s) => ({ observation: s, result: null }));
    }
    if (profileFilter)
      list = list.filter((item) => item.observation.profile === profileFilter);
    if (!search)
      list.sort((a, b) => getFreq(b.observation) - getFreq(a.observation));
    return list;
  }, [observations, search, profileFilter, getFreq]);

  const maxFreq = useMemo(
    () => Math.max(...filtered.map((f) => getFreq(f.observation)), 1),
    [filtered, getFreq],
  );

  // Reset highlight when filter changes
  useEffect(() => setHlIdx(-1), [filtered]);

  const scrollToHl = useCallback((idx: number) => {
    const container = rowsRef.current;
    if (!container) return;
    const row = container.children[idx] as HTMLElement | undefined;
    row?.scrollIntoView({ block: "nearest" });
  }, []);

  const keyMap = useMemo(() => {
    const shared: Record<string, () => void> = {
      Escape: () => {
        if (focusPanel === "runs") setFocusPanel("list");
        else if (selectedId != null && current?.id != null)
          navigate(`/project/${current.id}`);
      },
      ArrowLeft: () => setFocusPanel("list"),
      ArrowRight: () => {
        if (selectedId != null) setFocusPanel("runs");
      },
      "/": () => {
        const el = document.getElementById(
          "observation-search",
        ) as HTMLInputElement | null;
        el?.focus();
      },
    };

    if (focusPanel === "list") {
      const moveDown = () =>
        setHlIdx((i) => {
          const next = i >= filtered.length - 1 ? 0 : i + 1;
          scrollToHl(next);
          return next;
        });
      const moveUp = () =>
        setHlIdx((i) => {
          const next = i <= 0 ? filtered.length - 1 : i - 1;
          scrollToHl(next);
          return next;
        });
      // eslint-disable-next-line react-hooks/refs -- ref is only read inside callbacks, not during render
      Object.assign(shared, {
        ArrowDown: moveDown,
        j: moveDown,
        ArrowUp: moveUp,
        k: moveUp,
        Enter: () => {
          if (hlIdx >= 0 && hlIdx < filtered.length) {
            const s = filtered[hlIdx].observation;
            if (current?.id == null) return;
            navigate(
              s.id === selectedId
                ? `/project/${current.id}`
                : `/project/${current.id}/observation/${s.id}`,
            );
          }
        },
      });
    }

    return shared;
  }, [focusPanel, filtered, hlIdx, selectedId, navigate, scrollToHl, current]);

  useKeyboardNav(keyMap);

  // Reset focus panel when detail closes
  useEffect(() => {
    if (selectedId == null) setFocusPanel("list");
  }, [selectedId]);

  return (
    <>
      {/* Left: command list */}
      <div
        className="min-w-[280px] shrink-0 flex flex-col"
        style={{
          width: listWidth,
          boxShadow:
            selectedId != null && focusPanel === "list"
              ? `inset 0 0 0 1.5px ${alpha(C.accent, 53)}, 0 0 8px ${alpha(C.accent, 13)}`
              : "none",
          transition: "box-shadow 0.15s",
        }}
      >
        {/* Filters */}
        <div className="px-[12px] py-[6px] border-b border-[var(--c-border)]">
          {error && (
            <div className="text-[#f44] px-[16px] py-[8px] text-[13px]">
              {error}
            </div>
          )}
          <FilterBar
            search={search}
            onSearch={setSearch}
            userFilter={userFilter}
            onUserFilter={setUserFilter}
            profileFilter={profileFilter}
            onProfileFilter={setProfileFilter}
          />
        </div>

        {/* Table header */}
        <div
          className={`${GRID_CLS} px-[12px] py-[6px] text-[10px] font-bold text-dim uppercase tracking-[0.8px] border-b border-[var(--c-border)] bg-surface`}
        >
          <span>Command</span>
          <span>Prof.</span>
          <span>Platform</span>
          <span>Runs</span>
          <span className="text-center">Track</span>
        </div>

        {/* Rows */}
        <div ref={rowsRef} className="flex-1 overflow-y-auto">
          {filtered.length === 0 && (
            <div className="p-[20px] text-center text-dim text-xs">
              {observations.length === 0 ? (
                <span>
                  No observations yet. Run{" "}
                  <code style={{ fontFamily: MONO, color: C.accent as string }}>
                    wezel
                  </code>{" "}
                  in your project to start tracking builds.{" "}
                  <a
                    href="https://github.com/wezel-build/wezel#readme"
                    target="_blank"
                    rel="noreferrer"
                    style={{ color: C.accent as string }}
                  >
                    Docs →
                  </a>
                </span>
              ) : (
                "No commands match filters"
              )}
            </div>
          )}
          {filtered.map(({ observation: s, result }, fi) => {
            const isSel = s.id === selectedId;
            const freq = getFreq(s);
            return (
              <div
                key={s.id}
                onClick={() => {
                  if (current?.id == null) return;
                  navigate(
                    isSel
                      ? `/project/${current.id}`
                      : `/project/${current.id}/observation/${s.id}`,
                  );
                }}
                className={`${GRID_CLS} px-[12px] py-[8px] items-center cursor-pointer transition-all duration-100`}
                style={{
                  background: isSel
                    ? alpha(C.accent, 6)
                    : fi === hlIdx || hoveredId === s.id
                      ? C.surface2
                      : "transparent",
                  borderLeft: isSel
                    ? `2px solid ${C.accent}`
                    : "2px solid transparent",
                }}
                onMouseEnter={() => {
                  if (!isSel) setHoveredId(s.id);
                }}
                onMouseLeave={() => {
                  setHoveredId(null);
                }}
              >
                <span
                  className="text-[11px] font-medium font-mono overflow-hidden text-ellipsis whitespace-nowrap"
                  style={{ color: isSel ? C.text : C.textMid }}
                >
                  {result ? (
                    <>
                      {result.highlight((m, i) => (
                        <span
                          key={i}
                          style={{ color: C.accent as string, fontWeight: 700 }}
                        >
                          {m}
                        </span>
                      ))}
                    </>
                  ) : (
                    s.name
                  )}
                </span>
                <Badge
                  color={s.profile === "dev" ? C.textDim : C.amber}
                  bg={s.profile === "dev" ? C.surface3 : alpha(C.amber, 8)}
                >
                  {s.profile === "dev" ? "dev" : "rel"}
                </Badge>
                <span
                  className="text-[10px] font-mono overflow-hidden text-ellipsis whitespace-nowrap"
                  style={{ color: s.platform ? C.textMid : C.textDim }}
                >
                  {s.platform ?? "—"}
                </span>
                <FreqBar value={freq} max={maxFreq} />
                <div className="flex justify-center">
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      togglePin(s.id);
                    }}
                    className="bg-transparent border-0 cursor-pointer p-[2px] flex"
                    style={{
                      color: s.pinned ? C.accent : (C.textDim as string),
                      opacity: s.pinned ? 1 : 0.5,
                    }}
                  >
                    {s.pinned ? <Pin size={13} /> : <PinOff size={13} />}
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      </div>

      {/* Resize handle + detail */}
      <PanelHandle onDrag={(d) => setListWidth((w) => Math.max(280, w + d))} />
      {selectedId == null && (
        <div className="flex-1 flex items-center justify-center border-l border-[var(--c-border)]">
          <div className="flex flex-col gap-[10px] max-w-[340px]">
            {observations.length === 0 ? (
              <>
                <div
                  className="text-[11px] font-mono font-semibold"
                  style={{ color: C.accent }}
                >
                  no observations yet
                </div>
                <p
                  className="text-[11px] font-mono leading-[1.6]"
                  style={{ color: C.textMid }}
                >
                  Observations are recorded by{" "}
                  <span style={{ color: C.text }}>Pheromone</span>, the local
                  agent that hooks into your shell and tracks every build
                  command you run. Install it once and it automatically reports
                  to this dashboard in the background. Until then, this list
                  stays empty.
                </p>
                <a
                  href="https://wezel-build.github.io/docs/"
                  target="_blank"
                  rel="noreferrer"
                  className="text-[11px] font-mono no-underline"
                  style={{ color: C.accent }}
                >
                  install Pheromone →
                </a>
              </>
            ) : (
              <div
                className="text-[11px] font-mono"
                style={{ color: C.textDim }}
              >
                select an observation
              </div>
            )}
            <a
              href="https://wezel-build.github.io/docs/"
              target="_blank"
              rel="noreferrer"
              className="text-[11px] font-mono no-underline"
              style={{ color: C.accent }}
            >
              docs →
            </a>
          </div>
        </div>
      )}
      {selectedId != null && (
        <div
          className="flex-1 p-[12px] overflow-hidden bg-bg"
          style={{
            boxShadow:
              focusPanel === "runs"
                ? `inset 0 0 0 1.5px ${alpha(C.accent, 53)}, 0 0 8px ${alpha(C.accent, 13)}`
                : "none",
            transition: "box-shadow 0.15s",
          }}
        >
          <DetailView
            key={selectedId}
            observationId={selectedId}
            keyboardActive={focusPanel === "runs"}
            userFilter={userFilter}
          />
        </div>
      )}
    </>
  );
}
