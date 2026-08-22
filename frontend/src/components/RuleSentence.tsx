import { useTranslation } from "react-i18next";
import { FEATURES, FREQUENCY_OPTIONS, type Cinema, type NotificationChannel, type NotificationFrequency, type NotificationRule } from "../types";
import { frequencyLabel } from "../format";
import { PillDropdown, type PillDropdownOption } from "./PillDropdown";
import { FeaturePopover } from "./FeaturePopover";

export interface RuleSentenceProps {
  rule: NotificationRule;
  index: number;
  total: number;
  cinemas: Cinema[];
  telegramUnverified: boolean;
  onChange: (patch: Partial<NotificationRule>) => void;
  onRemove: () => void;
  onMoveUp: () => void;
  onMoveDown: () => void;
}

export function RuleSentence({ rule, index, total, cinemas, telegramUnverified, onChange, onRemove, onMoveUp, onMoveDown }: RuleSentenceProps) {
  const { t } = useTranslation();

  const cinemaOptions: PillDropdownOption[] = [
    { value: "", label: t("preferences.anyCinema") },
    ...cinemas.map((c) => ({ value: String(c.id), label: c.name })),
  ];
  const frequencyOptions: PillDropdownOption[] = FREQUENCY_OPTIONS.map((v) => ({
    value: v, label: frequencyLabel(t, v),
  }));

  const emailOn = rule.channel === "email" || rule.channel === "both";
  const telegramOn = rule.channel === "telegram" || rule.channel === "both";
  const isNever = rule.frequency === "never";

  const toggleChannel = (which: "email" | "telegram") => {
    const email = which === "email" ? !emailOn : emailOn;
    const telegram = which === "telegram" ? !telegramOn : telegramOn;
    const channel: NotificationChannel = email && telegram ? "both" : email ? "email" : "telegram";
    onChange({ channel });
  };

  const toggleFeature = (f: string) => {
    const features = rule.features.includes(f)
      ? rule.features.filter((x) => x !== f)
      : [...rule.features, f];
    onChange({ features });
  };

  return (
    <div className="card pref-card sentence-card">
      <div className="sentence" aria-label={"Rule " + (index + 1)}>
        <span className="sentence-text">{t("preferences.sentencePrefix")}</span>{" "}
        <PillDropdown
          ariaLabel={"Rule " + (index + 1) + " cinema"}
          value={rule.cinemaId == null ? "" : String(rule.cinemaId)}
          options={cinemaOptions}
          onChange={(v) => onChange({ cinemaId: v ? Number(v) : null })}
        />{" "}
        <span className="sentence-text">{t("preferences.sentenceFilm")}</span>{" "}
        <span className="sentence-text">{t("preferences.sentenceWith")}</span>{" "}
        {rule.features.length === 0 ? (
          <span className="sentence-any">{t("preferences.sentenceAnyFeature")}</span>
        ) : (
          rule.features.map((f) => (
            <button
              key={f}
              type="button"
              className="pill pill-on pill-feature"
              aria-label={"remove " + f}
              onClick={() => toggleFeature(f)}
            >
              {f} ✕
            </button>
          ))
        )}
        <FeaturePopover selected={rule.features} onToggle={toggleFeature} />{" "}
        <span className="sentence-text">{t("preferences.sentenceShows")}</span>{" "}
        <span className="sentence-text">{t("preferences.sentenceTitleContains")}</span>{" "}
        <input
          className="pref-input sentence-title"
          type="text"
          placeholder={t("preferences.anyTitle")}
          value={rule.titleSubstring ?? ""}
          onChange={(e) => onChange({ titleSubstring: e.target.value || null })}
          aria-label={"Rule " + (index + 1) + " title"}
        />
        {isNever ? null : (
          <>
            <span className="sentence-text">
              {rule.frequency === "immediately"
                ? t("preferences.sentenceSendImmediate")
                : t("preferences.sentenceSendDigest")}
            </span>{" "}
            <PillDropdown
              ariaLabel={"Rule " + (index + 1) + " frequency"}
              value={rule.frequency}
              options={frequencyOptions}
              onChange={(v) => onChange({ frequency: v as NotificationFrequency })}
            />{" "}
            <span className="sentence-text">{t("preferences.sentenceOver")}</span>{" "}
            <button
              type="button"
              className={"pill " + (emailOn ? "pill-on" : "")}
              aria-label="Email"
              disabled={emailOn && !telegramOn}
              onClick={() => toggleChannel("email")}
            >
              {emailOn ? "✓ " : ""}{t("preferences.channelEmail")}
            </button>
            <button
              type="button"
              className={"pill " + (telegramOn ? "pill-on" : "")}
              aria-label="Telegram"
              disabled={telegramOn && !emailOn}
              onClick={() => toggleChannel("telegram")}
            >
              {telegramOn ? "✓ " : ""}{t("preferences.channelTelegram")}
            </button>
          </>
        )}
        {(rule.channel === "telegram" || rule.channel === "both") && telegramUnverified && !isNever && (
          <span className="rule-warn">{t("preferences.telegramUnverified")}</span>
        )}
        <span className="rule-actions">
          <button type="button" className="rule-remove" aria-label="remove rule" onClick={onRemove}>x</button>
          <button type="button" className="rule-move" aria-label="move up" disabled={index === 0} onClick={onMoveUp}>^</button>
          <button type="button" className="rule-move" aria-label="move down" disabled={index === total - 1} onClick={onMoveDown}>v</button>
        </span>
      </div>
    </div>
  );
}
