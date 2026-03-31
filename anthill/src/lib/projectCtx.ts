import { createContext } from "react";
import type { ProjectApi } from "./api";
import type { Project } from "./data";

export interface ProjectCtxValue {
  projects: Project[];
  current: Project | null;
  loaded: boolean;
  setCurrent: (p: Project) => void;
  addProject: (name: string, upstream: string) => Promise<Project>;
  renameProject: (id: number, name: string) => Promise<Project>;
  pApi: ProjectApi;
}

/** Fallback api that always rejects — used before a project is selected. */
export const nullApi: ProjectApi = {
  overview: () => Promise.reject("no project"),
  observations: () => Promise.reject("no project"),
  observation: () => Promise.reject("no project"),
  togglePin: () => Promise.reject("no project"),
  commits: () => Promise.reject("no project"),
  commit: () => Promise.reject("no project"),
  githubCommit: () => Promise.reject("no project"),
  scheduleCommit: () => Promise.reject("no project"),
  users: () => Promise.reject("no project"),
  benchmarks: () => Promise.reject("no project"),
  branchTimeline: () => Promise.reject("no project"),
  compare: () => Promise.reject("no project"),
  bisections: () => Promise.reject("no project"),
  bisection: () => Promise.reject("no project"),
  abandonBisection: () => Promise.reject("no project"),
};

export const ProjectCtx = createContext<ProjectCtxValue>({
  projects: [],
  current: null,
  loaded: false,
  setCurrent: () => {},
  addProject: () => Promise.reject("no provider"),
  renameProject: () => Promise.reject("no provider"),
  pApi: nullApi,
});
