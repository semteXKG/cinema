import type { NotificationPreferences } from "../types";

export async function fetchPreferences(): Promise<NotificationPreferences> {
  const res = await fetch("/api/preferences", { credentials: "include" });
  if (!res.ok) throw new Error("failed to load preferences");
  return res.json();
}

export async function savePreferences(
  prefs: Partial<NotificationPreferences>
): Promise<NotificationPreferences> {
  const res = await fetch("/api/preferences", {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    credentials: "include",
    body: JSON.stringify(prefs),
  });
  if (!res.ok) throw new Error("failed to save preferences");
  return res.json();
}
