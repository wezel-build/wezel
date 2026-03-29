import { useState, useEffect, useCallback, useRef } from "react";
import { useProject } from "./useProject";
import { api } from "./api";
import type { Overview, ObservationSummary, GithubCommit } from "./api";
import type { Observation, ForagerCommit, Pheromone } from "./data";

const EMPTY_OBSERVATIONS: ObservationSummary[] = [];
const EMPTY_COMMITS: ForagerCommit[] = [];
const EMPTY_USERS: string[] = [];
const EMPTY_PHEROMONES: Pheromone[] = [];
const EMPTY_GITHUB_COMMIT: GithubCommit | null = null;
const EMPTY_OVERVIEW: Overview = {
  observationCount: 0,
  trackedCount: 0,
  latestCommitShortSha: null,
};

interface AsyncState<T> {
  data: T | null;
  loading: boolean;
  error: string | null;
}

function useAsync<T>(
  fetcher: () => Promise<T>,
  deps: unknown[] = [],
): AsyncState<T> & { refetch: () => void } {
  const [state, setState] = useState<AsyncState<T>>({
    data: null,
    loading: true,
    error: null,
  });
  const fetcherRef = useRef(fetcher);
  fetcherRef.current = fetcher;

  const doFetch = useCallback((silent: boolean) => {
    if (!silent) {
      setState((s) => ({ ...s, loading: true, error: null }));
    }
    fetcherRef
      .current()
      .then((data) => setState({ data, loading: false, error: null }))
      .catch((e) =>
        silent
          ? setState((s) => ({ ...s, loading: false, error: String(e) }))
          : setState({ data: null, loading: false, error: String(e) }),
      );
  }, []);

  const refetch = useCallback(() => doFetch(false), [doFetch]);

  useEffect(() => {
    doFetch(false);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, refetch]);

  return { ...state, refetch };
}

export function useOverview() {
  const { pApi, current } = useProject();
  const { data, loading, error } = useAsync(
    () => pApi.overview(),
    [current?.id],
  );
  return { overview: data ?? EMPTY_OVERVIEW, loading, error };
}

export function useObservations() {
  const { pApi, current } = useProject();
  const { data, loading, error, refetch } = useAsync(
    () => pApi.observations(),
    [current?.id],
  );

  const togglePin = useCallback(
    async (id: number) => {
      await pApi.togglePin(id);
      refetch();
    },
    [pApi, refetch],
  );

  return {
    observations: data ?? EMPTY_OBSERVATIONS,
    loading,
    error,
    togglePin,
  };
}

export function useObservation(id: number | null) {
  const { pApi, current } = useProject();
  const { data, loading, error } = useAsync(
    () => (id != null ? pApi.observation(id) : Promise.reject("no id")),
    [id, current?.id],
  );
  return {
    observation: data as Observation | null,
    loading: id != null && loading,
    error,
  };
}

export function useCommits() {
  const { pApi, current } = useProject();
  const result = useAsync(() => pApi.commits(), [current?.id]);
  return {
    commits: result.data ?? EMPTY_COMMITS,
    loading: result.loading,
    error: result.error,
  };
}

export function useCommit(sha: string | undefined) {
  const { pApi, current } = useProject();
  return useAsync(
    () => (sha ? pApi.commit(sha) : Promise.reject("no sha")),
    [sha, current?.id],
  );
}

export function useGithubCommit(sha: string | undefined) {
  const { pApi, current } = useProject();
  const result = useAsync(
    () => (sha ? pApi.githubCommit(sha) : Promise.reject("no sha")),
    [sha, current?.id],
  );
  return {
    githubCommit: result.data ?? EMPTY_GITHUB_COMMIT,
    loading: !!sha && result.loading,
    error: result.error,
  };
}

export function useUsers() {
  const { pApi, current } = useProject();
  const result = useAsync(() => pApi.users(), [current?.id]);
  return {
    users: result.data ?? EMPTY_USERS,
    loading: result.loading,
    error: result.error,
  };
}

export function usePheromones() {
  const result = useAsync(() => api.pheromones(), []);
  return {
    pheromones: result.data ?? EMPTY_PHEROMONES,
    loading: result.loading,
    error: result.error,
  };
}
