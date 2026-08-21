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
