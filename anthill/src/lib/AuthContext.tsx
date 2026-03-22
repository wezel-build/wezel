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
  logout: () => Promise<void>;
}

const AuthContext = createContext<AuthState>({
  user: null,
  loading: true,
  forbidden: false,
  authRequired: true,
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

  useEffect(() => {
    Promise.all([
      authApi.me().catch(() => null),
      authApi.config().catch(() => ({ auth_required: true })),
    ]).then(([user, cfg]) => {
      setUser(user);
      setAuthRequired(cfg.auth_required);
      setLoading(false);
    });
  }, []);

  const logout = useCallback(async () => {
    await authApi.logout();
    setUser(null);
  }, []);

  return (
    <AuthContext.Provider
      value={{ user, loading, forbidden, authRequired, logout }}
    >
      {children}
    </AuthContext.Provider>
  );
}

// eslint-disable-next-line react-refresh/only-export-components
export function useAuth() {
  return useContext(AuthContext);
}
