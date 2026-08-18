import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
import { Marquee } from "../components/Marquee";
import { FREQUENCY_OPTIONS, type NotificationFrequency, type NotificationPreferences } from "../types";
import { fetchPreferences, savePreferences } from "../api/preferences";

function frequencyLabel(t: TFunction, value: NotificationFrequency): string {
  if (value === "never") return t("preferences.frequencies.never");
  if (value === "immediately") return t("preferences.frequencies.immediately");
  return t("preferences.frequencies.days", { count: Number(value) });
}

export function PreferencesPage() {
  const { t } = useTranslation();
  const [prefs, setPrefs] = useState<NotificationPreferences | null>(null);
  const [loading, setLoading] = useState(true);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    fetchPreferences()
      .then((p) => { if (!cancelled) setPrefs(p); })
      .catch(() => { if (!cancelled) setError(t("preferences.loadError")); })
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [t]);

  useEffect(() => {
    if (!saved) return;
    const id = setTimeout(() => setSaved(false), 2000);
    return () => clearTimeout(id);
  }, [saved]);

  const handleSave = async () => {
    if (!prefs) return;
    try {
      const updated = await savePreferences(prefs);
      setPrefs(updated);
      setSaved(true);
      setError(null);
    } catch {
      setError(t("preferences.saveError"));
    }
  };

  if (loading) return <div className="preferences"><Marquee /><p>{t("preferences.loading")}</p></div>;
  if (error) return <div className="preferences"><Marquee /><p className="pref-error">{error}</p></div>;
  if (!prefs) return null;

  const channels: Array<{
    name: "email" | "telegram";
    freq: NotificationFrequency;
    onChange: (v: NotificationFrequency) => void;
  }> = [
    { name: "email", freq: prefs.emailFrequency, onChange: (v) => setPrefs({ ...prefs, emailFrequency: v }) },
    { name: "telegram", freq: prefs.telegramFrequency, onChange: (v) => setPrefs({ ...prefs, telegramFrequency: v }) },
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
                <option key={v} value={v}>{frequencyLabel(t, v)}</option>
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
                value={prefs.telegramHandle ?? ""}
                onChange={(e) => setPrefs({ ...prefs, telegramHandle: e.target.value })}
                aria-label={t("preferences.telegramHandle")}
              />
            </label>
          )}
          {c.name === "telegram" && (
            <div className="pref-telegram-status">
              {prefs.telegramVerified ? (
                <span className="pref-verified">{t("preferences.telegramVerified")}</span>
              ) : prefs.telegramHandle ? (
                <span className="pref-verify-prompt">{t("preferences.telegramVerifyPrompt")}</span>
              ) : null}
            </div>
          )}
        </div>
      ))}
      <div className="pref-actions">
        <button className="auth-submit" onClick={handleSave}>{t("preferences.save")}</button>
        {saved && <span className="pref-saved">{t("preferences.saved")}</span>}
      </div>
    </div>
  );
}
