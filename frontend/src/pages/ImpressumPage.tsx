import { useTranslation } from "react-i18next";
import { Marquee } from "../components/Marquee";

export function ImpressumPage() {
  const { t } = useTranslation();
  return (
    <div className="impressum">
      <Marquee />
      <h1>{t("impressum.title")}</h1>
      <p>
        {t("impressum.operator")}
        <br />
        4230 {t("impressum.city")}
        <br />
        <a href={`mailto:${t("impressum.email")}`}>{t("impressum.email")}</a>
      </p>
      <p className="small">{t("impressum.private")}</p>
      <p className="small">{t("impressum.takedown")}</p>
    </div>
  );
}
