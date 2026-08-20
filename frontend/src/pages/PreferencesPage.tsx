import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
import { Marquee } from "../components/Marquee";
import { FEATURES, FREQUENCY_OPTIONS, type NotificationFrequency, type NotificationPreferences, type NotificationRule, type Cinema } from "../types";
import { fetchPreferences, savePreferences, fetchRules, saveRules } from "../api/preferences";

function frequencyLabel(t: TFunction, value: NotificationFrequency): string {
  if (value === "never") return t("preferences.frequencies.never");
  if (value === "immediately") return t("preferences.frequencies.immediately");
  return t("preferences.frequencies.days", { count: Number(value) });
}

export function PreferencesPage() {
  const { t } = useTranslation();
  const [prefs, setPrefs] = useState<NotificationPreferences | null>(null);
  const [rules, setRules] = useState<NotificationRule[]>([]);
  const [cinemas, setCinemas] = useState<Cinema[]>([]);
  const [loading, setLoading] = useState(true);
  const [saved, setSaved] = useState(false);
  const [rulesSaved, setRulesSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    Promise.all([fetchPreferences(), fetchRules()])
      .then(([p, r]) => {
        if (!cancelled) {
          setPrefs(p);
          setRules(r.rules);
          setCinemas(r.cinemas);
        }
      })
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

  const addRule = () => setRules([...rules, { position: rules.length, cinemaId: null, features: [], titleSubstring: null, frequency: "3" }]);
  const removeRule = (i: number) => setRules(rules.filter((_, idx) => idx !== i).map((r, idx) => ({ ...r, position: idx })));
  const updateRule = (i: number, patch: Partial<NotificationRule>) => setRules(rules.map((r, idx) => idx === i ? { ...r, ...patch } : r));
  const toggleFeature = (i: number, f: string) => setRules(rules.map((r, idx) => idx === i ? { ...r, features: r.features.includes(f) ? r.features.filter((x) => x !== f) : [...r.features, f] } : r));
  const handleSaveRules = async () => { const res = await saveRules(rules); setRules(res.rules); setRulesSaved(true); };

  if (loading) return <div className="preferences"><Marquee /><p>{t("preferences.loading")}</p></div>;
  if (error) return <div className="preferences"><Marquee /><p className="pref-error">{error}</p></div>;
  if (!prefs) return null;

  const channels: Array<{ name: "email" | "telegram"; enabled: boolean; onChange: (v: boolean) => void }> = [
    { name: "email", enabled: prefs.emailEnabled, onChange: (v) => setPrefs({ ...prefs, emailEnabled: v }) },
    { name: "telegram", enabled: prefs.telegramEnabled, onChange: (v) => setPrefs({ ...prefs, telegramEnabled: v }) },
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
            <span>{t("preferences." + c.name)}</span>
            <input
              type="checkbox"
              aria-label={t("preferences." + c.name)}
              checked={c.enabled}
              onChange={(e) => c.onChange(e.target.checked)}
            />
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
      <h3>{t("preferences.rulesTitle")}</h3>
      <p className="pref-desc">{t("preferences.rulesDesc")}</p>
      {rules.map((r, i) => (
        <div className="card pref-card" key={i}>
          <div className="rule-row">
            <select aria-label={"Rule " + (i + 1) + " cinema"} value={r.cinemaId ?? ""} onChange={(e) => updateRule(i, { cinemaId: e.target.value ? Number(e.target.value) : null })}>
              <option value="">{t("preferences.anyCinema")}</option>
              {cinemas.map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
            </select>
            <input aria-label={"Rule " + (i + 1) + " title"} placeholder={t("preferences.anyTitle")} value={r.titleSubstring ?? ""} onChange={(e) => updateRule(i, { titleSubstring: e.target.value || null })} />
            <select aria-label={"Rule " + (i + 1) + " frequency"} value={r.frequency} onChange={(e) => updateRule(i, { frequency: e.target.value as NotificationFrequency })}>
              {FREQUENCY_OPTIONS.map((v) => <option key={v} value={v}>{frequencyLabel(t, v)}</option>)}
            </select>
            <button className="mock-button" onClick={() => removeRule(i)}>{"x"}</button>
          </div>
          <div className="rule-features">
            {FEATURES.map((f) => (
              <button key={f} className={"chip " + (r.features.includes(f) ? "chip-on" : "")} onClick={() => toggleFeature(i, f)}>{f}</button>
            ))}
          </div>
        </div>
      ))}
      <button className="auth-submit" onClick={addRule}>{t("preferences.addRule")}</button>
      <button className="auth-submit" onClick={handleSaveRules}>{t("preferences.saveRules")}</button>
      {rulesSaved && <span className="pref-saved">{t("preferences.saved")}</span>}
    </div>
  );
}