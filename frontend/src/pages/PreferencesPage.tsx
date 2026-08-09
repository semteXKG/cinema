import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
import { Marquee } from "../components/Marquee";
import { FREQUENCY_OPTIONS, type NotificationFrequency } from "../types";

function frequencyLabel(t: TFunction, value: NotificationFrequency): string {
  if (value === "never") return t("preferences.frequencies.never");
  if (value === "immediately") return t("preferences.frequencies.immediately");
  return t("preferences.frequencies.days", { count: Number(value) });
}

export function PreferencesPage() {
  const { t } = useTranslation();
  const [emailFreq, setEmailFreq] = useState<NotificationFrequency>("immediately");
  const [telegramFreq, setTelegramFreq] = useState<NotificationFrequency>("never");
  const [saved, setSaved] = useState(false);
  const [telegramHandle, setTelegramHandle] = useState("");

  useEffect(() => {
    if (!saved) return;
    const id = setTimeout(() => setSaved(false), 2000);
    return () => clearTimeout(id);
  }, [saved]);

  const channels: Array<{
    name: "email" | "telegram";
    freq: NotificationFrequency;
    onChange: (v: NotificationFrequency) => void;
  }> = [
    { name: "email", freq: emailFreq, onChange: setEmailFreq },
    { name: "telegram", freq: telegramFreq, onChange: setTelegramFreq },
  ];

  return (
    <div className="preferences">
      <Marquee />
      <h2>{t("preferences.title")}</h2>
      {channels.map((c) => (
        <div className="card pref-card" key={c.name}>
          <h3>{t(`preferences.${c.name}`)}</h3>
          <p className="pref-desc">{t(`preferences.${c.name}Desc`)}</p>
          <label className="pref-field">
            <span>{t("preferences.frequency")}</span>
            <select
              className="pref-select"
              aria-label={t(`preferences.${c.name}`)}
              value={c.freq}
              onChange={(e) => c.onChange(e.target.value as NotificationFrequency)}
            >
              {FREQUENCY_OPTIONS.map((v) => (
                <option key={v} value={v}>
                  {frequencyLabel(t, v)}
                </option>
              ))}
            </select>
          </label>
          {c.name === "telegram" && (
            <label className="pref-field">
              <span>{t("preferences.telegramHandle")}</span>
              <input
                className="pref-input"
                type="text"
                placeholder={t("preferences.telegramHandlePlaceholder")}
                value={telegramHandle}
                onChange={(e) => setTelegramHandle(e.target.value)}
                aria-label={t("preferences.telegramHandle")}
              />
            </label>
          )}
        </div>
      ))}
      <div className="pref-actions">
        <button className="auth-submit" onClick={() => setSaved(true)}>
          {t("preferences.save")}
        </button>
        {saved && <span className="pref-saved">{t("preferences.saved")}</span>}
      </div>
    </div>
  );
}
