import { useEffect, useState } from "react";
import { fetchShowings } from "./api";
import type { ApiPayload } from "./types";
import { Marquee } from "./components/Marquee";
import { Sidebar } from "./components/Sidebar";
import { CinemaSection } from "./components/CinemaSection";

const POLL_MS = 15 * 60 * 1000; // mirrors the old <meta refresh=900>

export default function App() {
  const [payload, setPayload] = useState<ApiPayload | null>(null);

  useEffect(() => {
    let alive = true;
    const load = () =>
      fetchShowings()
        .then((p) => alive && setPayload(p))
        .catch(() => {});
    load();
    const id = setInterval(load, POLL_MS);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, []);

  return (
    <>
      <Marquee />
      <div className="layout">
        <Sidebar />
        <main>
          {payload === null || payload.cinemas === null ? (
            <p className="empty">No data yet — the first check is running.</p>
          ) : payload.cinemas.length === 0 ? (
            <p className="empty">No OV showings found right now.</p>
          ) : (
            payload.cinemas.map((c) => <CinemaSection key={c.name} cinema={c} />)
          )}
        </main>
      </div>
      {payload?.generatedAt && (
        <p className="meta">
          Last checked: {payload.generatedAt}
          {payload.sources && (
            <>
              {" · "}Cineplexx:{" "}
              <span className={payload.sources.cineplexx === "ok" ? "ok" : "err"}>
                {payload.sources.cineplexx ?? "–"}
              </span>
              {" · "}Megaplex:{" "}
              <span className={payload.sources.megaplex === "ok" ? "ok" : "err"}>
                {payload.sources.megaplex ?? "–"}
              </span>
            </>
          )}
        </p>
      )}
    </>
  );
}
