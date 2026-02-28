import { useState, useEffect, useCallback, useRef } from "react";
import { api } from "./api";
import type { Scenario, ForagerCommit } from "./data";

const EMPTY_SCENARIOS: Scenario[] = [];
const EMPTY_COMMITS: ForagerCommit[] = [];
const EMPTY_USERS: string[] = [];

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

  const refetch = useCallback(() => {
    setState((s) => ({ ...s, loading: true, error: null }));
    fetcherRef
      .current()
      .then((data) => setState({ data, loading: false, error: null }))
      .catch((e) => setState({ data: null, loading: false, error: String(e) }));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    refetch();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, refetch]);

  return { ...state, refetch };
}

export function useScenarios() {
  const { data, loading, error, refetch } = useAsync(() => api.scenarios(), []);

  const togglePin = useCallback(
    async (id: number) => {
      await api.togglePin(id);
      refetch();
    },
    [refetch],
  );

  return { scenarios: data ?? EMPTY_SCENARIOS, loading, error, togglePin };
}

export function useCommits() {
  const result = useAsync(() => api.commits(), []);
  return {
    commits: result.data ?? EMPTY_COMMITS,
    loading: result.loading,
    error: result.error,
  };
}

export function useCommit(sha: string | undefined) {
  return useAsync(
    () => (sha ? api.commit(sha) : Promise.reject("no sha")),
    [sha],
  );
}

export function useUsers() {
  const result = useAsync(() => api.users(), []);
  return {
    users: result.data ?? EMPTY_USERS,
    loading: result.loading,
    error: result.error,
  };
}
