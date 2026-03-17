import type { Observation, ForagerCommit, Project, Pheromone } from "./data";

export interface GithubCommit {
  sha: string;
  shortSha: string;
  author: string;
  message: string;
  timestamp: string;
  htmlUrl: string;
}

export interface Overview {
  observationCount: number;
  trackedCount: number;
  latestCommitShortSha: string | null;
  latestCommitStatus: string | null;
}

/** Observation as returned by the list endpoint (no graph). */
export type ObservationSummary = Omit<Observation, "graph">;

const BASE = import.meta.env.VITE_BURROW_URL ?? "";

async function get<T>(path: string): Promise<T> {
  const res = await fetch(`${BASE}${path}`, { credentials: "include" });
  if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);
  return res.json();
}

async function post<T>(path: string, body: unknown): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
    credentials: "include",
  });
  if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);
  return res.json();
}

async function patch<T>(path: string, body?: unknown): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    method: "PATCH",
    headers: { "Content-Type": "application/json" },
    body: body != null ? JSON.stringify(body) : undefined,
    credentials: "include",
  });
  if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);
  return res.json();
}

export interface AuthUser {
  login: string;
}

export const authApi = {
  me: (): Promise<AuthUser> => get<AuthUser>(`${BASE}/auth/me`),
  config: (): Promise<{ auth_required: boolean }> =>
    get<{ auth_required: boolean }>(`${BASE}/auth/config`),
  logout: (): Promise<void> =>
    fetch(`${BASE}/auth/logout`, {
      method: "POST",
      credentials: "include",
    }).then(() => undefined),
  loginUrl: `${BASE}/auth/github`,
};

export interface ForagerJobStatus {
  id: number;
  status: string;
}

function projectApi(projectId: number) {
  const p = `/api/project/${projectId}`;
  return {
    overview: () => get<Overview>(`${p}/overview`),
    observations: () => get<ObservationSummary[]>(`${p}/observation`),
    observation: (id: number) => get<Observation>(`${p}/observation/${id}`),
    togglePin: (id: number) => patch<Observation>(`${p}/observation/${id}/pin`),
    commits: () => get<ForagerCommit[]>(`${p}/commit`),
    commit: (sha: string) => get<ForagerCommit>(`${p}/commit/${sha}`),
    githubCommit: (sha: string) =>
      get<GithubCommit>(`${p}/github/commit/${sha}`),
    scheduleCommit: (sha: string) =>
      post<ForagerCommit>(`${p}/commit`, { sha }),
    users: () => get<string[]>(`${p}/user`),
    benchmarks: () => get<string[]>(`${p}/benchmarks`),
  };
}

export type ProjectApi = ReturnType<typeof projectApi>;

export interface BenchmarkPrResponse {
  prUrl: string;
}

export const api = {
  projects: () => get<Project[]>("/api/project"),
  createProject: (name: string, upstream: string) =>
    post<Project>("/api/project", { name, upstream }),
  renameProject: (id: number, name: string) =>
    patch<Project>(`/api/project/${id}`, { name }),
  forProject: projectApi,
  pheromones: () => get<Pheromone[]>("/api/pheromones"),
  enqueueForagerJob: (
    projectUpstream: string,
    commitSha: string,
    benchmarkName: string,
  ) =>
    post<ForagerJobStatus>("/api/forager/jobs", {
      project_upstream: projectUpstream,
      commit_sha: commitSha,
      benchmark_name: benchmarkName,
    }),
  admin: {
    pheromones: () => get<Pheromone[]>("/api/admin/pheromone"),
    registerPheromone: (githubRepo: string) =>
      post<Pheromone>("/api/admin/pheromone", { github_repo: githubRepo }),
    fetchPheromone: (name: string) =>
      post<Pheromone>(`/api/admin/pheromone/${name}/fetch`, {}),
  },
};

export function benchmarkPrApi(projectId: number) {
  return {
    createPr: (
      benchmarkName: string,
      files: Record<string, string>,
    ): Promise<BenchmarkPrResponse> =>
      post<BenchmarkPrResponse>(
        `/api/project/${projectId}/benchmark/pr`,
        { benchmarkName, files },
      ),
  };
}
