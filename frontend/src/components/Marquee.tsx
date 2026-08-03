import { useTranslation } from "react-i18next";

export function Marquee() {
  const { t } = useTranslation();
  return (
    <header className="marquee">
      <div className="bulbs"></div>
      <h1>🎬 {t("brand")}</h1>
      <p className="tagline">{t("tagline")}</p>
      <div className="bulbs"></div>
    </header>
  );
}
