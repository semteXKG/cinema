import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Marquee } from "../components/Marquee";
import { AddRuleChoice } from "../components/AddRuleChoice";
import { RuleSentence } from "../components/RuleSentence";
import type { NotificationPreferences, NotificationRule, Cinema } from "../types";
import { fetchPreferences, savePreferences, fetchRules, saveRules } from "../api/preferences";

export function PreferencesPage() {
  const { t } = useTranslation();
  const [prefs, setPrefs] = useState<NotificationPreferences | null>(null);
  const [rules, setRules] = useState<NotificationRule[]>([]);
  const [cinemas, setCinemas] = useState<Cinema[]>([]);
  const [loading, setLoading] = useState(true);
  const [saved, setSaved] = useState(false);
  const [rulesSaved, setRulesSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [adding, setAdding] = useState(false);

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
      const updated = await savePreferences({ telegramHandle: prefs.telegramHandle });
      setPrefs(updated);
      setSaved(true);
      setError(null);
    } catch {
      setError(t("preferences.saveError"));
    }
  };

  const addRule = (rule: NotificationRule) => {
    setRules([...rules, { ...rule, position: rules.length }]);
    setAdding(false);
  };
  const removeRule = (i: number) => setRules(rules.filter((_, idx) => idx !== i).map((r, idx) => ({ ...r, position: idx })));
  const updateRule = (i: number, patch: Partial<NotificationRule>) => setRules(rules.map((r, idx) => idx === i ? { ...r, ...patch } : r));
  const moveRule = (i: number, dir: -1 | 1) => {
    const j = i + dir;
    if (j < 0 || j >= rules.length) return;
    const next = [...rules];
    const tmp = next[i]; next[i] = next[j]; next[j] = tmp;
    setRules(next.map((r, idx) => ({ ...r, position: idx })));
  };
  const handleSaveRules = async () => { const res = await saveRules(rules); setRules(res.rules); setRulesSaved(true); };

  if (loading) return <div className="preferences"><Marquee /><p>{t("preferences.loading")}</p></div>;
  if (error) return <div className="preferences"><Marquee /><p className="pref-error">{error}</p></div>;
  if (!prefs) return null;

  const telegramUnverified = !prefs.telegramVerified;

  return (
    <div className="preferences">
      <Marquee />
      <h2>{t("preferences.title")}</h2>
      <div className="card pref-card">
        <h3>{t("preferences.telegram")}</h3>
        <p className="pref-desc">{t("preferences.telegramDesc")}</p>
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
        <div className="pref-telegram-status">
          {prefs.telegramVerified ? (
            <span className="pref-verified">{t("preferences.telegramVerified")}</span>
          ) : prefs.telegramHandle ? (
            <span className="pref-verify-prompt">{t("preferences.telegramVerifyPrompt")}</span>
          ) : null}
        </div>
      </div>
      <div className="pref-actions">
        <button className="auth-submit" onClick={handleSave}>{t("preferences.save")}</button>
        {saved && <span className="pref-saved">{t("preferences.saved")}</span>}
      </div>
      <h3>{t("preferences.rulesTitle")}</h3>
      <p className="pref-desc">{t("preferences.rulesDesc")}</p>
      {rules.map((r, i) => (
        <RuleSentence
          key={i}
          rule={r}
          index={i}
          total={rules.length}
          cinemas={cinemas}
          telegramUnverified={telegramUnverified}
          onChange={(patch) => updateRule(i, patch)}
          onRemove={() => removeRule(i)}
          onMoveUp={() => moveRule(i, -1)}
          onMoveDown={() => moveRule(i, 1)}
        />
      ))}
      {adding ? (
        <AddRuleChoice onAdd={addRule} />
      ) : (
        <div className="pref-actions">
          <button className="auth-submit" onClick={() => setAdding(true)}>{t("preferences.addRule")}</button>
          <button className="auth-submit" onClick={handleSaveRules}>{t("preferences.saveRules")}</button>
          {rulesSaved && <span className="pref-saved">{t("preferences.saved")}</span>}
        </div>
      )}
    </div>
  );
}
