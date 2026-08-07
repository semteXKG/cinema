import { createContext, useContext, useState, useEffect, useCallback, type ReactNode } from "react";
import { fetchMe, fetchProviders, sendMagicLink, fetchLoginStatus, logout as apiLogout } from "../api";
import type { AuthUser, AuthProviders } from "../types";

interface AuthState {
  user: AuthUser | null;
  loading: boolean;
  providers: AuthProviders | null;
  loginEmail: (email: string) => Promise<void>;
  pollLoginStatus: (
    sendEmail?: () => Promise<void>,
    maxMs?: number,
    isCancelled?: () => boolean,
  ) => Promise<void>;
  loginSSO: (provider: string) => void;
  logout: () => Promise<void>;
}

const AuthContext = createContext<AuthState>({
  user: null,
  loading: true,
  providers: null,
  loginEmail: async () => {},
  pollLoginStatus: async () => {},
  loginSSO: () => {},
  logout: async () => {},
});

export function AuthProvider({ children }: { children: ReactNode }) {
  const [user, setUser] = useState<AuthUser | null>(null);
  const [loading, setLoading] = useState(true);
  const [providers, setProviders] = useState<AuthProviders | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const u = await fetchMe();
      setUser(u);
    } catch {
      setUser(null);
    }
    try {
      const p = await fetchProviders();
      setProviders(p);
    } catch {
      setProviders(null);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { refresh(); }, [refresh]);

  const pollLoginStatus = useCallback(
    async (sendEmail?: () => Promise<void>, maxMs = 15 * 60 * 1000, isCancelled?: () => boolean) => {
      if (sendEmail) {
        await sendEmail();
      }
      const deadline = Date.now() + maxMs;
      while (Date.now() < deadline) {
        await new Promise((r) => setTimeout(r, 1000));
        if (isCancelled?.()) return;
        try {
          if (await fetchLoginStatus()) {
            await refresh();
            return;
          }
        } catch {
          // transient network error: keep polling
        }
      }
    },
    [refresh],
  );

  const loginEmail = useCallback(async (email: string) => {
    await pollLoginStatus(() => sendMagicLink(email));
  }, [pollLoginStatus]);

  const loginSSO = useCallback((provider: string) => {
    window.location.href = `/api/auth/sso/${provider}`;
  }, []);

  const logout = useCallback(async () => {
    await apiLogout();
    setUser(null);
  }, []);

  return (
    <AuthContext.Provider value={{ user, loading, providers, loginEmail, pollLoginStatus, loginSSO, logout }}>
      {children}
    </AuthContext.Provider>
  );
}

export function useAuth() {
  return useContext(AuthContext);
}
