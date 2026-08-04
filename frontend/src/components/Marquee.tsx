import { NavLink } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { LanguageSwitcher } from "./LanguageSwitcher";

export function Marquee() {
  const { t } = useTranslation();
  return (
    <header className="marquee">
      <div className="bulbs"></div>
      <div className="marquee-brand">
        <img className="marquee-logo" src="/projector-logo.png" alt="" />
        <h1>{t("brand")}</h1>
      </div>
      <p className="tagline">{t("tagline")}</p>
      <nav className="marqnav">
        <NavLink to="/">{t("nav.home")}</NavLink>
        <NavLink to="/impressum">{t("nav.impressum")}</NavLink>
        <LanguageSwitcher />
      </nav>
      <div className="bulbs"></div>
    </header>
  );
}
