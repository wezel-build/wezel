import {
  createContext,
  useContext,
  useEffect,
  useState,
  useCallback,
  type ReactNode,
} from "react";
import { authApi, type AuthUser } from "./api";

interface AuthState {
  user: AuthUser | null;
  loading: boolean;
  forbidden: boolean;
  authRequired: boolean;
  setupRequired: boolean;
  appSlug: string | null;
  githubHost: string | null;
  logout: () => Promise<void>;
}

const AuthContext = createContext<AuthState>({
  user: null,
  loading: true,
  forbidden: false,
  authRequired: true,
  setupRequired: false,
  appSlug: null,
  githubHost: null,
  logout: async () => {},
});

export function AuthProvider({ children }: { children: ReactNode }) {
  const [user, setUser] = useState<AuthUser | null>(null);
  const [loading, setLoading] = useState(true);
  const [forbidden] = useState(() => {
    const params = new URLSearchParams(window.location.search);
    if (params.get("error") === "forbidden") {
      window.history.replaceState({}, "", window.location.pathname);
      return true;
    }
    return false;
  });
  const [authRequired, setAuthRequired] = useState(true);
  const [setupRequired, setSetupRequired] = useState(false);
  const [appSlug, setAppSlug] = useState<string | null>(null);
  const [githubHost, setGithubHost] = useState<string | null>(null);

  useEffect(() => {
    Promise.all([
      authApi.me().catch(() => null),
      authApi
        .config()
        .catch(() => ({ auth_required: true, setup_required: false })),
    ]).then(([user, cfg]) => {
      setUser(user);
      setAuthRequired(cfg.auth_required);
      setSetupRequired(cfg.setup_required ?? false);
      setAppSlug((cfg as { app_slug?: string }).app_slug ?? null);
      setGithubHost((cfg as { github_host?: string }).github_host ?? null);
      setLoading(false);
    });
  }, []);

  const logout = useCallback(async () => {
    await authApi.logout();
    setUser(null);
  }, []);

  return (
    <AuthContext.Provider
      value={{
        user,
        loading,
        forbidden,
        authRequired,
        setupRequired,
        appSlug,
        githubHost,
        logout,
      }}
    >
      {children}
    </AuthContext.Provider>
  );
}

// eslint-disable-next-line react-refresh/only-export-components
export function useAuth() {
  return useContext(AuthContext);
}
