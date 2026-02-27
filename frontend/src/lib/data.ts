import usersData from "../mock_data/users.json";
import scenariosData from "../mock_data/scenarios.json";
import commitsData from "../mock_data/commits.json";
import graph1 from "../mock_data/graphs/1.json";
import graph2 from "../mock_data/graphs/2.json";
import graph3 from "../mock_data/graphs/3.json";
import graph4 from "../mock_data/graphs/4.json";
import graph5 from "../mock_data/graphs/5.json";
import graph6 from "../mock_data/graphs/6.json";
import graph7 from "../mock_data/graphs/7.json";
import graph8 from "../mock_data/graphs/8.json";
import runs1 from "../mock_data/runs/1.json";
import runs2 from "../mock_data/runs/2.json";
import runs3 from "../mock_data/runs/3.json";
import runs4 from "../mock_data/runs/4.json";
import runs5 from "../mock_data/runs/5.json";
import runs6 from "../mock_data/runs/6.json";
import runs7 from "../mock_data/runs/7.json";
import runs8 from "../mock_data/runs/8.json";

// ── Data model ───────────────────────────────────────────────────────────────

export interface CrateTopo {
  name: string;
  deps: string[];
}

export interface Run {
  user: string;
  timestamp: string;
  commit: string;
  buildTimeMs: number;
  dirtyCrates: string[];
}

export interface Scenario {
  id: number;
  name: string;
  profile: "dev" | "release";
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

// ── Mock data ────────────────────────────────────────────────────────────────

export const USERS: string[] = usersData;

const graphsById: Record<number, CrateTopo[]> = {
  1: graph1,
  2: graph2,
  3: graph3,
  4: graph4,
  5: graph5,
  6: graph6,
  7: graph7,
  8: graph8,
};
const runsById: Record<number, Run[]> = {
  1: runs1 as Run[],
  2: runs2 as Run[],
  3: runs3 as Run[],
  4: runs4 as Run[],
  5: runs5 as Run[],
  6: runs6 as Run[],
  7: runs7 as Run[],
  8: runs8 as Run[],
};

export const MOCK_SCENARIOS: Scenario[] = (
  scenariosData as {
    id: number;
    name: string;
    profile: "dev" | "release";
    pinned: boolean;
  }[]
).map((s) => ({
  ...s,
  graph: graphsById[s.id] ?? [],
  runs: runsById[s.id] ?? [],
}));

export const MOCK_COMMITS = commitsData as ForagerCommit[];
