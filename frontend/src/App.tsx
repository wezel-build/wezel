import { useState, useCallback, useMemo } from "react";
import { Workflow, Search, X, Pin, PinOff } from "lucide-react";
import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
  Handle,
  Position,
  BackgroundVariant,
  MarkerType,
  type Node,
  type Edge,
  type NodeProps,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";

import usersData from "./mock_data/users.json";
import scenariosData from "./mock_data/scenarios.json";
import graph1 from "./mock_data/graphs/1.json";
import graph2 from "./mock_data/graphs/2.json";
import graph3 from "./mock_data/graphs/3.json";
import graph4 from "./mock_data/graphs/4.json";
import graph5 from "./mock_data/graphs/5.json";
import graph6 from "./mock_data/graphs/6.json";
import graph7 from "./mock_data/graphs/7.json";
import graph8 from "./mock_data/graphs/8.json";

// ── Data model ───────────────────────────────────────────────────────────────

interface CrateNode {
  name: string;
  heat: number; // 0–100
  deps: string[];
}

interface Scenario {
  id: number;
  name: string;
  profile: "dev" | "release";
  /** Per-user execution counts — aggregated for display */
  userFreqs: Record<string, number>;
  pinned: boolean;
  avgBuildMs: number;
  llvmLines: number;
  cratesInGraph: number;
  crateGraph: CrateNode[];
}

// ── Heat color scale ─────────────────────────────────────────────────────────

function heatColor(heat: number): { border: string; bg: string; text: string } {
  if (heat >= 80) return { border: "#ef4444", bg: "#3b1118", text: "#fca5a5" };
  if (heat >= 60) return { border: "#f59e0b", bg: "#352008", text: "#fcd34d" };
  if (heat >= 40) return { border: "#eab308", bg: "#2e2a08", text: "#fde68a" };
  if (heat >= 20) return { border: "#6366f1", bg: "#1c1a3a", text: "#a5b4fc" };
  return { border: "#334155", bg: "#111827", text: "#64748b" };
}

// ── Mock data (from JSON) ────────────────────────────────────────────────────

const USERS: string[] = usersData;

const graphsById: Record<number, CrateNode[]> = {
  1: graph1,
  2: graph2,
  3: graph3,
  4: graph4,
  5: graph5,
  6: graph6,
  7: graph7,
  8: graph8,
};

const MOCK_SCENARIOS: Scenario[] = (
  scenariosData as Omit<Scenario, "crateGraph">[]
).map((s) => ({
  ...s,
  crateGraph: graphsById[s.id] ?? [],
}));

// ── Theme ────────────────────────────────────────────────────────────────────

const C = {
  bg: "#0a0a14",
  surface: "#12121f",
  surface2: "#1a1a2e",
  surface3: "#22223a",
  border: "#262640",
  text: "#d4d4e8",
  textMid: "#9494b8",
  textDim: "#5a5a7a",
  accent: "#6366f1",
  green: "#22c55e",
  amber: "#f59e0b",
  red: "#ef4444",
  pink: "#ec4899",
  cyan: "#06b6d4",
};

const MONO = "'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace";
const SANS = "'Inter', -apple-system, system-ui, sans-serif";

// ── Helpers ──────────────────────────────────────────────────────────────────

function fmtMs(ms: number): string {
  if (ms >= 60_000) return `${(ms / 60_000).toFixed(1)}m`;
  if (ms >= 1000) return `${(ms / 1000).toFixed(1)}s`;
  return `${ms}ms`;
}

function fmtCount(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(0)}K`;
  return `${n}`;
}

// ── Graph layout ─────────────────────────────────────────────────────────────

function layoutGraph(crateGraph: CrateNode[]): {
  nodes: Node[];
  edges: Edge[];
} {
  const nameToIdx = new Map<string, number>();
  crateGraph.forEach((c, i) => nameToIdx.set(c.name, i));

  const depths = new Map<string, number>();
  function getDepth(name: string): number {
    if (depths.has(name)) return depths.get(name)!;
    const node = crateGraph.find((c) => c.name === name);
    if (!node || node.deps.length === 0) {
      depths.set(name, 0);
      return 0;
    }
    const d =
      1 +
      Math.max(
        ...node.deps.filter((d) => nameToIdx.has(d)).map((d) => getDepth(d)),
      );
    depths.set(name, d);
    return d;
  }
  crateGraph.forEach((c) => getDepth(c.name));

  const maxDepth = Math.max(...Array.from(depths.values()), 0);
  const layers: string[][] = Array.from({ length: maxDepth + 1 }, () => []);
  crateGraph.forEach((c) => {
    layers[maxDepth - (depths.get(c.name) ?? 0)].push(c.name);
  });

  const NW = 150,
    NH = 44,
    GX = 32,
    GY = 72;
  const nodes: Node[] = [];
  const edges: Edge[] = [];

  layers.forEach((layer, ly) => {
    const w = layer.length * NW + (layer.length - 1) * GX;
    layer.forEach((name, ci) => {
      const crate = crateGraph.find((c) => c.name === name)!;
      const colors = heatColor(crate.heat);
      nodes.push({
        id: name,
        type: "crate",
        position: { x: -w / 2 + ci * (NW + GX), y: ly * (NH + GY) },
        data: { label: name, heat: crate.heat, colors },
      });
    });
  });

  crateGraph.forEach((crate) => {
    crate.deps.forEach((dep) => {
      if (nameToIdx.has(dep)) {
        const col = heatColor(crate.heat);
        edges.push({
          id: `${crate.name}->${dep}`,
          source: crate.name,
          target: dep,
          style: { stroke: col.border, strokeWidth: 1.5, opacity: 0.45 },
          markerEnd: {
            type: MarkerType.ArrowClosed,
            color: col.border,
            width: 12,
            height: 12,
          },
        });
      }
    });
  });

  return { nodes, edges };
}

// ── ReactFlow crate node ─────────────────────────────────────────────────────

function CrateNodeComponent({ data }: NodeProps) {
  const d = data as {
    label: string;
    heat: number;
    colors: { border: string; bg: string; text: string };
  };
  return (
    <div
      style={{
        background: d.colors.bg,
        border: `1.5px solid ${d.colors.border}`,
        borderRadius: 6,
        padding: "4px 10px",
        color: d.colors.text,
        fontSize: 11,
        fontFamily: MONO,
        fontWeight: 500,
        minWidth: 100,
        textAlign: "center",
        boxShadow: `0 0 8px ${d.colors.border}22`,
      }}
    >
      <Handle
        type="target"
        position={Position.Top}
        style={{
          background: d.colors.border,
          width: 5,
          height: 5,
          border: "none",
        }}
      />
      <div
        style={{
          fontSize: 8,
          color: d.colors.border,
          letterSpacing: 0.8,
          marginBottom: 1,
        }}
      >
        {d.heat}%
      </div>
      <div>{d.label}</div>
      <Handle
        type="source"
        position={Position.Bottom}
        style={{
          background: d.colors.border,
          width: 5,
          height: 5,
          border: "none",
        }}
      />
    </div>
  );
}

const nodeTypes = { crate: CrateNodeComponent };

// ── Small components ─────────────────────────────────────────────────────────

function FreqBar({ value }: { value: number }) {
  const col = value >= 70 ? C.red : value >= 40 ? C.amber : C.accent;
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
      <div
        style={{
          flex: 1,
          height: 4,
          background: C.surface3,
          borderRadius: 2,
          overflow: "hidden",
        }}
      >
        <div
          style={{
            width: `${value}%`,
            height: "100%",
            background: col,
            borderRadius: 2,
          }}
        />
      </div>
      <span
        style={{
          fontSize: 10,
          color: col,
          minWidth: 24,
          textAlign: "right",
          fontFamily: MONO,
        }}
      >
        {value}
      </span>
    </div>
  );
}

function Badge({
  children,
  color,
  bg,
}: {
  children: React.ReactNode;
  color: string;
  bg: string;
}) {
  return (
    <span
      style={{
        fontSize: 10,
        fontWeight: 600,
        letterSpacing: 0.6,
        padding: "1px 6px",
        borderRadius: 3,
        background: bg,
        color,
        border: `1px solid ${color}33`,
        textTransform: "uppercase",
      }}
    >
      {children}
    </span>
  );
}

function Stat({
  label,
  value,
  color,
}: {
  label: string;
  value: string;
  color: string;
}) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 1 }}>
      <span
        style={{
          fontSize: 9,
          color: C.textDim,
          textTransform: "uppercase",
          letterSpacing: 0.8,
          fontWeight: 600,
        }}
      >
        {label}
      </span>
      <span style={{ fontSize: 15, fontWeight: 700, color, fontFamily: MONO }}>
        {value}
      </span>
    </div>
  );
}

// ── Filter bar ───────────────────────────────────────────────────────────────

function FilterBar({
  search,
  onSearch,
  userFilter,
  onUserFilter,
  profileFilter,
  onProfileFilter,
}: {
  search: string;
  onSearch: (v: string) => void;
  userFilter: string[];
  onUserFilter: (v: string[]) => void;
  profileFilter: string | null;
  onProfileFilter: (v: string | null) => void;
}) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 8,
        padding: "6px 0",
        fontSize: 11,
        flexWrap: "wrap",
      }}
    >
      {/* Search */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 4,
          background: C.surface2,
          border: `1px solid ${C.border}`,
          borderRadius: 4,
          padding: "3px 8px",
          minWidth: 180,
        }}
      >
        <Search size={12} color={C.textDim} />
        <input
          value={search}
          onChange={(e) => onSearch(e.target.value)}
          placeholder="filter commands…"
          style={{
            background: "transparent",
            border: "none",
            outline: "none",
            color: C.text,
            fontSize: 11,
            fontFamily: MONO,
            width: "100%",
          }}
        />
        {search && (
          <button
            onClick={() => onSearch("")}
            style={{
              background: "none",
              border: "none",
              cursor: "pointer",
              padding: 0,
              display: "flex",
            }}
          >
            <X size={11} color={C.textDim} />
          </button>
        )}
      </div>

      {/* User filter */}
      <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
        <span
          style={{
            color: C.textDim,
            fontSize: 10,
            fontWeight: 600,
            letterSpacing: 0.5,
          }}
        >
          USER
        </span>
        {USERS.map((u) => (
          <button
            key={u}
            onClick={() =>
              onUserFilter(
                userFilter.includes(u)
                  ? userFilter.filter((x) => x !== u)
                  : [...userFilter, u],
              )
            }
            style={{
              background: userFilter.includes(u)
                ? C.accent + "22"
                : "transparent",
              border: `1px solid ${userFilter.includes(u) ? C.accent : C.border}`,
              borderRadius: 3,
              padding: "2px 7px",
              cursor: "pointer",
              color: userFilter.includes(u) ? C.accent : C.textMid,
              fontSize: 10,
              fontFamily: MONO,
            }}
          >
            {u}
          </button>
        ))}
      </div>

      {/* Profile filter */}
      <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
        <span
          style={{
            color: C.textDim,
            fontSize: 10,
            fontWeight: 600,
            letterSpacing: 0.5,
          }}
        >
          PROFILE
        </span>
        {(["dev", "release"] as const).map((p) => (
          <button
            key={p}
            onClick={() => onProfileFilter(profileFilter === p ? null : p)}
            style={{
              background: profileFilter === p ? C.accent + "22" : "transparent",
              border: `1px solid ${profileFilter === p ? C.accent : C.border}`,
              borderRadius: 3,
              padding: "2px 7px",
              cursor: "pointer",
              color: profileFilter === p ? C.accent : C.textMid,
              fontSize: 10,
              fontFamily: MONO,
              textTransform: "uppercase",
            }}
          >
            {p}
          </button>
        ))}
      </div>
    </div>
  );
}

// ── Heat legend ──────────────────────────────────────────────────────────────

function HeatLegend() {
  const stops = [
    { label: "cold", heat: 5 },
    { label: "low", heat: 25 },
    { label: "mid", heat: 45 },
    { label: "warm", heat: 65 },
    { label: "hot", heat: 90 },
  ];
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 10,
        fontSize: 9,
        color: C.textDim,
        fontFamily: MONO,
      }}
    >
      <span
        style={{
          fontWeight: 700,
          letterSpacing: 0.5,
          textTransform: "uppercase",
        }}
      >
        rebuild freq
      </span>
      {stops.map((s) => {
        const c = heatColor(s.heat);
        return (
          <div
            key={s.label}
            style={{ display: "flex", alignItems: "center", gap: 3 }}
          >
            <div
              style={{
                width: 8,
                height: 8,
                borderRadius: 2,
                background: c.bg,
                border: `1.5px solid ${c.border}`,
              }}
            />
            <span style={{ color: c.text }}>{s.label}</span>
          </div>
        );
      })}
    </div>
  );
}

// ── Summary panel (right of graph) ───────────────────────────────────────────

function Summary({
  scenario,
  frequency,
}: {
  scenario: Scenario;
  frequency: number;
}) {
  const hotCrates = [...scenario.crateGraph]
    .sort((a, b) => b.heat - a.heat)
    .slice(0, 6);

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 14,
        fontSize: 11,
        minWidth: 180,
      }}
    >
      {/* Metrics */}
      <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
        <Stat
          label="Avg build"
          value={fmtMs(scenario.avgBuildMs)}
          color={C.amber}
        />
        <Stat
          label="LLVM lines"
          value={fmtCount(scenario.llvmLines)}
          color={C.cyan}
        />
        <Stat
          label="Crates"
          value={`${scenario.cratesInGraph}`}
          color={C.pink}
        />
        <Stat
          label="Frequency"
          value={`${frequency}`}
          color={frequency >= 70 ? C.red : C.accent}
        />
      </div>

      <div style={{ height: 1, background: C.border }} />

      {/* Hottest crates */}
      <div>
        <div
          style={{
            fontSize: 9,
            fontWeight: 700,
            color: C.textDim,
            letterSpacing: 0.8,
            textTransform: "uppercase",
            marginBottom: 6,
          }}
        >
          Hottest crates
        </div>
        <div style={{ display: "flex", flexDirection: "column", gap: 3 }}>
          {hotCrates.map((c) => {
            const col = heatColor(c.heat);
            return (
              <div
                key={c.name}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 6,
                  padding: "3px 6px",
                  borderRadius: 3,
                  background: col.bg,
                  border: `1px solid ${col.border}33`,
                }}
              >
                <span
                  style={{
                    fontSize: 10,
                    fontFamily: MONO,
                    color: col.text,
                    flex: 1,
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                  }}
                >
                  {c.name}
                </span>
                <span
                  style={{
                    fontSize: 9,
                    fontFamily: MONO,
                    color: col.border,
                    fontWeight: 700,
                    minWidth: 28,
                    textAlign: "right",
                  }}
                >
                  {c.heat}%
                </span>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

// ── Detail view: graph + summary ─────────────────────────────────────────────

function DetailView({
  scenario,
  frequency,
}: {
  scenario: Scenario;
  frequency: number;
}) {
  const { nodes, edges } = useMemo(
    () => layoutGraph(scenario.crateGraph),
    [scenario.id], // eslint-disable-line react-hooks/exhaustive-deps
  );

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        height: "100%",
        gap: 8,
      }}
    >
      {/* Header row */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 12,
          flexWrap: "wrap",
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <span
            style={{
              fontSize: 13,
              fontWeight: 600,
              color: C.text,
              fontFamily: MONO,
            }}
          >
            {scenario.name}
          </span>
          <Badge
            color={scenario.profile === "dev" ? C.textMid : C.amber}
            bg={scenario.profile === "dev" ? C.surface3 : C.amber + "18"}
          >
            {scenario.profile}
          </Badge>

          {scenario.pinned && (
            <span style={{ fontSize: 10, color: C.accent }}>📌 tracked</span>
          )}
        </div>
        <HeatLegend />
      </div>

      {/* Graph + Summary */}
      <div style={{ flex: 1, display: "flex", gap: 12, minHeight: 0 }}>
        {/* Graph */}
        <div
          style={{
            flex: 1,
            borderRadius: 6,
            border: `1px solid ${C.border}`,
            overflow: "hidden",
            background: C.bg,
          }}
        >
          <ReactFlow
            nodes={nodes}
            edges={edges}
            nodeTypes={nodeTypes}
            fitView
            fitViewOptions={{ padding: 0.25 }}
            colorMode="dark"
            minZoom={0.3}
            maxZoom={2}
            proOptions={{ hideAttribution: true }}
          >
            <Background
              variant={BackgroundVariant.Dots}
              gap={16}
              size={1}
              color="#1a1a2e"
            />
            <Controls
              style={{
                background: C.surface,
                borderRadius: 4,
                border: `1px solid ${C.border}`,
              }}
            />
            <MiniMap
              nodeColor={(n) => {
                const c = (n.data as { colors?: { border: string } })?.colors;
                return c?.border ?? C.accent;
              }}
              maskColor="rgba(0,0,0,0.75)"
              style={{
                background: C.bg,
                border: `1px solid ${C.border}`,
                borderRadius: 4,
                height: 80,
                width: 120,
              }}
            />
          </ReactFlow>
        </div>
        {/* Summary sidebar */}
        <div
          style={{
            width: 200,
            overflowY: "auto",
            padding: "8px 4px",
            borderLeft: `1px solid ${C.border}`,
            paddingLeft: 12,
          }}
        >
          <Summary scenario={scenario} frequency={frequency} />
        </div>
      </div>
    </div>
  );
}

// ── App ──────────────────────────────────────────────────────────────────────

export default function App() {
  const [scenarios, setScenarios] = useState(MOCK_SCENARIOS);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [search, setSearch] = useState("");
  const [userFilter, setUserFilter] = useState<string[]>([]);
  const [profileFilter, setProfileFilter] = useState<string | null>(null);

  const togglePin = useCallback((id: number) => {
    setScenarios((prev) =>
      prev.map((s) => (s.id === id ? { ...s, pinned: !s.pinned } : s)),
    );
  }, []);

  const getFreq = useCallback(
    (s: Scenario) => {
      if (userFilter.length === 0)
        return Object.values(s.userFreqs).reduce((a, b) => a + b, 0);
      return userFilter.reduce((sum, u) => sum + (s.userFreqs[u] ?? 0), 0);
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

  const selected =
    selectedId != null
      ? (scenarios.find((s) => s.id === selectedId) ?? null)
      : null;

  return (
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
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
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
        </div>
        <div style={{ fontSize: 10, color: C.textDim, fontFamily: MONO }}>
          {filtered.length}/{scenarios.length} commands ·{" "}
          {scenarios.filter((s) => s.pinned).length} tracked
        </div>
      </div>

      {/* ── Main ─────────────────────────────────────────── */}
      <div style={{ flex: 1, display: "flex", overflow: "hidden" }}>
        {/* Left: command list */}
        <div
          style={{
            width: selected ? 380 : "100%",
            minWidth: 340,
            flexShrink: 0,
            display: "flex",
            flexDirection: "column",
            borderRight: selected ? `1px solid ${C.border}` : "none",
            transition: "width 0.15s ease",
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
            <span>Freq</span>
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
              return (
                <div
                  key={s.id}
                  onClick={() => setSelectedId(isSel ? null : s.id)}
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
                  <FreqBar value={getFreq(s)} />
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

        {/* Right: detail */}
        {selected && (
          <div
            style={{
              flex: 1,
              padding: 12,
              overflow: "hidden",
              background: C.bg,
            }}
          >
            <DetailView
              key={selected.id}
              scenario={selected}
              frequency={getFreq(selected)}
            />
          </div>
        )}
      </div>
    </div>
  );
}
