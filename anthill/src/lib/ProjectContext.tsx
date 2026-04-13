import { useState, useEffect, useMemo } from "react";
import type { ReactNode } from "react";
import type { Project } from "./data";
import { api } from "./api";
import { ProjectCtx, nullApi } from "./projectCtx";

const STORAGE_KEY = "wezel:projectId";

export function ProjectProvider({ children }: { children: ReactNode }) {
  const [projects, setProjects] = useState<Project[]>([]);
  const [current, setCurrentRaw] = useState<Project | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api
      .projects()
      .then((list) => {
        setProjects(list);
        const stored = localStorage.getItem(STORAGE_KEY);
        const match = stored
          ? list.find((p) => String(p.id) === stored)
          : undefined;
        setCurrentRaw(match ?? list[0] ?? null);
        setLoaded(true);
      })
      .catch((e) => {
        console.error("Failed to load projects:", e);
        setError(String(e));
      });
  }, []);

  const setCurrent = (p: Project) => {
    localStorage.setItem(STORAGE_KEY, String(p.id));
    setCurrentRaw(p);
  };

  const pApi = useMemo(
    () => (current ? api.forProject(current.id) : nullApi),
    [current],
  );

  if (error) {
    return (
      <div style={{ color: "red", padding: 16 }}>
        Failed to load projects: {error}
      </div>
    );
  }

  return (
    <ProjectCtx.Provider
      value={{
        projects,
        current,
        setCurrent,
        loaded,
        pApi,
      }}
    >
      {children}
    </ProjectCtx.Provider>
  );
}
