import type { Scenario, ForagerCommit, Project } from "./data";

export interface GithubCommit {
  sha: string;
  shortSha: string;
  author: string;
  message: string;
  timestamp: string;
  htmlUrl: string;
}

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
  const p = `/api/project/${projectId}`;
  return {
    overview: () => get<Overview>(`${p}/overview`),
    scenarios: () => get<ScenarioSummary[]>(`${p}/scenario`),
    scenario: (id: number) => get<Scenario>(`${p}/scenario/${id}`),
    togglePin: (id: number) => patch<Scenario>(`${p}/scenario/${id}/pin`),
    commits: () => get<ForagerCommit[]>(`${p}/commit`),
    commit: (sha: string) => get<ForagerCommit>(`${p}/commit/${sha}`),
    githubCommit: (sha: string) =>
      get<GithubCommit>(`${p}/github/commit/${sha}`),
    scheduleCommit: (sha: string) =>
      post<ForagerCommit>(`${p}/commit`, { sha }),
    users: () => get<string[]>(`${p}/user`),
  };
}

export type ProjectApi = ReturnType<typeof projectApi>;

export const api = {
  projects: () => get<Project[]>("/api/project"),
  createProject: (name: string, upstream: string) =>
    post<Project>("/api/project", { name, upstream }),
  forProject: projectApi,
};
