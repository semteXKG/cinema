import { useTranslation } from "react-i18next";

export function InvalidLinkPage() {
  const { t } = useTranslation();
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
        <p className="auth-note">{t("auth.invalidLink")}</p>
      </main>
    </>
  );
}
