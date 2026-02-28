import { useState, useEffect, useMemo, useCallback } from "react";
import type { ReactNode } from "react";
import type { Project } from "./data";
import { api } from "./api";
import { ProjectCtx, nullApi } from "./projectCtx";

const STORAGE_KEY = "wezel:projectId";

export function ProjectProvider({ children }: { children: ReactNode }) {
  const [projects, setProjects] = useState<Project[]>([]);
  const [current, setCurrentRaw] = useState<Project | null>(null);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    api.projects().then((list) => {
      setProjects(list);
      const stored = localStorage.getItem(STORAGE_KEY);
      const match = stored
        ? list.find((p) => String(p.id) === stored)
        : undefined;
      setCurrentRaw(match ?? list[0] ?? null);
      setLoaded(true);
    });
  }, []);

  const setCurrent = (p: Project) => {
    localStorage.setItem(STORAGE_KEY, String(p.id));
    setCurrentRaw(p);
  };

  const addProject = useCallback(
    async (name: string, upstream: string): Promise<Project> => {
      const created = await api.createProject(name, upstream);
      setProjects((prev) => [...prev, created]);
      setCurrent(created);
      return created;
    },
    [],
  );

  const pApi = useMemo(
    () => (current ? api.forProject(current.id) : nullApi),
    [current],
  );

  return (
    <ProjectCtx.Provider
      value={{ projects, current, setCurrent, addProject, loaded, pApi }}
    >
      {children}
    </ProjectCtx.Provider>
  );
}
