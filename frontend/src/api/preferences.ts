import type { NotificationPreferences, RulesResponse, NotificationRule, NotificationChannel, Cinema } from "../types";

type WireRule = Omit<NotificationRule, "channel"> & { channels: string[] };

function channelsToChannel(channels: string[]): NotificationChannel {
  const hasEmail = channels.includes("email");
  const hasTelegram = channels.includes("telegram");
  if (hasEmail && hasTelegram) return "both";
  if (hasTelegram) return "telegram";
  return "email";
}

function channelToChannels(channel: NotificationChannel): string[] {
  if (channel === "both") return ["email", "telegram"];
  return [channel];
}

export async function fetchPreferences(): Promise<NotificationPreferences> {
  const res = await fetch("/api/preferences", { credentials: "include" });
  if (!res.ok) throw new Error("failed to load preferences");
  return res.json();
}

export async function savePreferences(prefs: Pick<NotificationPreferences, "telegramHandle">): Promise<NotificationPreferences> {
  const res = await fetch("/api/preferences", {
    method: "PUT", headers: { "Content-Type": "application/json" },
    credentials: "include", body: JSON.stringify({ telegramHandle: prefs.telegramHandle }),
  });
  if (!res.ok) throw new Error("failed to save preferences");
  return res.json();
}

export async function fetchRules(): Promise<RulesResponse> {
  const res = await fetch("/api/preferences/rules", { credentials: "include" });
  if (!res.ok) throw new Error("failed to load rules");
  const data: { rules: WireRule[]; cinemas: Cinema[] } = await res.json();
  return {
    rules: data.rules.map((r) => ({ ...r, channel: channelsToChannel(r.channels) })),
    cinemas: data.cinemas,
  };
}

export async function saveRules(rules: NotificationRule[]): Promise<RulesResponse> {
  const res = await fetch("/api/preferences/rules", {
    method: "PUT", headers: { "Content-Type": "application/json" },
    credentials: "include",
    body: JSON.stringify({ rules: rules.map((r) => ({ cinemaId: r.cinemaId, features: r.features, titleSubstring: r.titleSubstring, frequency: r.frequency, channels: channelToChannels(r.channel) })) }),
  });
  if (!res.ok) throw new Error("failed to save rules");
  const data: { rules: WireRule[]; cinemas: Cinema[] } = await res.json();
  return {
    rules: data.rules.map((r) => ({ ...r, channel: channelsToChannel(r.channels) })),
    cinemas: data.cinemas,
  };
}
