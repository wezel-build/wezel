import { useState, useCallback, useMemo, useRef } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { Pin, PinOff } from "lucide-react";
import { useTheme } from "../lib/theme";
import { MONO } from "../lib/format";
import type { ScenarioSummary } from "../lib/api";
import { useScenarios } from "../lib/hooks";
import { FilterBar } from "../components/FilterBar";
import { FreqBar } from "../components/FreqBar";
import { Badge } from "../components/Badge";
import { PanelHandle } from "../components/PanelHandle";
import { DetailView } from "./ScenarioDetailPage";
import { useKeyboardNav } from "../lib/useKeyboardNav";
import fuzzysort from "fuzzysort";

export default function ScenariosPage() {
  const { C } = useTheme();
  const navigate = useNavigate();
  const { id } = useParams();
  const selectedId = id ? Number(id) : null;

  const { scenarios, togglePin: apiTogglePin } = useScenarios();
  const [search, setSearch] = useState("");
  const [userFilter, setUserFilter] = useState<string[]>([]);
  const [profileFilter, setProfileFilter] = useState<string | null>(null);
  const [listWidth, setListWidth] = useState(380);
  const rowsRef = useRef<HTMLDivElement>(null);
  const [hlIdx, setHlIdx] = useState(-1);
  const [focusPanel, setFocusPanel] = useState<"list" | "runs">("list");

  const togglePin = useCallback(
    (sid: number) => {
      apiTogglePin(sid);
    },
    [apiTogglePin],
  );

  const getFreq = useCallback(
    (s: ScenarioSummary) => {
      if (userFilter.length === 0) return s.runs.length;
      return s.runs.filter((r) => userFilter.includes(r.user)).length;
    },
    [userFilter],
  );

  const filtered = useMemo(() => {
    let list: { scenario: ScenarioSummary; result: Fuzzysort.Result | null }[];
    if (search) {
      const results = fuzzysort.go(search, scenarios, {
        key: "name",
        all: true,
      });
      list = results.map((r) => ({ scenario: r.obj, result: r }));
    } else {
      list = scenarios.map((s) => ({ scenario: s, result: null }));
    }
    if (profileFilter)
      list = list.filter((item) => item.scenario.profile === profileFilter);
    if (!search) list.sort((a, b) => getFreq(b.scenario) - getFreq(a.scenario));
    return list;
  }, [scenarios, search, profileFilter, getFreq]);

  const maxFreq = useMemo(
    () => Math.max(...filtered.map((f) => getFreq(f.scenario)), 1),
    [filtered, getFreq],
  );

  // Reset highlight when filter changes
  useMemo(() => setHlIdx(-1), [filtered]);

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
        else if (selectedId != null) navigate("/");
      },
      ArrowLeft: () => setFocusPanel("list"),
      ArrowRight: () => {
        if (selectedId != null) setFocusPanel("runs");
      },
      "/": () => {
        const el = document.getElementById(
          "scenario-search",
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
      Object.assign(shared, {
        ArrowDown: moveDown,
        j: moveDown,
        ArrowUp: moveUp,
        k: moveUp,
        Enter: () => {
          if (hlIdx >= 0 && hlIdx < filtered.length) {
            const s = filtered[hlIdx].scenario;
            navigate(s.id === selectedId ? "/" : `/scenario/${s.id}`);
          }
        },
      });
    }

    return shared;
  }, [focusPanel, filtered, hlIdx, selectedId, navigate, scrollToHl]);

  useKeyboardNav(keyMap);

  // Reset focus panel when detail closes
  useMemo(() => {
    if (selectedId == null) setFocusPanel("list");
  }, [selectedId]);

  return (
    <>
      {/* Left: command list */}
      <div
        style={{
          width: selectedId != null ? listWidth : "100%",
          minWidth: 280,
          flexShrink: 0,
          display: "flex",
          flexDirection: "column",
          boxShadow:
            selectedId != null && focusPanel === "list"
              ? `inset 0 0 0 1.5px ${C.accent}88, 0 0 8px ${C.accent}22`
              : "none",
          transition: "box-shadow 0.15s",
        }}
      >
        {/* Filters */}
        <div
          style={{
            padding: "6px 12px",
            borderBottom: `1px solid ${C.border}`,
          }}
        >
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
          style={{
            display: "grid",
            gridTemplateColumns:
              "minmax(140px, 3fr) 50px minmax(80px, 1fr) 56px",
            gap: 6,
            padding: "4px 12px",
            fontSize: 9,
            fontWeight: 700,
            color: C.textDim,
            textTransform: "uppercase",
            letterSpacing: 0.8,
            borderBottom: `1px solid ${C.border}`,
            background: C.surface,
          }}
        >
          <span>Command</span>
          <span>Prof.</span>
          <span>Runs</span>
          <span style={{ textAlign: "center" }}>Track</span>
        </div>

        {/* Rows */}
        <div ref={rowsRef} style={{ flex: 1, overflowY: "auto" }}>
          {filtered.length === 0 && (
            <div
              style={{
                padding: 20,
                textAlign: "center",
                color: C.textDim,
                fontSize: 12,
              }}
            >
              No commands match filters
            </div>
          )}
          {filtered.map(({ scenario: s, result }, fi) => {
            const isSel = s.id === selectedId;
            const freq = getFreq(s);
            return (
              <div
                key={s.id}
                onClick={() => navigate(isSel ? "/" : `/scenario/${s.id}`)}
                style={{
                  display: "grid",
                  gridTemplateColumns:
                    "minmax(140px, 3fr) 50px minmax(80px, 1fr) 56px",
                  gap: 6,
                  padding: "6px 12px",
                  alignItems: "center",
                  cursor: "pointer",
                  background: isSel
                    ? C.accent + "10"
                    : fi === hlIdx
                      ? C.surface2
                      : "transparent",
                  borderLeft: isSel
                    ? `2px solid ${C.accent}`
                    : "2px solid transparent",
                  transition: "all 0.1s",
                }}
                onMouseEnter={(e) => {
                  if (!isSel) e.currentTarget.style.background = C.surface2;
                }}
                onMouseLeave={(e) => {
                  if (!isSel)
                    e.currentTarget.style.background =
                      fi === hlIdx ? C.surface2 : "transparent";
                }}
              >
                <span
                  style={{
                    fontSize: 11,
                    fontWeight: 500,
                    color: isSel ? C.text : C.textMid,
                    fontFamily: MONO,
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                  }}
                >
                  {result ? (
                    <>
                      {result.highlight((m, i) => (
                        <span
                          key={i}
                          style={{ color: C.accent, fontWeight: 700 }}
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
                  bg={s.profile === "dev" ? C.surface3 : C.amber + "15"}
                >
                  {s.profile === "dev" ? "dev" : "rel"}
                </Badge>
                <FreqBar value={freq} max={maxFreq} />
                <div style={{ display: "flex", justifyContent: "center" }}>
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      togglePin(s.id);
                    }}
                    style={{
                      background: "none",
                      border: "none",
                      cursor: "pointer",
                      padding: 2,
                      color: s.pinned ? C.accent : C.textDim,
                      display: "flex",
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
      {selectedId != null && (
        <PanelHandle
          onDrag={(d) => setListWidth((w) => Math.max(280, w + d))}
        />
      )}
      {selectedId != null && (
        <div
          style={{
            flex: 1,
            padding: 12,
            overflow: "hidden",
            background: C.bg,
            boxShadow:
              focusPanel === "runs"
                ? `inset 0 0 0 1.5px ${C.accent}88, 0 0 8px ${C.accent}22`
                : "none",
            transition: "box-shadow 0.15s",
          }}
        >
          <DetailView
            key={selectedId}
            scenarioId={selectedId}
            keyboardActive={focusPanel === "runs"}
            userFilter={userFilter}
          />
        </div>
      )}
    </>
  );
}
