import { useTranslation } from "react-i18next";

const LANGS = ["en", "de"] as const;

export function LanguageSwitcher() {
  const { i18n } = useTranslation();
  return (
    <span className="lang">
      {LANGS.map((l) => (
        <button
          key={l}
          type="button"
          onClick={() => i18n.changeLanguage(l)}
          className={i18n.language.startsWith(l) ? "active" : ""}
        >
          {l.toUpperCase()}
        </button>
      ))}
    </span>
  );
}
