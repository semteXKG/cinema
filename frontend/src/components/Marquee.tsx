import { useState, type FormEvent } from "react";
import { NavLink, useSearchParams } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { LanguageSwitcher } from "./LanguageSwitcher";
import { useAuth } from "../hooks/useAuth";

export function Marquee() {
  const { t } = useTranslation();
  const { user, loading, providers, loginEmail, loginSSO, logout } = useAuth();
  const [showLogin, setShowLogin] = useState(false);
  const [emailInput, setEmailInput] = useState("");
  const [sending, setSending] = useState(false);
  const [searchParams] = useSearchParams();
  const confirmed = searchParams.get("login") === "confirmed";

  const handleEmailSubmit = async (e: FormEvent) => {
    e.preventDefault();
    if (!emailInput.trim() || sending) return;
    setSending(true);
    try {
      await loginEmail(emailInput.trim());
    } finally {
      setSending(false);
    }
  };

  return (
    <header className="marquee">
      <div className="bulbs"></div>
      <div className="marquee-brand">
        <img
          className="marquee-logo"
          src="/projector-logo.svg"
          alt=""
        />
        <div className="marquee-text">
          <h1>{t("brand")}</h1>
          <p className="tagline">{t("tagline")}</p>
        </div>
      </div>
      <nav className="marqnav">
        <NavLink to="/">{t("nav.home")}</NavLink>
        <NavLink to="/impressum">{t("nav.impressum")}</NavLink>
        <LanguageSwitcher />
        {!loading && (
          !user ? (
            <button className="auth-btn" onClick={() => setShowLogin(!showLogin)}>
              {t("auth.signIn")}
            </button>
          ) : (
            <button className="auth-btn" onClick={logout}>
              {t("auth.signOut")}
            </button>
          )
        )}
      </nav>
      {confirmed && (
        <div className="auth-panel">
          <p className="auth-note">{t("auth.confirmed")}</p>
        </div>
      )}
      {showLogin && !user && (
        <div className="auth-panel">
          {providers?.email && (
            <form onSubmit={handleEmailSubmit}>
              <input
                className="auth-input"
                type="email"
                placeholder={t("auth.emailPlaceholder")}
                value={emailInput}
                onChange={(e) => setEmailInput(e.target.value)}
                disabled={sending}
              />
              <button className="auth-submit" type="submit" disabled={sending}>
                {sending ? t("auth.waiting") : t("auth.sendLink")}
              </button>
            </form>
          )}
          {providers?.google && (
            <button className="auth-sso" onClick={() => loginSSO("google")}>
              {t("auth.signInWith", { provider: "Google" })}
            </button>
          )}
          {providers?.apple && (
            <button className="auth-sso" onClick={() => loginSSO("apple")}>
              {t("auth.signInWith", { provider: "Apple" })}
            </button>
          )}
          {providers?.github && (
            <button className="auth-sso" onClick={() => loginSSO("github")}>
              {t("auth.signInWith", { provider: "GitHub" })}
            </button>
          )}
        </div>
      )}
      <div className="bulbs"></div>
    </header>
  );
}
