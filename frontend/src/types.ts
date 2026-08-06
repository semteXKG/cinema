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
  apple: boolean;
  github: boolean;
}
