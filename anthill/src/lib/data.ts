// ── Project ──────────────────────────────────────────────────────────────────

export interface Project {
  id: number;
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

export interface Scenario {
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
  prevValue?: number;
}

export interface Measurement {
  id: number;
  name: string;
  kind: string;
  status: MeasurementStatus;
  value?: number;
  prevValue?: number;
  unit?: string;
  detail?: MeasurementDetail[];
}

export interface ForagerCommit {
  sha: string;
  shortSha: string;
  author: string;
  message: string;
  timestamp: string;
  status: "not-started" | "running" | "complete";
  measurements: Measurement[];
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
