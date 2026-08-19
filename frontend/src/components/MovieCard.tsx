import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import type { MovieView } from "../types";
import { formatShowing } from "../format";
import { useAuth } from "../hooks/useAuth";
import { setIgnored, unsetIgnored } from "../api";

interface MovieCardProps {
  movie: MovieView;
  cinema: string;
}

export function MovieCard({ movie, cinema }: MovieCardProps) {
  useTranslation();
  const { user } = useAuth();
  const [ignored, setIgnoredState] = useState(movie.ignored);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setIgnoredState(movie.ignored);
  }, [movie.ignored]);

  const handleToggle = async () => {
    setError(null);
    if (ignored) {
      try {
        await unsetIgnored(cinema, movie.title);
        setIgnoredState(false);
      } catch {
        setError("Failed to unignore");
      }
    } else {
      try {
        await setIgnored(cinema, movie.title);
        setIgnoredState(true);
      } catch {
        setError("Failed to ignore");
      }
    }
  };

  if (!user) {
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

  if (ignored) {
    return (
      <div className="card ignored-card">
        <div className="ignored-row">
          <span className="ignored-title">{movie.title}</span>
          <span className="ignored-label"> · Ignored</span>
          <button className="ignore-btn" onClick={handleToggle} title="Unignore">
            👁
          </button>
        </div>
        {error && <div className="ignore-error">{error}</div>}
      </div>
    );
  }

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
      <button className="ignore-btn" onClick={handleToggle} title="Ignore">
        ✕
      </button>
      {error && <div className="ignore-error">{error}</div>}
    </div>
  );
}
