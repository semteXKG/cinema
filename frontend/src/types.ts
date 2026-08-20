export interface ShowingRow {
  start: string;
  detail: string;
  url: string;
}

export interface MovieView {
  title: string;
  badge: string | null;
  metaLine: string;
  poster: string | null;
  showings: ShowingRow[];
  ignored: boolean;
}

export interface CinemaView {
  name: string;
  movies: MovieView[];
}

export interface ApiPayload {
  generatedAt: string | null;
  sources: Record<string, string> | null;
  cinemas: CinemaView[] | null;
}

export interface AuthUser {
  id: number;
  email: string;
}

export interface AuthProviders {
  email: boolean;
  google: boolean;
  github: boolean;
  dev: boolean;
}

export type NotificationFrequency =
  | "never"
  | "immediately"
  | "1"
  | "2"
  | "3"
  | "4"
  | "5"
  | "6"
  | "7";

export const FREQUENCY_OPTIONS: NotificationFrequency[] = [
  "never", "immediately", "1", "2", "3", "4", "5", "6", "7",
];

export interface NotificationPreferences {
  telegramHandle: string;
  telegramVerified: boolean;
  digestAnchor: string;
  digestHour: number;
}

export const FEATURES = ["OV", "OmU", "OmdU", "2D", "3D", "IMAX", "Atmos", "DolbyCinema", "4DX"] as const;
export type Feature = (typeof FEATURES)[number];

export interface Cinema { id: number; name: string; }

export type NotificationChannel = "email" | "telegram" | "both";

export interface NotificationRule {
  id?: number;
  position: number;
  cinemaId: number | null;
  cinemaName?: string | null;
  features: string[];
  titleSubstring: string | null;
  frequency: NotificationFrequency;
  channel: NotificationChannel;
}

export interface RulesResponse { rules: NotificationRule[]; cinemas: Cinema[]; }
