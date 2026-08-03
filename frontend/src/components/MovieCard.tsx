import { useTranslation } from "react-i18next";
import type { MovieView } from "../types";
import { formatShowing } from "../format";

export function MovieCard({ movie }: { movie: MovieView }) {
  useTranslation();
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
          <span className="when">{formatShowing(s.start)}</span>
          {s.detail && <span className="detail">{s.detail}</span>}
        </a>
      ))}
    </div>
  );
}
