import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { useAuth } from "../hooks/useAuth";

export function LoginConfirmedPage() {
  const { t } = useTranslation();
  const { user, loading, pollLoginStatus } = useAuth();

  useEffect(() => {
    if (user || loading) return;
    let cancelled = false;
    void pollLoginStatus(undefined, 20_000, () => cancelled);
    return () => {
      cancelled = true;
    };
  }, [user, loading, pollLoginStatus]);

  return (
    <>
      <header className="marquee">
        <div className="bulbs"></div>
        <div className="marquee-brand">
          <img className="marquee-logo" src="/projector-logo.svg" alt="" />
          <div className="marquee-text">
            <h1>{t("brand")}</h1>
            <p className="tagline">{t("tagline")}</p>
          </div>
        </div>
        <div className="bulbs"></div>
      </header>
      <main>
        <p className="auth-note">{user ? t("auth.loggedIn") : t("auth.confirmed")}</p>
      </main>
    </>
  );
}
