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
