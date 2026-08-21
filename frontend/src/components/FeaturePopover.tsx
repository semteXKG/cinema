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
