import { NavLink } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { LanguageSwitcher } from "./LanguageSwitcher";

export function Marquee() {
  const { t } = useTranslation();
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
      </nav>
      <div className="bulbs"></div>
    </header>
  );
}
