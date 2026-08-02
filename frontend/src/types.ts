export interface ShowingRow {
  date: string;
  time: string;
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
