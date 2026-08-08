import { useState } from "react";
import { NavLink } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { LanguageSwitcher } from "./LanguageSwitcher";
import { useAuth } from "../hooks/useAuth";
import { LoginModal } from "./LoginModal";

export function Marquee() {
  const { t } = useTranslation();
  const { user, loading, logout } = useAuth();
  const [showLogin, setShowLogin] = useState(false);

  return (
    <header className="marquee">
      <div className="bulbs"></div>
      <div className="marquee-brand">
        <img className="marquee-logo" src="/projector-logo.svg" alt="" />
        <div className="marquee-text">
          <h1>{t("brand")}</h1>
          <p className="tagline">{t("tagline")}</p>
        </div>
      </div>
      <nav className="marqnav">
        <NavLink to="/">{t("nav.home")}</NavLink>
        <NavLink to="/impressum">{t("nav.impressum")}</NavLink>
        <LanguageSwitcher />
        {!loading &&
          (!user ? (
            <button className="auth-btn" onClick={() => setShowLogin(true)}>
              {t("auth.signIn")}
            </button>
          ) : (
            <button className="auth-btn" onClick={logout}>
              {t("auth.signOut")}
            </button>
          ))}
      </nav>
      <LoginModal open={showLogin} onClose={() => setShowLogin(false)} />
      <div className="bulbs"></div>
    </header>
  );
}
