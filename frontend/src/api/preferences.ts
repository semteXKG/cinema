import type { NotificationPreferences, RulesResponse, NotificationRule } from "../types";

export async function fetchPreferences(): Promise<NotificationPreferences> {
  const res = await fetch("/api/preferences", { credentials: "include" });
  if (!res.ok) throw new Error("failed to load preferences");
  return res.json();
}

export async function savePreferences(prefs: Partial<NotificationPreferences>): Promise<NotificationPreferences> {
  const res = await fetch("/api/preferences", {
    method: "PUT", headers: { "Content-Type": "application/json" },
    credentials: "include", body: JSON.stringify(prefs),
  });
  if (!res.ok) throw new Error("failed to save preferences");
  return res.json();
}

export async function fetchRules(): Promise<RulesResponse> {
  const res = await fetch("/api/preferences/rules", { credentials: "include" });
  if (!res.ok) throw new Error("failed to load rules");
  return res.json();
}

export async function saveRules(rules: NotificationRule[]): Promise<RulesResponse> {
  const res = await fetch("/api/preferences/rules", {
    method: "PUT", headers: { "Content-Type": "application/json" },
    credentials: "include",
    body: JSON.stringify({ rules: rules.map((r) => ({ cinemaId: r.cinemaId, features: r.features, titleSubstring: r.titleSubstring, frequency: r.frequency })) }),
  });
  if (!res.ok) throw new Error("failed to save rules");
  return res.json();
}