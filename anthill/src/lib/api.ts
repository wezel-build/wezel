import type { Scenario, ForagerCommit, Project } from "./data";

export interface Overview {
  scenarioCount: number;
  trackedCount: number;
  latestCommitShortSha: string | null;
  latestCommitStatus: string | null;
}

/** Scenario as returned by the list endpoint (no graph). */
export type ScenarioSummary = Omit<Scenario, "graph">;

const BASE = import.meta.env.VITE_BURROW_URL ?? "http://localhost:3001";

async function get<T>(path: string): Promise<T> {
  const res = await fetch(`${BASE}${path}`);
  if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);
  return res.json();
}

async function post<T>(path: string, body: unknown): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);
  return res.json();
}

async function patch<T>(path: string, body?: unknown): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    method: "PATCH",
    headers: { "Content-Type": "application/json" },
    body: body != null ? JSON.stringify(body) : undefined,
  });
  if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);
  return res.json();
}

function projectApi(projectId: number) {
  const p = `/api/projects/${projectId}`;
  return {
    overview: () => get<Overview>(`${p}/overview`),
    scenarios: () => get<ScenarioSummary[]>(`${p}/scenarios`),
    scenario: (id: number) => get<Scenario>(`${p}/scenarios/${id}`),
    togglePin: (id: number) => patch<Scenario>(`${p}/scenarios/${id}/pin`),
    commits: () => get<ForagerCommit[]>(`${p}/commits`),
    commit: (sha: string) => get<ForagerCommit>(`${p}/commits/${sha}`),
    users: () => get<string[]>(`${p}/users`),
  };
}

export type ProjectApi = ReturnType<typeof projectApi>;

export const api = {
  projects: () => get<Project[]>("/api/projects"),
  createProject: (name: string, upstream: string) =>
    post<Project>("/api/projects", { name, upstream }),
  forProject: projectApi,
};
