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
    expect(screen.getByText(/OV/)).toBeInTheDocument();
    expect(screen.getByText("IMAX")).toBeInTheDocument();
    expect(screen.getByText("4DX")).toBeInTheDocument();
    expect(screen.getByText(/OV/)).toHaveClass("pill-on");
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
