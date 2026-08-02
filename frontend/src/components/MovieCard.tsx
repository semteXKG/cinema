import type { MovieView } from "../types";

export function MovieCard({ movie }: { movie: MovieView }) {
  return (
    <div className="card">
      <div className="filmrow">
        {movie.poster && <img src={`/posters/${movie.poster}`} alt="" loading="lazy" />}
        <div className="filmtitle">
          <strong>{movie.title}</strong>
          {movie.badge && <span className="badge">{movie.badge}</span>}
          {movie.metaLine && <div className="filmmeta">{movie.metaLine}</div>}
        </div>
      </div>
      {movie.showings.map((s, i) => (
        <a className="showing" href={s.url} key={i}>
          <span className="when">
            {s.date} · {s.time}
          </span>
          {s.detail && <span className="detail">{s.detail}</span>}
        </a>
      ))}
    </div>
  );
}
