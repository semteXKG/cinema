import type { ApiPayload, AuthUser, AuthProviders } from "./types";

export async function fetchShowings(): Promise<ApiPayload> {
  const resp = await fetch("/api/showings");
  if (!resp.ok) {
    throw new Error(`GET /api/showings failed: ${resp.status}`);
  }
  return (await resp.json()) as ApiPayload;
}

export async function fetchMe(): Promise<AuthUser> {
  const resp = await fetch("/api/auth/me");
  if (!resp.ok) throw new Error("not authenticated");
  return (await resp.json()) as AuthUser;
}

export async function fetchProviders(): Promise<AuthProviders> {
  const resp = await fetch("/api/auth/providers");
  if (!resp.ok) throw new Error("providers fetch failed");
  return (await resp.json()) as AuthProviders;
}

export async function sendMagicLink(email: string): Promise<void> {
  await fetch("/api/auth/email", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ email }),
  });
}

export async function fetchLoginStatus(): Promise<boolean> {
  const resp = await fetch("/api/auth/login/status");
  if (!resp.ok) throw new Error("login status failed");
  const data = (await resp.json()) as { loggedIn: boolean };
  return data.loggedIn;
}

export async function logout(): Promise<void> {
  await fetch("/api/auth/logout", { method: "POST" });
}
