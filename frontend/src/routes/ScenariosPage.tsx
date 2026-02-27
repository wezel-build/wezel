import { useState, useCallback, useMemo } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { Pin, PinOff } from "lucide-react";
import { useTheme } from "../lib/theme";
import { MONO } from "../lib/format";
import { MOCK_SCENARIOS, type Scenario } from "../lib/data";
import { FilterBar } from "../components/FilterBar";
import { FreqBar } from "../components/FreqBar";
import { Badge } from "../components/Badge";
import { PanelHandle } from "../components/PanelHandle";
import { DetailView } from "./ScenarioDetailPage";

export default function ScenariosPage() {
  const { C } = useTheme();
  const navigate = useNavigate();
  const { id } = useParams();
  const selectedId = id ? Number(id) : null;

  const [scenarios, setScenarios] = useState(MOCK_SCENARIOS);
  const [search, setSearch] = useState("");
  const [userFilter, setUserFilter] = useState<string[]>([]);
  const [profileFilter, setProfileFilter] = useState<string | null>(null);
  const [listWidth, setListWidth] = useState(380);

  const togglePin = useCallback((sid: number) => {
    setScenarios((prev) =>
      prev.map((s) => (s.id === sid ? { ...s, pinned: !s.pinned } : s)),
    );
  }, []);

  const getFreq = useCallback(
    (s: Scenario) => {
      if (userFilter.length === 0) return s.runs.length;
      return s.runs.filter((r) => userFilter.includes(r.user)).length;
    },
    [userFilter],
  );

  const filtered = useMemo(() => {
    let list = [...scenarios];
    if (search) {
      const q = search.toLowerCase();
      list = list.filter((s) => s.name.toLowerCase().includes(q));
    }
    if (profileFilter) list = list.filter((s) => s.profile === profileFilter);
    list.sort((a, b) => getFreq(b) - getFreq(a));
    return list;
  }, [scenarios, search, profileFilter, getFreq]);

  const maxFreq = useMemo(
    () => Math.max(...filtered.map(getFreq), 1),
    [filtered, getFreq],
  );

  const selected =
    selectedId != null
      ? (scenarios.find((s) => s.id === selectedId) ?? null)
      : null;

  return (
    <>
      {/* Left: command list */}
      <div
        style={{
          width: selected ? listWidth : "100%",
          minWidth: 280,
          flexShrink: 0,
          display: "flex",
          flexDirection: "column",
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
        <div style={{ flex: 1, overflowY: "auto" }}>
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
          {filtered.map((s) => {
            const isSel = s.id === selectedId;
            const freq = getFreq(s);
            return (
              <div
                key={s.id}
                onClick={() =>
                  navigate(isSel ? "/" : `/scenario/${s.id}`)
                }
                style={{
                  display: "grid",
                  gridTemplateColumns:
                    "minmax(140px, 3fr) 50px minmax(80px, 1fr) 56px",
                  gap: 6,
                  padding: "6px 12px",
                  alignItems: "center",
                  cursor: "pointer",
                  background: isSel ? C.accent + "10" : "transparent",
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
                    e.currentTarget.style.background = "transparent";
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
                  {s.name}
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
      {selected && (
        <PanelHandle
          onDrag={(d) => setListWidth((w) => Math.max(280, w + d))}
        />
      )}
      {selected && (
        <div
          style={{
            flex: 1,
            padding: 12,
            overflow: "hidden",
            background: C.bg,
          }}
        >
          <DetailView key={selected.id} scenario={selected} />
        </div>
      )}
    </>
  );
}