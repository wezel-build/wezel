import type { Scenario, ForagerCommit } from "./data";

const BASE = import.meta.env.VITE_BURROW_URL ?? "http://localhost:3001";

async function get<T>(path: string): Promise<T> {
  const res = await fetch(`${BASE}${path}`);
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

export const api = {
  scenarios: () => get<Scenario[]>("/api/scenarios"),
  scenario: (id: number) => get<Scenario>(`/api/scenarios/${id}`),
  togglePin: (id: number) => patch<Scenario>(`/api/scenarios/${id}/pin`),
  commits: () => get<ForagerCommit[]>("/api/commits"),
  commit: (sha: string) => get<ForagerCommit>(`/api/commits/${sha}`),
  users: () => get<string[]>("/api/users"),
};