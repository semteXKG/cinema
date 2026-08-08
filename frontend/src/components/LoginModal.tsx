import { useEffect, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { useAuth } from "../hooks/useAuth";

interface LoginModalProps {
  open: boolean;
  onClose: () => void;
}

export function LoginModal({ open, onClose }: LoginModalProps) {
  const { t } = useTranslation();
  const { user, providers, loginEmail, loginSSO } = useAuth();
  const [emailInput, setEmailInput] = useState("");
  const [sending, setSending] = useState(false);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  useEffect(() => {
    if (open && user) onClose();
  }, [open, user, onClose]);

  if (!open) return null;

  const handleEmailSubmit = async (e: FormEvent) => {
    e.preventDefault();
    if (!emailInput.trim() || sending) return;
    setSending(true);
    try {
      await loginEmail(emailInput.trim());
    } finally {
      setSending(false);
    }
  };

  return (
    <div className="modal-overlay" data-testid="modal-overlay" onClick={onClose}>
      <div className="modal" role="dialog" aria-modal="true" onClick={(e) => e.stopPropagation()}>
        <button className="modal-close" aria-label="Close" onClick={onClose}>
          ×
        </button>
        <h2 className="modal-title">{t("auth.signIn")}</h2>
        {providers?.email && (
          <form onSubmit={handleEmailSubmit}>
            <input
              className="auth-input"
              type="email"
              placeholder={t("auth.emailPlaceholder")}
              value={emailInput}
              onChange={(e) => setEmailInput(e.target.value)}
              disabled={sending}
            />
            <button className="auth-submit" type="submit" disabled={sending}>
              {sending ? t("auth.waiting") : t("auth.sendLink")}
            </button>
          </form>
        )}
        <div className="modal-divider">{t("auth.or")}</div>
        {providers?.google && (
          <button className="auth-sso" onClick={() => loginSSO("google")}>
            {t("auth.signInWith", { provider: "Google" })}
          </button>
        )}
        {providers?.github && (
          <button className="auth-sso" onClick={() => loginSSO("github")}>
            {t("auth.signInWith", { provider: "GitHub" })}
          </button>
        )}
      </div>
    </div>
  );
}
