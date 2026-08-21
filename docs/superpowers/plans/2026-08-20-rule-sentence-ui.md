# Rule sentence-builder UI — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the flat row-of-dropdowns rule editor on the Preferences page with a natural-language sentence builder, plus a template-picker entry point for "+ Regel hinzufügen".

**Architecture:** New leaf React components (`PillDropdown`, `FeaturePopover`, `AddRuleChoice`, `RuleSentence`) composed into `PreferencesPage`. The data model and API are unchanged (already carry `cinemaId`, `features`, `titleSubstring`, `frequency`, `channels`). Build stays green throughout: new components are added first (unused by the page until Task 5), then the page is rewired, then CSS.

**Tech Stack:** React 19 + Vite + TypeScript, vitest + @testing-library/react, react-i18next. Verification per task: `cd frontend && npm test -- --run && npm run build`.

**Spec:** `docs/superpowers/specs/2026-08-20-rule-sentence-ui-design.md`

## Global Constraints

- Channel vocabulary is exactly `{"email", "telegram"}` on the wire; the frontend uses `channel: "email" | "telegram" | "both"` and converts at the API boundary (already implemented in `api/preferences.ts`).
- Feature vocabulary is exactly `["OV","OmU","OmdU","2D","3D","IMAX","Atmos","DolbyCinema","4DX"]` (from `FEATURES` in `types.ts`).
- Frequency values are exactly `"never" | "immediately" | "1".."7"` (from `FREQUENCY_OPTIONS`).
- No emojis in code or copy (the 🗑 and ↑↓ in mockups are placeholders — use text/SVG). No new dependencies.
- CSS variables to reuse: `--bg`, `--panel`, `--edge`, `--gold`, `--gold-bright`, `--text`, `--dim`, `--faint`, `--err`, `--ok`.
- German + English copy in `locales/de.json` and `locales/en.json`. The app is German-first.
- All interactive pills/buttons must have ≥40px tap targets (touch-friendly).

## File Structure

- `frontend/src/components/PillDropdown.tsx` — NEW. Reusable ▾ pill that opens a dropdown list. Used for cinema + frequency.
- `frontend/src/components/FeaturePopover.tsx` — NEW. "+ Feature" pill that opens a popover with all 9 features as toggle pills.
- `frontend/src/components/AddRuleChoice.tsx` — NEW. The "+ Regel" entry (two cards: template / new). Exports `DEFAULT_RULE` and `TEMPLATES`.
- `frontend/src/components/RuleSentence.tsx` — NEW. Renders one rule as the editable sentence. Uses PillDropdown + FeaturePopover.
- `frontend/src/pages/PreferencesPage.tsx` — MODIFY. Rewire to use `RuleSentence` + `AddRuleChoice`; add `moveRule`; change `addRule(rule)` signature.
- `frontend/src/index.css` — MODIFY. Add `.sentence`/`.pill`/`.popover` classes; remove old `.rule-row`/`.chip`/`.rule-remove`/`.rule-features` classes.
- `frontend/src/locales/en.json` + `de.json` — MODIFY. Add sentence fragments, add-rule entry, template names + summaries; update `rulesDesc`.
- `frontend/src/pages/PreferencesPage.test.tsx` — MODIFY. Rewrite for the new structure.
- New test files: `PillDropdown.test.tsx`, `FeaturePopover.test.tsx`, `AddRuleChoice.test.tsx`, `RuleSentence.test.tsx`.

---

### Task 1: PillDropdown component

A reusable ▾ pill that opens a dropdown list of options. Standalone; no dependencies on other new components.

**Files:**
- Create: `frontend/src/components/PillDropdown.tsx`
- Create: `frontend/src/components/PillDropdown.test.tsx`

**Interfaces:**
- Produces:
  ```tsx
  export interface PillDropdownOption { value: string; label: string; }
  export interface PillDropdownProps {
    value: string;
    options: PillDropdownOption[];
    onChange: (value: string) => void;
    ariaLabel: string;
  }
  export function PillDropdown({ value, options, onChange, ariaLabel }: PillDropdownProps): JSX.Element
  ```
  Renders a `<button class="pill pill-drop">` showing the matching option's label + "▾". Click opens a `<div class="dropdown">` listing all options as buttons; the selected one has class `pill-on`. Clicking an option calls `onChange(value)` and closes. Outside-click or Esc closes without selecting.

- [ ] **Step 1: Write the failing test**

Create `frontend/src/components/PillDropdown.test.tsx`:

```tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { PillDropdown } from "./PillDropdown";

const opts = [
  { value: "a", label: "Alpha" },
  { value: "b", label: "Beta" },
];

describe("PillDropdown", () => {
  it("shows the current value's label and opens on click", () => {
    const onChange = vi.fn();
    render(<PillDropdown value="a" options={opts} onChange={onChange} ariaLabel="pick" />);
    expect(screen.getByRole("button", { name: "pick" })).toHaveTextContent("Alpha");
    expect(screen.queryByText("Beta")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "pick" }));
    expect(screen.getByText("Beta")).toBeInTheDocument();
  });

  it("selects an option and closes", () => {
    const onChange = vi.fn();
    render(<PillDropdown value="a" options={opts} onChange={onChange} ariaLabel="pick" />);
    fireEvent.click(screen.getByRole("button", { name: "pick" }));
    fireEvent.click(screen.getByText("Beta"));
    expect(onChange).toHaveBeenCalledWith("b");
    expect(screen.queryByText("Alpha")).not.toBeInTheDocument();
  });

  it("closes on outside click without selecting", () => {
    const onChange = vi.fn();
    render(
      <div>
        <PillDropdown value="a" options={opts} onChange={onChange} ariaLabel="pick" />
        <div data-testid="outside">outside</div>
      </div>
    );
    fireEvent.click(screen.getByRole("button", { name: "pick" }));
    fireEvent.mouseDown(screen.getByTestId("outside"));
    expect(screen.queryByText("Beta")).not.toBeInTheDocument();
    expect(onChange).not.toHaveBeenCalled();
  });

  it("closes on Escape without selecting", () => {
    const onChange = vi.fn();
    render(<PillDropdown value="a" options={opts} onChange={onChange} ariaLabel="pick" />);
    fireEvent.click(screen.getByRole("button", { name: "pick" }));
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(screen.queryByText("Beta")).not.toBeInTheDocument();
    expect(onChange).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd frontend && npm test -- --run PillDropdown`
Expected: FAIL — module `./PillDropdown` not found.

- [ ] **Step 3: Write the component**

Create `frontend/src/components/PillDropdown.tsx`:

```tsx
import { useEffect, useRef, useState } from "react";

export interface PillDropdownOption {
  value: string;
  label: string;
}

export interface PillDropdownProps {
  value: string;
  options: PillDropdownOption[];
  onChange: (value: string) => void;
  ariaLabel: string;
}

export function PillDropdown({ value, options, onChange, ariaLabel }: PillDropdownProps) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const current = options.find((o) => o.value === value);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div className="pill-wrap" ref={ref}>
      <button
        type="button"
        className="pill pill-drop"
        aria-label={ariaLabel}
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
      >
        {current?.label ?? ""} ▾
      </button>
      {open && (
        <div className="dropdown" role="listbox" aria-label={ariaLabel}>
          {options.map((o) => (
            <button
              key={o.value}
              type="button"
              className={"pill " + (o.value === value ? "pill-on" : "")}
              onClick={() => { onChange(o.value); setOpen(false); }}
              role="option"
              aria-selected={o.value === value}
            >
              {o.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd frontend && npm test -- --run PillDropdown`
Expected: 4 tests pass.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/PillDropdown.tsx frontend/src/components/PillDropdown.test.tsx
git commit -m "feat: PillDropdown reusable dropdown pill"
```

---

### Task 2: FeaturePopover component

A "+ Feature" pill that opens a popover listing all 9 features as toggle pills (selected = gold). Standalone.

**Files:**
- Create: `frontend/src/components/FeaturePopover.tsx`
- Create: `frontend/src/components/FeaturePopover.test.tsx`
- Modify: `frontend/src/locales/en.json` (add `addFeature`)
- Modify: `frontend/src/locales/de.json` (add `addFeature`)

**Interfaces:**
- Produces:
  ```tsx
  export interface FeaturePopoverProps {
    selected: string[];
    onToggle: (feature: string) => void;
  }
  export function FeaturePopover({ selected, onToggle }: FeaturePopoverProps): JSX.Element
  ```
  Renders a `<button class="pill pill-add">+ Feature</button>`. Click opens a `<div class="popover">` listing all `FEATURES`; each is a `.pill` (`.pill-on` if selected). Clicking a feature calls `onToggle(feature)` and keeps the popover open. Outside-click or Esc closes.

- [ ] **Step 1: Add the i18n key**

In `frontend/src/locales/en.json` under `preferences`, add:
```json
    "addFeature": "+ Feature",
```

In `frontend/src/locales/de.json` under `preferences`, add:
```json
    "addFeature": "+ Feature",
```

- [ ] **Step 2: Write the failing test**

Create `frontend/src/components/FeaturePopover.test.tsx`:

```tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../i18n";
import { FeaturePopover } from "./FeaturePopover";

beforeEach(() => i18n.changeLanguage("en"));

describe("FeaturePopover", () => {
  it("opens on click and lists all 9 features", () => {
    const onToggle = vi.fn();
    render(<FeaturePopover selected={["OV"]} onToggle={onToggle} />);
    fireEvent.click(screen.getByRole("button", { name: /\+ Feature/i }));
    expect(screen.getByText("OV")).toBeInTheDocument();
    expect(screen.getByText("IMAX")).toBeInTheDocument();
    expect(screen.getByText("4DX")).toBeInTheDocument();
    expect(screen.getByText("OV")).toHaveClass("pill-on");
  });

  it("toggles a feature and stays open", () => {
    const onToggle = vi.fn();
    render(<FeaturePopover selected={[]} onToggle={onToggle} />);
    fireEvent.click(screen.getByRole("button", { name: /\+ Feature/i }));
    fireEvent.click(screen.getByText("IMAX"));
    expect(onToggle).toHaveBeenCalledWith("IMAX");
    expect(screen.getByText("OV")).toBeInTheDocument();
  });

  it("closes on Escape", () => {
    const onToggle = vi.fn();
    render(<FeaturePopover selected={[]} onToggle={onToggle} />);
    fireEvent.click(screen.getByRole("button", { name: /\+ Feature/i }));
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(screen.queryByText("OV")).not.toBeInTheDocument();
  });
});
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cd frontend && npm test -- --run FeaturePopover`
Expected: FAIL — module `./FeaturePopover` not found.

- [ ] **Step 4: Write the component**

Create `frontend/src/components/FeaturePopover.tsx`:

```tsx
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { FEATURES } from "../types";

export interface FeaturePopoverProps {
  selected: string[];
  onToggle: (feature: string) => void;
}

export function FeaturePopover({ selected, onToggle }: FeaturePopoverProps) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div className="pill-wrap" ref={ref}>
      <button
        type="button"
        className="pill pill-add"
        aria-haspopup="dialog"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
      >
        {t("preferences.addFeature")}
      </button>
      {open && (
        <div className="popover" role="dialog">
          {FEATURES.map((f) => (
            <button
              key={f}
              type="button"
              className={"pill " + (selected.includes(f) ? "pill-on" : "")}
              onClick={() => onToggle(f)}
            >
              {selected.includes(f) ? "✓ " : ""}{f}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cd frontend && npm test -- --run FeaturePopover`
Expected: 3 tests pass.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/FeaturePopover.tsx frontend/src/components/FeaturePopover.test.tsx frontend/src/locales/en.json frontend/src/locales/de.json
git commit -m "feat: FeaturePopover pill picker"
```

---

### Task 3: AddRuleChoice component

The "+ Regel hinzufügen" entry. Two cards: "Aus Vorlage erstellen" / "Neue Regel starten". Reveals 3 template cards when "Aus Vorlage" is picked. Exports `DEFAULT_RULE` and `TEMPLATES`.

**Files:**
- Create: `frontend/src/components/AddRuleChoice.tsx`
- Create: `frontend/src/components/AddRuleChoice.test.tsx`
- Modify: `frontend/src/locales/en.json` (add entry + template keys)
- Modify: `frontend/src/locales/de.json` (add entry + template keys)

**Interfaces:**
- Produces:
  ```tsx
  export const DEFAULT_RULE: NotificationRule;            // cinemaId null, [], null, "3", "both", position 0
  export interface RuleTemplate { key: string; rule: NotificationRule; }
  export const TEMPLATES: RuleTemplate[];                  // 3 templates
  export interface AddRuleChoiceProps { onAdd: (rule: NotificationRule) => void; }
  export function AddRuleChoice({ onAdd }: AddRuleChoiceProps): JSX.Element
  ```
  `DEFAULT_RULE` = `{ position: 0, cinemaId: null, features: [], titleSubstring: null, frequency: "3", channel: "both" }`.
  `TEMPLATES`:
  - `{ key: "ovTelegram", rule: { position: 0, cinemaId: null, features: ["OV"], titleSubstring: null, frequency: "immediately", channel: "telegram" } }`
  - `{ key: "digestEmail", rule: { position: 0, cinemaId: null, features: [], titleSubstring: null, frequency: "7", channel: "email" } }`
  - `{ key: "instantAll", rule: { position: 0, cinemaId: null, features: [], titleSubstring: null, frequency: "immediately", channel: "both" } }`
  The page's `addRule(rule)` overwrites `position` to `rules.length`.

- [ ] **Step 1: Add the i18n keys**

In `frontend/src/locales/en.json` under `preferences`, add:
```json
    "addRuleFromTemplate": "From a template",
    "addRuleNew": "Start new",
    "addRuleFromTemplateDesc": "Quick-start with a ready-made rule",
    "addRuleNewDesc": "Blank sentence, all yours",
    "templateOvTelegram": "All OV showings instantly via Telegram",
    "templateOvTelegramSummary": "Any cinema · OV · instantly · Telegram",
    "templateDigestEmail": "Weekly digest via email",
    "templateDigestEmailSummary": "Any cinema · all features · every 7 days · email",
    "templateInstantAll": "Instant alert for everything, both channels",
    "templateInstantAllSummary": "Any cinema · all features · instantly · both",
```

In `frontend/src/locales/de.json` under `preferences`, add:
```json
    "addRuleFromTemplate": "Aus Vorlage erstellen",
    "addRuleNew": "Neue Regel starten",
    "addRuleFromTemplateDesc": "Schnellstart mit einer fertigen Regel",
    "addRuleNewDesc": "Leerer Satz, alles einstellbar",
    "templateOvTelegram": "Alle OV-Vorstellungen sofort per Telegram",
    "templateOvTelegramSummary": "Beliebiges Kino · OV · sofort · Telegram",
    "templateDigestEmail": "Wöchentlicher Digest per E-Mail",
    "templateDigestEmailSummary": "Beliebiges Kino · alle Features · alle 7 Tage · E-Mail",
    "templateInstantAll": "Sofort-Alarm für alles, beide Kanäle",
    "templateInstantAllSummary": "Beliebiges Kino · alle Features · sofort · beides",
```

- [ ] **Step 2: Write the failing test**

Create `frontend/src/components/AddRuleChoice.test.tsx`:

```tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, expect, it, vi, beforeEach } from "vitest";
import i18n from "../i18n";
import { AddRuleChoice, DEFAULT_RULE, TEMPLATES } from "./AddRuleChoice";

beforeEach(() => i18n.changeLanguage("en"));

describe("AddRuleChoice", () => {
  it("exports a default rule with expected values", () => {
    expect(DEFAULT_RULE).toEqual({
      position: 0, cinemaId: null, features: [], titleSubstring: null,
      frequency: "3", channel: "both",
    });
  });

  it("exports three templates", () => {
    expect(TEMPLATES).toHaveLength(3);
    expect(TEMPLATES.map((t) => t.key)).toEqual(["ovTelegram", "digestEmail", "instantAll"]);
  });

  it("'Start new' calls onAdd with DEFAULT_RULE", () => {
    const onAdd = vi.fn();
    render(<AddRuleChoice onAdd={onAdd} />);
    fireEvent.click(screen.getByText("Start new"));
    expect(onAdd).toHaveBeenCalledWith(DEFAULT_RULE);
  });

  it("reveals templates after 'From a template' and adds the chosen one", () => {
    const onAdd = vi.fn();
    render(<AddRuleChoice onAdd={onAdd} />);
    fireEvent.click(screen.getByText("From a template"));
    fireEvent.click(screen.getByText("All OV showings instantly via Telegram"));
    expect(onAdd).toHaveBeenCalledWith(TEMPLATES[0].rule);
  });
});
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cd frontend && npm test -- --run AddRuleChoice`
Expected: FAIL — module `./AddRuleChoice` not found.

- [ ] **Step 4: Write the component**

Create `frontend/src/components/AddRuleChoice.tsx`:

```tsx
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
```

Note: the i18n key lookup uses the template key with the first letter capitalized — `ovTelegram` → `templateOvTelegram`. The keys added in Step 1 match (`templateOvTelegram`, `templateOvTelegramSummary`, etc.).

- [ ] **Step 5: Run the test to verify it passes**

Run: `cd frontend && npm test -- --run AddRuleChoice`
Expected: 4 tests pass.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/AddRuleChoice.tsx frontend/src/components/AddRuleChoice.test.tsx frontend/src/locales/en.json frontend/src/locales/de.json
git commit -m "feat: AddRuleChoice template/new entry point"
```

---

### Task 4: RuleSentence component

Renders one rule as the editable sentence. Uses `PillDropdown` (cinema + frequency) and `FeaturePopover` (features). Channel pills toggle (disabled when last enabled). Title input. Frequency "never" hides the channel pills. Remove + up/down (disabled at ends). Telegram-unverified warning badge.

**Files:**
- Create: `frontend/src/components/RuleSentence.tsx`
- Create: `frontend/src/components/RuleSentence.test.tsx`
- Modify: `frontend/src/locales/en.json` (add sentence fragments)
- Modify: `frontend/src/locales/de.json` (add sentence fragments)

**Interfaces:**
- Consumes: `PillDropdown` (Task 1), `FeaturePopover` (Task 2), `FEATURES`/`FREQUENCY_OPTIONS`/`NotificationRule`/`Cinema`/`NotificationChannel`/`NotificationFrequency` from `../types`, the `frequencyLabel` helper (currently in `PreferencesPage.tsx` — move it to a shared module in this task so both `PreferencesPage` and `RuleSentence` can use it; see Step 4).
- Produces:
  ```tsx
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
  export function RuleSentence(props: RuleSentenceProps): JSX.Element
  ```

- [ ] **Step 1: Add the sentence-fragment i18n keys**

In `frontend/src/locales/en.json` under `preferences`, add:
```json
    "sentencePrefix": "Notify me when",
    "sentenceShows": "shows a movie",
    "sentenceWith": "with",
    "sentenceAny": "(any)",
    "sentenceTitleContains": "title contains",
    "sentenceSend": "and send it",
    "sentenceOver": "via",
```

In `frontend/src/locales/de.json` under `preferences`, add:
```json
    "sentencePrefix": "Benachrichtige mich, wenn",
    "sentenceShows": "einen Film zeigt",
    "sentenceWith": "mit",
    "sentenceAny": "(beliebig)",
    "sentenceTitleContains": "Titel enthält",
    "sentenceSend": "und schicke es",
    "sentenceOver": "über",
```

- [ ] **Step 2: Write the failing test**

Create `frontend/src/components/RuleSentence.test.tsx`:

```tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, expect, it, vi, beforeEach } from "vitest";
import i18n from "../i18n";
import { RuleSentence } from "./RuleSentence";
import type { NotificationRule, Cinema } from "../types";

beforeEach(() => i18n.changeLanguage("en"));

const cinemas: Cinema[] = [
  { id: 1, name: "Cineplexx Linz" },
  { id: 2, name: "Megaplex PlusCity" },
];

const baseRule: NotificationRule = {
  position: 0, cinemaId: null, features: [], titleSubstring: null,
  frequency: "3", channel: "both",
};

function renderRule(overrides: Partial<Parameters<typeof RuleSentence>[0]> = {}) {
  const props: Parameters<typeof RuleSentence>[0] = {
    rule: baseRule,
    index: 0,
    total: 1,
    cinemas,
    telegramUnverified: false,
    onChange: vi.fn(),
    onRemove: vi.fn(),
    onMoveUp: vi.fn(),
    onMoveDown: vi.fn(),
    ...overrides,
  };
  return { props, ...render(<RuleSentence {...props} />) };
}

describe("RuleSentence", () => {
  it("renders the sentence with the default rule", () => {
    renderRule();
    expect(screen.getByText(/Notify me when/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /cinema/i })).toHaveTextContent("Any cinema");
  });

  it("toggling Email off (when Telegram still on) sets channel to telegram", () => {
    const onChange = vi.fn();
    renderRule({ onChange });
    const emailPill = screen.getByRole("button", { name: /^Email$/i });
    fireEvent.click(emailPill);
    expect(onChange).toHaveBeenCalledWith({ channel: "telegram" });
  });

  it("Email pill is disabled when it is the only enabled channel", () => {
    renderRule({ rule: { ...baseRule, channel: "email" } });
    const emailPill = screen.getByRole("button", { name: /^Email$/i });
    expect(emailPill).toBeDisabled();
  });

  it("opening the feature popover and clicking IMAX adds it", () => {
    const onChange = vi.fn();
    renderRule({ onChange });
    fireEvent.click(screen.getByRole("button", { name: /\+ Feature/i }));
    fireEvent.click(screen.getByText("IMAX"));
    expect(onChange).toHaveBeenCalledWith({ features: ["IMAX"] });
  });

  it("clicking ✕ on a selected feature removes it", () => {
    const onChange = vi.fn();
    renderRule({ rule: { ...baseRule, features: ["OV", "IMAX"] }, onChange });
    fireEvent.click(screen.getByRole("button", { name: /remove IMAX/i }));
    expect(onChange).toHaveBeenCalledWith({ features: ["OV"] });
  });

  it("typing in the title input updates titleSubstring", () => {
    const onChange = vi.fn();
    renderRule({ onChange });
    fireEvent.change(screen.getByPlaceholderText(/any title/i), { target: { value: "Odyssey" } });
    expect(onChange).toHaveBeenCalledWith({ titleSubstring: "Odyssey" });
  });

  it("frequency 'never' hides the channel pills", () => {
    renderRule({ rule: { ...baseRule, frequency: "never" } });
    expect(screen.queryByRole("button", { name: /^Email$/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Telegram$/i })).not.toBeInTheDocument();
  });

  it("remove button calls onRemove", () => {
    const onRemove = vi.fn();
    renderRule({ onRemove });
    fireEvent.click(screen.getByRole("button", { name: /remove rule/i }));
    expect(onRemove).toHaveBeenCalled();
  });

  it("up button is disabled for the first rule", () => {
    renderRule({ index: 0, total: 3 });
    expect(screen.getByRole("button", { name: /move up/i })).toBeDisabled();
  });

  it("down button is disabled for the last rule", () => {
    renderRule({ index: 2, total: 3 });
    expect(screen.getByRole("button", { name: /move down/i })).toBeDisabled();
  });

  it("shows the telegram-unverified warning when channel references telegram and unverified", () => {
    renderRule({ rule: { ...baseRule, channel: "telegram" }, telegramUnverified: true });
    expect(screen.getByText(/Telegram not linked/i)).toBeInTheDocument();
  });
});
```

Note: the `any title` placeholder and `Any cinema` label come from existing i18n keys (`preferences.anyTitle`, `preferences.anyCinema`). The frequency labels come from `preferences.frequencies.*`. The channel labels come from `preferences.channelEmail/Telegram/Both` (existing).

- [ ] **Step 3: Run the test to verify it fails**

Run: `cd frontend && npm test -- --run RuleSentence`
Expected: FAIL — module `./RuleSentence` not found.

- [ ] **Step 4: Add frequencyLabel to the shared format module**

The existing `frontend/src/format.ts` already exports `formatShowing` and `formatGeneratedAt` and imports `i18n`. Append the `frequencyLabel` function to it so both `RuleSentence` and `PreferencesPage` can use it. Add the imports at the top of `format.ts` and the function at the bottom:

At the top of `frontend/src/format.ts`, add to the existing imports:
```tsx
import type { TFunction } from "i18next";
import type { NotificationFrequency } from "./types";
```

At the bottom of `frontend/src/format.ts`, append:
```tsx
export function frequencyLabel(t: TFunction, value: NotificationFrequency): string {
  if (value === "never") return t("preferences.frequencies.never");
  if (value === "immediately") return t("preferences.frequencies.immediately");
  return t("preferences.frequencies.days", { count: Number(value) });
}
```

Then in `PreferencesPage.tsx`, delete the local `frequencyLabel` function (lines 8-12 of the current file) and add `import { frequencyLabel } from "../format";` at the top. Verify `npm run build` still compiles (the page still uses the old markup — that's fine for now; Task 5 rewires it).

- [ ] **Step 5: Write the component**

Create `frontend/src/components/RuleSentence.tsx`:

```tsx
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
        <span className="sentence-text">{t("preferences.sentenceShows")}</span>{" "}
        <span className="sentence-text">{t("preferences.sentenceWith")}</span>{" "}
        {rule.features.length === 0 ? (
          <span className="sentence-any">{t("preferences.sentenceAny")}</span>
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
            <span className="sentence-text">{t("preferences.sentenceSend")}</span>{" "}
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
```

- [ ] **Step 6: Run the test to verify it passes**

Run: `cd frontend && npm test -- --run RuleSentence`
Expected: 10 tests pass.

- [ ] **Step 7: Run full build to confirm PreferencesPage still compiles**

Run: `cd frontend && npm run build`
Expected: build succeeds (the page still imports the old inline markup — but since `frequencyLabel` was moved to `format.ts` and re-imported, the page compiles unchanged in behavior).

- [ ] **Step 8: Commit**

```bash
git add frontend/src/components/RuleSentence.tsx frontend/src/components/RuleSentence.test.tsx frontend/src/locales/en.json frontend/src/locales/de.json frontend/src/format.ts frontend/src/pages/PreferencesPage.tsx
git commit -m "feat: RuleSentence editable sentence component"
```

---

### Task 5: PreferencesPage integration

Rewire `PreferencesPage.tsx` to use `AddRuleChoice` + `RuleSentence`. Change `addRule` to take a rule argument. Add `moveRule`. Rewrite the existing `PreferencesPage.test.tsx` for the new structure.

**Files:**
- Modify: `frontend/src/pages/PreferencesPage.tsx`
- Modify: `frontend/src/pages/PreferencesPage.test.tsx`
- Modify: `frontend/src/locales/en.json` (update `rulesDesc`)
- Modify: `frontend/src/locales/de.json` (update `rulesDesc`)

**Interfaces:**
- Consumes: `RuleSentence` (Task 4), `AddRuleChoice` + `DEFAULT_RULE` + `TEMPLATES` (Task 3), `frequencyLabel` from `../format`.
- Produces: the final `PreferencesPage` using the sentence UI. `addRule(rule: NotificationRule)` sets `position: rules.length` and appends. `moveRule(i, dir)` swaps adjacent rules and re-numbers positions.

- [ ] **Step 1: Update rulesDesc i18n**

In `frontend/src/locales/en.json`, replace the `rulesDesc` value:
```json
    "rulesDesc": "First matching rule wins (top to bottom). Use the arrows to reorder.",
```

In `frontend/src/locales/de.json`, replace the `rulesDesc` value:
```json
    "rulesDesc": "Die erste passende Regel gewinnt (von oben nach unten). Pfeile zum Sortieren.",
```

- [ ] **Step 2: Write the failing test (rewrite the existing test)**

Replace `frontend/src/pages/PreferencesPage.test.tsx`:

```tsx
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter } from "react-router-dom";
import i18n from "../i18n";
import { AuthProvider } from "../hooks/useAuth";
import { PreferencesPage } from "./PreferencesPage";
import type { NotificationPreferences } from "../types";

const mockPrefs: NotificationPreferences = {
  telegramHandle: "",
  telegramVerified: false,
  digestAnchor: "2026-08-09T09:00:00+02:00",
  digestHour: 9,
};

function mockFetch(prefs: NotificationPreferences | Error) {
  vi.stubGlobal("fetch", vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    if (url.startsWith("/api/auth/me")) return { ok: true, json: async () => ({ id: 1, email: "a@b.c" }) };
    if (url.startsWith("/api/auth/providers")) return { ok: true, json: async () => ({ email: true, google: true, github: true, dev: false }) };
    if (url.startsWith("/api/preferences/rules")) {
      if (init && init.method === "PUT") return { ok: true, json: async () => ({ rules: JSON.parse(String(init.body)).rules, cinemas: [{ id: 1, name: "Cineplexx Linz" }, { id: 2, name: "Megaplex PlusCity" }] }) };
      return { ok: true, json: async () => ({ rules: [], cinemas: [{ id: 1, name: "Cineplexx Linz" }, { id: 2, name: "Megaplex PlusCity" }] }) };
    }
    if (url.startsWith("/api/preferences")) {
      if (init && init.method === "PUT") return { ok: true, json: async () => JSON.parse(String(init.body)) };
      if (prefs instanceof Error) return { ok: false, status: 500 };
      return { ok: true, json: async () => prefs };
    }
    return { ok: false, status: 404 };
  }));
}

function renderPage() {
  return render(
    <MemoryRouter>
      <AuthProvider>
        <PreferencesPage />
      </AuthProvider>
    </MemoryRouter>
  );
}

afterEach(() => vi.unstubAllGlobals());
beforeEach(() => i18n.changeLanguage("en"));

describe("PreferencesPage", () => {
  it("shows a loading state while preferences are being fetched", async () => {
    mockFetch(mockPrefs);
    renderPage();
    expect(screen.getByText("Loading…")).toBeInTheDocument();
    expect(await screen.findByRole("heading", { name: "Notification preferences" })).toBeInTheDocument();
  });

  it("renders the Telegram handle input bound to the fetched handle", async () => {
    mockFetch({ ...mockPrefs, telegramHandle: "myhandle" });
    renderPage();
    await screen.findByRole("heading", { name: "Notification preferences" });
    expect(screen.getByLabelText("Telegram handle")).toHaveValue("myhandle");
  });

  it("starts a new rule, toggles Telegram off, and saves the mapped channels", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      if (url.startsWith("/api/auth/me")) return { ok: true, json: async () => ({ id: 1, email: "a@b.c" }) };
      if (url.startsWith("/api/auth/providers")) return { ok: true, json: async () => ({ email: true, google: true, github: true, dev: false }) };
      if (url.startsWith("/api/preferences/rules")) {
        if (init && init.method === "PUT") return { ok: true, json: async () => ({ rules: JSON.parse(String(init.body)).rules, cinemas: [{ id: 1, name: "Cineplexx Linz" }, { id: 2, name: "Megaplex PlusCity" }] }) };
        return { ok: true, json: async () => ({ rules: [], cinemas: [{ id: 1, name: "Cineplexx Linz" }, { id: 2, name: "Megaplex PlusCity" }] }) };
      }
      if (url.startsWith("/api/preferences")) return { ok: true, json: async () => mockPrefs };
      return { ok: false, status: 404 };
    });
    vi.stubGlobal("fetch", fetchMock);
    renderPage();
    await screen.findByRole("heading", { name: "Notification preferences" });
    fireEvent.click(screen.getByRole("button", { name: /Add rule/i }));
    fireEvent.click(screen.getByText("Start new"));
    const telegramPill = await screen.findByRole("button", { name: /^Telegram$/i });
    fireEvent.click(telegramPill);
    fireEvent.click(screen.getByRole("button", { name: /Save rules/i }));
    await waitFor(() => {
      const put = fetchMock.mock.calls.find(([u, i]) => String(u).startsWith("/api/preferences/rules") && i && i.method === "PUT");
      expect(put).toBeDefined();
      const body = JSON.parse(String(put![1]!.body));
      expect(body.rules[0].channels).toEqual(["email"]);
    });
  });

  it("shows the loadError text when fetching preferences fails", async () => {
    mockFetch(new Error("boom"));
    renderPage();
    expect(await screen.findByText("Could not load preferences.")).toBeInTheDocument();
  });
});
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cd frontend && npm test -- --run PreferencesPage`
Expected: FAIL — the page still uses the old markup; the "Start new" text and Telegram pill don't exist.

- [ ] **Step 4: Rewire PreferencesPage.tsx**

Rewrite `frontend/src/pages/PreferencesPage.tsx`. Remove the inline rule markup, the local `frequencyLabel`, the `FEATURES`/`FREQUENCY_OPTIONS`/`NotificationChannel` imports (now used inside the components). Import `AddRuleChoice`, `RuleSentence`, and `frequencyLabel` from `../format`. Add `moveRule`. Change `addRule` to take a rule.

```tsx
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
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cd frontend && npm test -- --run PreferencesPage`
Expected: 4 tests pass.

- [ ] **Step 6: Run the full test suite + build**

Run: `cd frontend && npm test -- --run && npm run build`
Expected: all tests pass; build succeeds. (The UI is still visually unstyled — Task 6 adds CSS.)

- [ ] **Step 7: Commit**

```bash
git add frontend/src/pages/PreferencesPage.tsx frontend/src/pages/PreferencesPage.test.tsx frontend/src/locales/en.json frontend/src/locales/de.json
git commit -m "feat: rewire PreferencesPage to sentence UI + AddRuleChoice"
```

---

### Task 6: CSS for the sentence UI

Add the `.sentence`/`.pill`/`.popover` classes and remove the old `.rule-row`/`.chip`/`.rule-remove`/`.rule-features` classes. Pure CSS — no logic change.

**Files:**
- Modify: `frontend/src/index.css`

**Interfaces:**
- Consumes: class names emitted by Tasks 1-5: `.pill-wrap`, `.pill`, `.pill-on`, `.pill-drop`, `.pill-add`, `.pill-feature`, `.dropdown`, `.popover`, `.sentence`, `.sentence-text`, `.sentence-any`, `.sentence-title`, `.rule-actions`, `.rule-move`, `.rule-warn`, `.add-rule-choice`, `.choice-card`, `.choice-desc`, `.template-list`, `.template-card`, `.template-summary`. Also the existing `.pref-card`, `.pref-input`, `.rule-warn` (exists), `.rule-remove` (rename usage).

- [ ] **Step 1: Replace the old rule classes with the sentence classes**

In `frontend/src/index.css`, find the block added in the prior channel-per-rule task:

```css
   .rule-row{display:flex;flex-wrap:wrap;gap:.5rem;align-items:center}
   .rule-row select,.rule-row input{...}
   .rule-row select:focus,.rule-row input:focus{...}
   .rule-remove{...}
   .rule-remove:hover{...}
   .rule-features{...}
   .chip{...}
   .chip:hover{...}
   .chip-on{...}
   .rule-warn{...}
   @media (max-width:560px){...}
```

Replace that entire block with:

```css
   .pill-wrap{position:relative;display:inline-block}
   .pill{
    display:inline-flex;align-items:center;gap:.2rem;
    background:var(--panel);border:1px solid var(--edge);color:var(--dim);
    border-radius:6px;padding:.25rem .6rem;font-size:.78rem;cursor:pointer;
    min-height:40px;transition:background .12s ease,border-color .12s ease,color .12s ease;
   }
   .pill:hover{color:var(--text);border-color:var(--gold)}
   .pill:disabled{opacity:.5;cursor:not-allowed}
   .pill-on{background:rgba(232,179,77,.18);border-color:var(--gold);color:var(--gold-bright)}
   .pill-drop{border-radius:6px}
   .pill-add{border-style:dashed}
   .pill-feature{border-radius:999px}
   .dropdown{
    position:absolute;top:calc(100% + .25rem);left:0;z-index:20;
    background:var(--panel);border:1px solid var(--edge);border-radius:8px;
    padding:.4rem;display:flex;flex-wrap:wrap;gap:.3rem;min-width:180px;
    box-shadow:0 8px 24px rgba(0,0,0,.6);
   }
   .popover{
    position:absolute;top:calc(100% + .25rem);left:0;z-index:20;
    background:var(--panel);border:1px solid var(--gold);border-radius:8px;
    padding:.6rem;display:flex;flex-wrap:wrap;gap:.3rem;width:240px;
    box-shadow:0 8px 24px rgba(0,0,0,.6);
   }
   .sentence{
    font-size:.9rem;line-height:2.4;color:var(--text);
    display:flex;flex-wrap:wrap;align-items:center;gap:.15rem;
   }
   .sentence-text{color:var(--text)}
   .sentence-any{color:var(--faint);font-size:.8rem}
   .sentence-title{width:auto;min-width:120px}
   .rule-actions{display:inline-flex;gap:.3rem;margin-left:auto;align-items:center}
   .rule-remove,.rule-move{
    background:transparent;border:1px solid var(--edge);color:var(--dim);
    border-radius:4px;padding:.15rem .45rem;font-size:.75rem;cursor:pointer;
    min-height:40px;min-width:40px;
   }
   .rule-remove{color:var(--err)}
   .rule-remove:hover,.rule-move:hover{border-color:var(--gold);color:var(--gold)}
   .rule-warn{color:var(--err);font-size:.7rem;margin-left:.4rem}
   .add-rule-choice{display:flex;gap:1rem;flex-wrap:wrap}
   .choice-card{display:flex;flex-direction:column;gap:.2rem;cursor:pointer;flex:1;min-width:200px}
   .choice-card:hover{border-color:var(--gold)}
   .choice-desc{color:var(--dim);font-size:.75rem}
   .template-list{display:flex;flex-direction:column;gap:.4rem}
   .template-card{display:flex;flex-direction:column;gap:.15rem;cursor:pointer;text-align:left}
   .template-card:hover{border-color:var(--gold)}
   .template-summary{color:var(--dim);font-size:.72rem}
   @media (max-width:560px){
    .sentence{font-size:.85rem;line-height:2.2}
    .popover{width:calc(100vw - 3rem)}
    .rule-actions{margin-left:0;width:100%;justify-content:flex-end}
   }
```

Keep the existing `.rule-warn` definition if it already exists elsewhere — but since the replaced block contained it, the new block redefines it. That's fine.

- [ ] **Step 2: Run build to verify CSS compiles**

Run: `cd frontend && npm run build`
Expected: build succeeds.

- [ ] **Step 3: Run tests to verify no regressions**

Run: `cd frontend && npm test -- --run`
Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/index.css
git commit -m "style: sentence-builder pills, popover, responsive layout"
```

---

## Verification

After all tasks:

- `cd frontend && npm test -- --run && npm run build`.
- `cd backend && cargo fmt --check && cargo clippy -- -D warnings && cargo test` (unchanged — should still be green).
- Manual: open Preferences, click "+ Regel hinzufügen" → see the two-card choice → pick "Aus Vorlage" → pick a template → see the sentence → toggle a feature, toggle a channel, type a title, reorder with arrows, save.
