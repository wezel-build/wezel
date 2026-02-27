import { useState, useCallback, useMemo } from "react";
import { Workflow } from "lucide-react";
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

// ── Data model ───────────────────────────────────────────────────────────────

interface CrateNode {
  name: string;
  /** 0–100: how often this crate gets rebuilt in this scenario */
  heat: number;
  deps: string[];
}

interface Scenario {
  id: number;
  name: string;
  profile: "dev" | "release";
  frequency: number; // 0–100
  pinned: boolean;
  crateGraph: CrateNode[];
}

// ── Heat color scale ─────────────────────────────────────────────────────────

function heatColor(heat: number): { border: string; bg: string; text: string } {
  if (heat >= 80) return { border: "#ef4444", bg: "#451a1a", text: "#fca5a5" };
  if (heat >= 60) return { border: "#f59e0b", bg: "#451a03", text: "#fcd34d" };
  if (heat >= 40) return { border: "#eab308", bg: "#3a3505", text: "#fde68a" };
  if (heat >= 20) return { border: "#6366f1", bg: "#1e1b4b", text: "#a5b4fc" };
  return { border: "#334155", bg: "#0f172a", text: "#64748b" };
}

// ── Mock data ────────────────────────────────────────────────────────────────

const MOCK_SCENARIOS: Scenario[] = [
  {
    id: 1,
    name: "cargo test -p auth_core",
    profile: "dev",
    frequency: 97,
    pinned: true,
    crateGraph: [
      {
        name: "auth_core",
        heat: 95,
        deps: ["crypto_utils", "db_pool", "config"],
      },
      { name: "crypto_utils", heat: 72, deps: ["ring", "base64"] },
      { name: "db_pool", heat: 65, deps: ["sqlx", "config"] },
      { name: "config", heat: 40, deps: ["serde", "toml"] },
      { name: "ring", heat: 5, deps: [] },
      { name: "base64", heat: 3, deps: [] },
      { name: "sqlx", heat: 8, deps: [] },
      { name: "serde", heat: 2, deps: [] },
      { name: "toml", heat: 2, deps: [] },
    ],
  },
  {
    id: 2,
    name: "cargo build --workspace",
    profile: "dev",
    frequency: 84,
    pinned: true,
    crateGraph: [
      {
        name: "wezel_app",
        heat: 90,
        deps: ["auth_core", "api_server", "wezel_ui"],
      },
      { name: "auth_core", heat: 82, deps: ["crypto_utils", "db_pool"] },
      {
        name: "api_server",
        heat: 78,
        deps: ["auth_core", "proto_gen", "db_pool"],
      },
      { name: "wezel_ui", heat: 70, deps: ["api_client", "serde"] },
      { name: "api_client", heat: 55, deps: ["proto_gen", "reqwest"] },
      { name: "proto_gen", heat: 45, deps: ["prost", "tonic"] },
      { name: "crypto_utils", heat: 30, deps: ["ring"] },
      { name: "db_pool", heat: 35, deps: ["sqlx"] },
      { name: "ring", heat: 4, deps: [] },
      { name: "sqlx", heat: 6, deps: [] },
      { name: "prost", heat: 3, deps: [] },
      { name: "tonic", heat: 5, deps: [] },
      { name: "reqwest", heat: 2, deps: [] },
      { name: "serde", heat: 2, deps: [] },
    ],
  },
  {
    id: 3,
    name: "cargo test -p pheromone_agent",
    profile: "dev",
    frequency: 72,
    pinned: false,
    crateGraph: [
      {
        name: "pheromone_agent",
        heat: 98,
        deps: ["fs_watcher", "cargo_metadata", "ipc"],
      },
      { name: "fs_watcher", heat: 60, deps: ["notify", "crossbeam"] },
      { name: "cargo_metadata", heat: 45, deps: ["serde_json", "camino"] },
      { name: "ipc", heat: 70, deps: ["serde", "bincode"] },
      { name: "notify", heat: 5, deps: [] },
      { name: "crossbeam", heat: 3, deps: [] },
      { name: "serde_json", heat: 4, deps: ["serde"] },
      { name: "camino", heat: 2, deps: [] },
      { name: "serde", heat: 2, deps: [] },
      { name: "bincode", heat: 3, deps: [] },
    ],
  },
  {
    id: 4,
    name: "cargo build -p forager --release",
    profile: "release",
    frequency: 58,
    pinned: true,
    crateGraph: [
      {
        name: "forager",
        heat: 92,
        deps: ["scenario_runner", "measure_collect", "ipc"],
      },
      {
        name: "scenario_runner",
        heat: 80,
        deps: ["cargo_metadata", "tempfile"],
      },
      {
        name: "measure_collect",
        heat: 75,
        deps: ["time_utils", "llvm_lines_parser"],
      },
      { name: "ipc", heat: 50, deps: ["serde", "bincode"] },
      { name: "cargo_metadata", heat: 20, deps: ["serde_json"] },
      { name: "tempfile", heat: 5, deps: [] },
      { name: "time_utils", heat: 15, deps: [] },
      { name: "llvm_lines_parser", heat: 60, deps: ["regex"] },
      { name: "serde", heat: 2, deps: [] },
      { name: "bincode", heat: 3, deps: [] },
      { name: "serde_json", heat: 3, deps: [] },
      { name: "regex", heat: 2, deps: [] },
    ],
  },
  {
    id: 5,
    name: "cargo clippy -p wezel_ui",
    profile: "dev",
    frequency: 45,
    pinned: false,
    crateGraph: [
      { name: "wezel_ui", heat: 90, deps: ["api_client", "theme"] },
      { name: "api_client", heat: 55, deps: ["proto_gen", "reqwest"] },
      { name: "theme", heat: 80, deps: ["serde"] },
      { name: "proto_gen", heat: 30, deps: ["prost"] },
      { name: "reqwest", heat: 3, deps: [] },
      { name: "prost", heat: 2, deps: [] },
      { name: "serde", heat: 2, deps: [] },
    ],
  },
  {
    id: 6,
    name: "cargo test --workspace --release",
    profile: "release",
    frequency: 31,
    pinned: false,
    crateGraph: [
      { name: "wezel_app", heat: 88, deps: ["auth_core", "api_server"] },
      { name: "auth_core", heat: 80, deps: ["crypto_utils"] },
      { name: "api_server", heat: 75, deps: ["auth_core", "db_pool"] },
      { name: "crypto_utils", heat: 40, deps: ["ring"] },
      { name: "db_pool", heat: 50, deps: ["sqlx"] },
      { name: "ring", heat: 5, deps: [] },
      { name: "sqlx", heat: 6, deps: [] },
    ],
  },
  {
    id: 7,
    name: "cargo build -p db_migrations",
    profile: "dev",
    frequency: 26,
    pinned: false,
    crateGraph: [
      { name: "db_migrations", heat: 95, deps: ["sqlx", "config"] },
      { name: "config", heat: 40, deps: ["serde", "toml"] },
      { name: "sqlx", heat: 8, deps: [] },
      { name: "serde", heat: 2, deps: [] },
      { name: "toml", heat: 2, deps: [] },
    ],
  },
  {
    id: 8,
    name: "cargo test -p proto_gen",
    profile: "dev",
    frequency: 19,
    pinned: false,
    crateGraph: [
      { name: "proto_gen", heat: 92, deps: ["prost", "tonic", "serde"] },
      { name: "prost", heat: 10, deps: [] },
      { name: "tonic", heat: 12, deps: [] },
      { name: "serde", heat: 2, deps: [] },
    ],
  },
];

// ── Theme ────────────────────────────────────────────────────────────────────

const C = {
  bg: "#0f0f1a",
  surface: "#16162a",
  surface2: "#1e1e3a",
  border: "#2d2d4a",
  text: "#e2e8f0",
  textDim: "#64748b",
  accent: "#6366f1",
  accentDim: "#4f46e5",
  green: "#22c55e",
  greenBg: "#052e16",
  amber: "#f59e0b",
  amberBg: "#451a03",
  red: "#ef4444",
  pink: "#ec4899",
  cyan: "#06b6d4",
};

const FONT =
  "'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif";

// ── Graph layout helper ──────────────────────────────────────────────────────

function layoutGraph(crateGraph: CrateNode[]): {
  nodes: Node[];
  edges: Edge[];
} {
  // Build adjacency: a map from crate name to its index
  const nameToIdx = new Map<string, number>();
  crateGraph.forEach((c, i) => nameToIdx.set(c.name, i));

  // Compute depth (layer) for each node via topological sort
  const depths = new Map<string, number>();
  function getDepth(name: string): number {
    if (depths.has(name)) return depths.get(name)!;
    const node = crateGraph.find((c) => c.name === name);
    if (!node || node.deps.length === 0) {
      depths.set(name, 0);
      return 0;
    }
    const maxChildDepth = Math.max(
      ...node.deps.filter((d) => nameToIdx.has(d)).map((d) => getDepth(d)),
    );
    const d = maxChildDepth + 1;
    depths.set(name, d);
    return d;
  }
  crateGraph.forEach((c) => getDepth(c.name));

  // Group by layer (invert: root at top)
  const maxDepth = Math.max(...Array.from(depths.values()), 0);
  const layers: string[][] = Array.from({ length: maxDepth + 1 }, () => []);
  crateGraph.forEach((c) => {
    const layer = maxDepth - (depths.get(c.name) ?? 0);
    layers[layer].push(c.name);
  });

  const NODE_W = 170;
  const NODE_H = 52;
  const GAP_X = 40;
  const GAP_Y = 90;

  const nodes: Node[] = [];
  const edges: Edge[] = [];

  layers.forEach((layer, layerIdx) => {
    const layerWidth = layer.length * NODE_W + (layer.length - 1) * GAP_X;
    const offsetX = -layerWidth / 2;
    layer.forEach((name, colIdx) => {
      const crate = crateGraph.find((c) => c.name === name)!;
      const colors = heatColor(crate.heat);
      nodes.push({
        id: name,
        type: "crateNode",
        position: {
          x: offsetX + colIdx * (NODE_W + GAP_X),
          y: layerIdx * (NODE_H + GAP_Y),
        },
        data: { label: name, heat: crate.heat, colors },
      });
    });
  });

  crateGraph.forEach((crate) => {
    crate.deps.forEach((dep) => {
      if (nameToIdx.has(dep)) {
        const srcColors = heatColor(crate.heat);
        edges.push({
          id: `${crate.name}->${dep}`,
          source: crate.name,
          target: dep,
          style: { stroke: srcColors.border, strokeWidth: 1.5, opacity: 0.5 },
          markerEnd: {
            type: MarkerType.ArrowClosed,
            color: srcColors.border,
            width: 14,
            height: 14,
          },
        });
      }
    });
  });

  return { nodes, edges };
}

// ── Custom ReactFlow node ────────────────────────────────────────────────────

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
        border: `2px solid ${d.colors.border}`,
        borderRadius: 8,
        padding: "6px 14px",
        color: d.colors.text,
        fontSize: 12,
        fontFamily: "'JetBrains Mono', monospace, " + FONT,
        fontWeight: 500,
        minWidth: 120,
        textAlign: "center",
        boxShadow: `0 0 10px ${d.colors.border}33`,
      }}
    >
      <Handle
        type="target"
        position={Position.Top}
        style={{
          background: d.colors.border,
          width: 6,
          height: 6,
          border: "none",
        }}
      />
      <div
        style={{
          fontSize: 9,
          color: d.colors.border,
          letterSpacing: 1,
          marginBottom: 2,
        }}
      >
        {d.heat}% hot
      </div>
      <div>{d.label}</div>
      <Handle
        type="source"
        position={Position.Bottom}
        style={{
          background: d.colors.border,
          width: 6,
          height: 6,
          border: "none",
        }}
      />
    </div>
  );
}

const nodeTypes = { crateNode: CrateNodeComponent };

// ── Small UI components ──────────────────────────────────────────────────────

function StatusPill({
  label,
  status,
  color,
}: {
  label: string;
  status: string;
  color: string;
}) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 6,
        background: C.surface2,
        border: `1px solid ${C.border}`,
        borderRadius: 20,
        padding: "4px 12px 4px 8px",
        fontSize: 11,
        fontWeight: 500,
      }}
    >
      <div
        style={{
          width: 7,
          height: 7,
          borderRadius: "50%",
          background: color,
          boxShadow: `0 0 6px ${color}`,
        }}
      />
      <span style={{ color: C.textDim, marginRight: 2 }}>{label}</span>
      <span style={{ color }}>{status}</span>
    </div>
  );
}

function FrequencyBar({ value }: { value: number }) {
  const barColor = value >= 70 ? C.red : value >= 40 ? C.amber : C.accent;
  return (
    <div
      style={{ display: "flex", alignItems: "center", gap: 8, minWidth: 100 }}
    >
      <div
        style={{
          flex: 1,
          height: 6,
          background: C.surface2,
          borderRadius: 3,
          overflow: "hidden",
        }}
      >
        <div
          style={{
            width: `${value}%`,
            height: "100%",
            background: barColor,
            borderRadius: 3,
            transition: "width 0.3s ease",
          }}
        />
      </div>
      <span
        style={{
          fontSize: 11,
          color: barColor,
          minWidth: 28,
          textAlign: "right",
        }}
      >
        {value}%
      </span>
    </div>
  );
}

function PinToggle({
  pinned,
  onToggle,
}: {
  pinned: boolean;
  onToggle: () => void;
}) {
  return (
    <button
      onClick={(e) => {
        e.stopPropagation();
        onToggle();
      }}
      style={{
        background: pinned ? `${C.accent}22` : "transparent",
        border: `1px solid ${pinned ? C.accent : C.border}`,
        borderRadius: 6,
        padding: "4px 10px",
        cursor: "pointer",
        color: pinned ? C.accent : C.textDim,
        fontSize: 11,
        fontFamily: FONT,
        fontWeight: 500,
        transition: "all 0.15s ease",
        display: "flex",
        alignItems: "center",
        gap: 4,
      }}
    >
      <span style={{ fontSize: 13 }}>{pinned ? "📌" : "○"}</span>
      <span style={{ fontSize: 10 }}>{pinned ? "Tracked" : "Track"}</span>
    </button>
  );
}

function ProfileBadge({ profile }: { profile: "dev" | "release" }) {
  const isDev = profile === "dev";
  return (
    <span
      style={{
        display: "inline-block",
        fontSize: 10,
        fontWeight: 600,
        textTransform: "uppercase",
        letterSpacing: 0.8,
        padding: "2px 8px",
        borderRadius: 4,
        background: isDev ? C.surface2 : C.amberBg,
        color: isDev ? C.textDim : C.amber,
        border: `1px solid ${isDev ? C.border : `${C.amber}33`}`,
      }}
    >
      {profile}
    </span>
  );
}

// ── Heat legend ──────────────────────────────────────────────────────────────

function HeatLegend() {
  const stops = [
    { label: "Cold", heat: 5 },
    { label: "Low", heat: 25 },
    { label: "Medium", heat: 45 },
    { label: "Warm", heat: 65 },
    { label: "Hot", heat: 90 },
  ];
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 12,
        padding: "6px 12px",
        background: C.surface,
        border: `1px solid ${C.border}`,
        borderRadius: 8,
        fontSize: 10,
        color: C.textDim,
      }}
    >
      <span
        style={{
          fontWeight: 600,
          letterSpacing: 0.5,
          textTransform: "uppercase",
        }}
      >
        Rebuild frequency:
      </span>
      {stops.map((s) => {
        const colors = heatColor(s.heat);
        return (
          <div
            key={s.label}
            style={{ display: "flex", alignItems: "center", gap: 4 }}
          >
            <div
              style={{
                width: 10,
                height: 10,
                borderRadius: 3,
                background: colors.bg,
                border: `2px solid ${colors.border}`,
              }}
            />
            <span style={{ color: colors.text }}>{s.label}</span>
          </div>
        );
      })}
    </div>
  );
}

// ── Graph panel ──────────────────────────────────────────────────────────────

function GraphPanel({ scenario }: { scenario: Scenario }) {
  const { nodes: initialNodes, edges: initialEdges } = useMemo(
    () => layoutGraph(scenario.crateGraph),
    [scenario.id], // eslint-disable-line react-hooks/exhaustive-deps
  );

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        height: "100%",
        gap: 12,
      }}
    >
      {/* Header */}
      <div>
        <div
          style={{
            fontSize: 10,
            color: C.textDim,
            textTransform: "uppercase",
            letterSpacing: 1,
            marginBottom: 4,
          }}
        >
          Build Graph
        </div>
        <div
          style={{
            fontSize: 15,
            fontWeight: 600,
            color: C.text,
            fontFamily: "'JetBrains Mono', monospace, " + FONT,
          }}
        >
          {scenario.name}
        </div>
        <div
          style={{
            display: "flex",
            gap: 10,
            marginTop: 8,
            alignItems: "center",
          }}
        >
          <ProfileBadge profile={scenario.profile} />
          {scenario.pinned && (
            <span style={{ fontSize: 11, color: C.accent }}>
              📌 Tracked by Forager
            </span>
          )}
          <span style={{ fontSize: 11, color: C.textDim }}>
            {scenario.crateGraph.length} crates
          </span>
        </div>
      </div>

      {/* Legend */}
      <HeatLegend />

      {/* Graph */}
      <div
        style={{
          flex: 1,
          borderRadius: 10,
          border: `1px solid ${C.border}`,
          overflow: "hidden",
          background: C.bg,
        }}
      >
        <ReactFlow
          nodes={initialNodes}
          edges={initialEdges}
          nodeTypes={nodeTypes}
          fitView
          fitViewOptions={{ padding: 0.3 }}
          colorMode="dark"
          minZoom={0.3}
          maxZoom={2}
          proOptions={{ hideAttribution: true }}
        >
          <Background
            variant={BackgroundVariant.Dots}
            gap={20}
            size={1}
            color="#1e1e3a"
          />
          <Controls
            style={{
              background: C.surface,
              borderRadius: 6,
              border: `1px solid ${C.border}`,
            }}
          />
          <MiniMap
            nodeColor={(n) => {
              const colors = (n.data as { colors?: { border: string } })
                ?.colors;
              return colors?.border ?? C.accent;
            }}
            maskColor="rgba(0, 0, 0, 0.7)"
            style={{
              background: C.bg,
              border: `1px solid ${C.border}`,
              borderRadius: 6,
            }}
          />
        </ReactFlow>
      </div>
    </div>
  );
}

// ── App ──────────────────────────────────────────────────────────────────────

export default function App() {
  const [scenarios, setScenarios] = useState(MOCK_SCENARIOS);
  const [selectedId, setSelectedId] = useState<number | null>(null);

  const sorted = [...scenarios].sort((a, b) => b.frequency - a.frequency);
  const selected =
    selectedId != null
      ? (scenarios.find((s) => s.id === selectedId) ?? null)
      : null;

  const togglePin = useCallback((id: number) => {
    setScenarios((prev) =>
      prev.map((s) => (s.id === id ? { ...s, pinned: !s.pinned } : s)),
    );
  }, []);

  return (
    <div
      style={{
        width: "100vw",
        height: "100vh",
        background: C.bg,
        color: C.text,
        fontFamily: FONT,
        display: "flex",
        flexDirection: "column",
        overflow: "hidden",
      }}
    >
      {/* ── Top bar ─────────────────────────────────────── */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          padding: "0 24px",
          height: 56,
          minHeight: 56,
          borderBottom: `1px solid ${C.border}`,
          background: C.surface,
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
          <Workflow size={24} color={C.accent} strokeWidth={2.5} />
          <span
            style={{
              fontSize: 20,
              fontWeight: 800,
              color: C.accent,
              letterSpacing: -0.5,
            }}
          >
            wezel
          </span>
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
          <StatusPill label="Pheromone" status="connected" color={C.green} />
          <StatusPill label="Forager" status="idle" color={C.amber} />
        </div>
      </div>

      {/* ── Main content ────────────────────────────────── */}
      <div style={{ flex: 1, display: "flex", overflow: "hidden" }}>
        {/* Scenario list */}
        <div
          style={{
            width: selected ? 420 : "100%",
            minWidth: 380,
            overflowY: "auto",
            padding: 20,
            transition: "width 0.2s ease",
            flexShrink: 0,
          }}
        >
          <div
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
              marginBottom: 16,
            }}
          >
            <div>
              <h2
                style={{
                  fontSize: 15,
                  fontWeight: 700,
                  margin: 0,
                  color: C.text,
                }}
              >
                Build Commands
              </h2>
              <p style={{ fontSize: 12, color: C.textDim, margin: "4px 0 0" }}>
                Ranked by execution frequency from Pheromone
              </p>
            </div>
            <div
              style={{
                fontSize: 11,
                color: C.textDim,
                background: C.surface2,
                padding: "4px 10px",
                borderRadius: 6,
              }}
            >
              {scenarios.length} commands ·{" "}
              {scenarios.filter((s) => s.pinned).length} pinned
            </div>
          </div>

          {/* Table header */}
          <div
            style={{
              display: "grid",
              gridTemplateColumns:
                "minmax(200px, 3fr) 70px minmax(120px, 1fr) 90px",
              gap: 12,
              padding: "8px 16px",
              fontSize: 10,
              fontWeight: 600,
              color: C.textDim,
              textTransform: "uppercase",
              letterSpacing: 1,
              borderBottom: `1px solid ${C.border}`,
            }}
          >
            <span>Command</span>
            <span>Profile</span>
            <span>Frequency</span>
            <span style={{ textAlign: "center" }}>Track</span>
          </div>

          {/* Rows */}
          {sorted.map((s) => {
            const isSelected = s.id === selectedId;
            return (
              <div
                key={s.id}
                onClick={() => setSelectedId(isSelected ? null : s.id)}
                style={{
                  display: "grid",
                  gridTemplateColumns:
                    "minmax(200px, 3fr) 70px minmax(120px, 1fr) 90px",
                  gap: 12,
                  padding: "12px 16px",
                  alignItems: "center",
                  cursor: "pointer",
                  borderRadius: 8,
                  background: isSelected ? `${C.accent}12` : "transparent",
                  borderLeft: isSelected
                    ? `3px solid ${C.accent}`
                    : "3px solid transparent",
                  transition: "all 0.12s ease",
                  marginTop: 2,
                }}
                onMouseEnter={(e) => {
                  if (!isSelected)
                    e.currentTarget.style.background = C.surface2;
                }}
                onMouseLeave={(e) => {
                  if (!isSelected)
                    e.currentTarget.style.background = "transparent";
                }}
              >
                <span
                  style={{
                    fontSize: 13,
                    fontWeight: 500,
                    color: isSelected ? C.text : "#c8c8e0",
                    fontFamily: "'JetBrains Mono', monospace, " + FONT,
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                  }}
                >
                  {s.name}
                </span>
                <ProfileBadge profile={s.profile} />
                <FrequencyBar value={s.frequency} />
                <div style={{ display: "flex", justifyContent: "center" }}>
                  <PinToggle
                    pinned={s.pinned}
                    onToggle={() => togglePin(s.id)}
                  />
                </div>
              </div>
            );
          })}
        </div>

        {/* Graph panel */}
        {selected && (
          <div
            style={{
              flex: 1,
              borderLeft: `1px solid ${C.border}`,
              overflowY: "auto",
              padding: 20,
              background: C.bg,
            }}
          >
            <GraphPanel key={selected.id} scenario={selected} />
          </div>
        )}
      </div>
    </div>
  );
}
