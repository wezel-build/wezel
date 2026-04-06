// ── Repo ─────────────────────────────────────────────────────────────────────

export interface Repo {
  id: number;
  upstream: string;
  webhookRegistered: boolean;
  projectCount: number;
}

export interface WebhookSetup {
  id: number;
  upstream: string;
  webhookSecret: string;
  webhookUrl: string;
  /** Whether the webhook was auto-registered on GitHub. */
  registered: boolean;
}

// ── Project ──────────────────────────────────────────────────────────────────

export interface Project {
  id: number;
  repo_id: number;
  name: string;
  upstream: string;
}

// ── Data model ───────────────────────────────────────────────────────────────

export interface CrateTopo {
  name: string;
  version?: string;
  deps: string[];
  buildDeps?: string[];
  devDeps?: string[];
  external?: boolean;
}

export interface Run {
  user: string;
  platform: string;
  timestamp: string;
  commit: string;
  buildTimeMs: number;
  dirtyCrates: string[];
}

export interface Observation {
  id: number;
  name: string;
  profile: "dev" | "release";
  platform?: string;
  pinned: boolean;
  graph: CrateTopo[];
  runs: Run[];
}

// ── Forager commit model ─────────────────────────────────────────────────────

export type MeasurementStatus =
  | "not-started"
  | "pending"
  | "running"
  | "complete"
  | "failed";

export interface MeasurementDetail {
  name: string;
  value: number;
}

export interface Measurement {
  id: number;
  name: string;
  status: MeasurementStatus;
  value?: number;
  unit?: string;
  tags?: Record<string, string>;
  detail?: MeasurementDetail[];
}

export interface ForagerCommit {
  sha: string;
  shortSha: string;
  author: string;
  message: string;
  timestamp: string;
  measurements: Measurement[];
}

// ── Branch timeline ─────────────────────────────────────────────────────────

export interface BranchTimeline {
  branch: string;
  commits: ForagerCommit[];
}

// ── Compare ─────────────────────────────────────────────────────────────────

export interface CompareResponse {
  base: ForagerCommit;
  head: ForagerCommit;
}

// ── Bisections ──────────────────────────────────────────────────────────────

export type BisectionStatus = "active" | "complete" | "abandoned";

export interface Bisection {
  id: number;
  projectId: number;
  experimentName: string;
  measurementName: string;
  branch: string;
  goodSha: string;
  badSha: string;
  goodValue: number;
  badValue: number;
  status: BisectionStatus;
  culpritSha?: string;
  identityTags?: Record<string, string>;
}

// ── Pheromone registry ───────────────────────────────────────────────────────

export interface PheromoneField {
  name: string;
  type: string;
  description?: string;
  deprecated?: boolean;
  deprecatedIn?: string;
  replacedBy?: string;
}

export type VizSpec =
  | { type: "stat"; field: string; label: string }
  | { type: "vega-lite"; spec: Record<string, unknown> };

export interface VizForKind {
  summary?: VizSpec;
  detail?: VizSpec;
}

/** Keys are step names or tool names. */
export type VizConfig = Record<string, VizForKind>;

/** Build a name → VizForKind lookup from the full pheromone list. */
export function buildVizMap(
  pheromones: Pheromone[],
): Record<string, VizForKind> {
  const map: Record<string, VizForKind> = {};
  for (const p of pheromones) {
    if (!p.vizJson) continue;
    for (const [name, cfg] of Object.entries(p.vizJson)) {
      map[name] = cfg;
    }
  }
  return map;
}

export interface Pheromone {
  id: number;
  name: string;
  githubRepo: string;
  version: string;
  platforms: string[];
  fields: PheromoneField[];
  fetchedAt: string;
  vizJson?: VizConfig;
}

// ── Registry adapter types ────────────────────────────────────────────────────

export interface RegistryUiField {
  id: string;
  label: string;
  type: "crate-picker" | "select" | "string";
  description?: string;
  options?: string[];
  default?: string;
}

export interface RegistryStep {
  name: string;
  tool: string;
  inputs: Record<string, unknown>;
}

export interface RegistryTemplate {
  id: string;
  name: string;
  description: string;
  steps: RegistryStep[];
  uiSchema: { fields: RegistryUiField[] };
}

export interface RegistryAdapter {
  toolchain: string;
  detectPatterns: string[];
  templates: RegistryTemplate[];
}

// ── Measurement identity ─────────────────────────────────────────────────────

/** Stable identity key for a measurement: name + sorted identity tags. */
export function measurementKey(m: Measurement): string {
  const tags = m.tags ?? {};
  const sorted = Object.entries(tags).sort(([a], [b]) => a.localeCompare(b));
  if (sorted.length === 0) return m.name;
  return m.name + "|" + sorted.map(([k, v]) => `${k}=${v}`).join(",");
}

// ── Heat computation ─────────────────────────────────────────────────────────

export function computeHeat(
  runs: Run[],
  crateNames: string[],
): Record<string, number> {
  if (runs.length === 0) {
    return Object.fromEntries(crateNames.map((n) => [n, 0]));
  }
  const counts: Record<string, number> = {};
  for (const name of crateNames) counts[name] = 0;
  for (const run of runs) {
    for (const c of run.dirtyCrates) {
      if (c in counts) counts[c]++;
    }
  }
  const result: Record<string, number> = {};
  for (const name of crateNames) {
    result[name] = Math.round((counts[name] / runs.length) * 100);
  }
  return result;
}
