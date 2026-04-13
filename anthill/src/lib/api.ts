import type {
  Observation,
  ForagerCommit,
  Project,
  Pheromone,
  BranchTimeline,
  CompareResponse,
  Bisection,
  Repo,
} from "./data";

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
  if (!res.ok) {
    const text = await res.text().catch(() => res.statusText);
    throw new Error(text || `${res.status} ${res.statusText}`);
  }
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

export interface AuthConfig {
  auth_required: boolean;
  setup_required: boolean;
  github_host?: string;
  app_slug?: string;
}

export const authApi = {
  me: (): Promise<AuthUser> => get<AuthUser>(`${BASE}/auth/me`),
  config: (): Promise<AuthConfig> => get<AuthConfig>(`${BASE}/auth/config`),
  logout: (): Promise<void> =>
    fetch(`${BASE}/auth/logout`, {
      method: "POST",
      credentials: "include",
    }).then(() => undefined),
  loginUrl: `${BASE}/auth/github`,
};

export interface ManifestResponse {
  manifest: unknown;
  post_url: string;
  github_host: string;
}

export const setupApi = {
  getManifest: (
    githubHost: string,
    publicUrl: string,
  ): Promise<ManifestResponse> =>
    post<ManifestResponse>(`${BASE}/api/setup/github-app/manifest`, {
      github_host: githubHost,
      public_url: publicUrl,
    }),
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
    experiments: () => get<string[]>(`${p}/benchmarks`),
    branchTimeline: (branch: string, limit?: number) => {
      const q = limit ? `?limit=${limit}` : "";
      return get<BranchTimeline>(
        `${p}/branch/${encodeURIComponent(branch)}/timeline${q}`,
      );
    },
    compare: (baseSha: string, headSha: string) =>
      get<CompareResponse>(
        `${p}/compare?base_sha=${encodeURIComponent(baseSha)}&head_sha=${encodeURIComponent(headSha)}`,
      ),
    bisections: (status?: string, branch?: string) => {
      const params = new URLSearchParams();
      if (status) params.set("status", status);
      if (branch) params.set("branch", branch);
      const qs = params.toString();
      return get<Bisection[]>(`${p}/bisections${qs ? `?${qs}` : ""}`);
    },
    bisection: (id: number) => get<Bisection>(`${p}/bisections/${id}`),
    abandonBisection: (id: number) =>
      patch<Bisection>(`${p}/bisections/${id}`, { status: "abandoned" }),
  };
}

export type ProjectApi = ReturnType<typeof projectApi>;

export interface ExperimentPrResponse {
  prUrl: string;
}

export interface GithubRepoEntry {
  full_name: string;
  html_url: string;
  private: boolean;
}

export const api = {
  repos: () => get<Repo[]>("/api/repo"),
  githubRepos: () => get<GithubRepoEntry[]>("/api/github/repos"),
  projects: () => get<Project[]>("/api/project"),
  forProject: projectApi,
  pheromones: () => get<Pheromone[]>("/api/pheromones"),
  enqueueForagerJob: (
    projectUpstream: string,
    commitSha: string,
    experimentName: string,
  ) =>
    post<ForagerJobStatus>("/api/forager/jobs", {
      project_upstream: projectUpstream,
      commit_sha: commitSha,
      benchmark_name: experimentName,
    }),
  admin: {
    pheromones: () => get<Pheromone[]>("/api/admin/pheromone"),
    registerPheromone: (githubRepo: string) =>
      post<Pheromone>("/api/admin/pheromone", { github_repo: githubRepo }),
    fetchPheromone: (name: string) =>
      post<Pheromone>(`/api/admin/pheromone/${name}/fetch`, {}),
  },
};

export function experimentPrApi(projectId: number) {
  return {
    createPr: (
      experimentName: string,
      files: Record<string, string>,
    ): Promise<ExperimentPrResponse> =>
      post<ExperimentPrResponse>(`/api/project/${projectId}/experiment/pr`, {
        experimentName,
        files,
      }),
  };
}
