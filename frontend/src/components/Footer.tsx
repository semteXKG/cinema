import { Link } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { LanguageSwitcher } from "./LanguageSwitcher";

export function Footer() {
  const { t } = useTranslation();
  return (
    <footer className="footer">
      <Link to="/impressum">{t("footer.impressum")}</Link>
      <LanguageSwitcher />
    </footer>
  );
}
