import { render, screen } from "@testing-library/react";
import { describe, it, expect, beforeEach } from "vitest";
import { InvalidLinkPage } from "./InvalidLinkPage";
import i18n from "../i18n";

function renderPage() {
  return render(<InvalidLinkPage />);
}

describe("InvalidLinkPage", () => {
  beforeEach(() => i18n.changeLanguage("en"));

  it("shows the brand header and the invalid-link message", () => {
    const { container } = renderPage();
    expect(screen.getByRole("heading", { name: "OV Cinema Linz" })).toBeInTheDocument();
    expect(container.querySelector(".marquee-logo")).toHaveAttribute("src", "/projector-logo.svg");
    expect(
      screen.getByText(/This sign-in link has expired or was already used/)
    ).toBeInTheDocument();
  });

  it("shows no showings content and no action button", () => {
    const { container } = renderPage();
    expect(screen.queryByText("Megaplex PlusCity")).toBeNull();
    expect(screen.queryByText("Impressum")).toBeNull();
    expect(screen.queryByRole("button")).toBeNull();
    expect(container).not.toHaveTextContent(/request a new link/i);
  });
});
