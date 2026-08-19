import type { CinemaView } from "../types";
import { MovieCard } from "./MovieCard";

export function CinemaSection({ cinema }: { cinema: CinemaView }) {
  return (
    <section>
      <h2>{cinema.name}</h2>
      {cinema.movies.map((m) => (
        <MovieCard key={m.title} movie={m} cinema={cinema.name} />
      ))}
    </section>
  );
}
