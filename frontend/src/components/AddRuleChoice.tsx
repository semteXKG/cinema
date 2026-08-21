import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { NotificationRule } from "../types";

export const DEFAULT_RULE: NotificationRule = {
  position: 0,
  cinemaId: null,
  features: [],
  titleSubstring: null,
  frequency: "3",
  channel: "both",
};

export interface RuleTemplate {
  key: string;
  rule: NotificationRule;
}

export const TEMPLATES: RuleTemplate[] = [
  {
    key: "ovTelegram",
    rule: { position: 0, cinemaId: null, features: ["OV"], titleSubstring: null, frequency: "immediately", channel: "telegram" },
  },
  {
    key: "digestEmail",
    rule: { position: 0, cinemaId: null, features: [], titleSubstring: null, frequency: "7", channel: "email" },
  },
  {
    key: "instantAll",
    rule: { position: 0, cinemaId: null, features: [], titleSubstring: null, frequency: "immediately", channel: "both" },
  },
];

export interface AddRuleChoiceProps {
  onAdd: (rule: NotificationRule) => void;
}

export function AddRuleChoice({ onAdd }: AddRuleChoiceProps) {
  const { t } = useTranslation();
  const [mode, setMode] = useState<"choice" | "templates">("choice");

  if (mode === "templates") {
    return (
      <div className="template-list">
        {TEMPLATES.map((tpl) => (
          <button
            key={tpl.key}
            type="button"
            className="card pref-card template-card"
            onClick={() => onAdd(tpl.rule)}
          >
            <strong>{t("preferences.template" + tpl.key.charAt(0).toUpperCase() + tpl.key.slice(1))}</strong>
            <span className="template-summary">{t("preferences.template" + tpl.key.charAt(0).toUpperCase() + tpl.key.slice(1) + "Summary")}</span>
          </button>
        ))}
      </div>
    );
  }

  return (
    <div className="add-rule-choice">
      <button type="button" className="card pref-card choice-card" onClick={() => setMode("templates")}>
        <strong>{t("preferences.addRuleFromTemplate")}</strong>
        <span className="choice-desc">{t("preferences.addRuleFromTemplateDesc")}</span>
      </button>
      <button type="button" className="card pref-card choice-card" onClick={() => onAdd(DEFAULT_RULE)}>
        <strong>{t("preferences.addRuleNew")}</strong>
        <span className="choice-desc">{t("preferences.addRuleNewDesc")}</span>
      </button>
    </div>
  );
}
