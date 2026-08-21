import i18n from "./i18n";
import type { TFunction } from "i18next";
import type { NotificationFrequency } from "./types";

function locale() {
  return i18n.language;
}

export function formatShowing(startIso: string): string {
  const d = new Date(startIso);
  if (Number.isNaN(d.getTime())) return startIso;
  const date = new Intl.DateTimeFormat(locale(), {
    weekday: "short",
    day: "2-digit",
    month: "2-digit",
  }).format(d);
  const time = new Intl.DateTimeFormat(locale(), {
    hour: "2-digit",
    minute: "2-digit",
  }).format(d);
  return `${date} · ${time}`;
}

export function formatGeneratedAt(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return new Intl.DateTimeFormat(locale(), {
    dateStyle: "short",
    timeStyle: "short",
  }).format(d);
}

export function frequencyLabel(t: TFunction, value: NotificationFrequency): string {
  if (value === "never") return t("preferences.frequencies.never");
  if (value === "immediately") return t("preferences.frequencies.immediately");
  return t("preferences.frequencies.days", { count: Number(value) });
}
