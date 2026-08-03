import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { MemoryRouter } from "react-router-dom";
import App from "./App";
import i18n from "./i18n";
import { fireEvent } from "@testing-library/react";

function renderAt(path: string) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <App />
    </MemoryRouter>
  );
}

describe("Impressum", () => {
  beforeEach(() => i18n.changeLanguage("en"));

  it("shows operator data at /impressum", () => {
    renderAt("/impressum");
    expect(screen.getByRole("heading", { name: "Impressum" })).toBeInTheDocument();
    expect(screen.getByText(/Klaus Gradinger/)).toBeInTheDocument();
    expect(screen.getByText(/4230 Pregarten/)).toBeInTheDocument();
    expect(screen.getByText(/klaus\.gradinger@gmail\.com/)).toBeInTheDocument();
    expect(screen.getByText(/hobby project/)).toBeInTheDocument();
    expect(screen.getByText(/promptly on short request/)).toBeInTheDocument();
  });

  it("shows the showings page at /", () => {
    renderAt("/");
    expect(screen.getByRole("heading", { name: /OV Cinema Linz/ })).toBeInTheDocument();
  });

  it("switches to German and back", () => {
    renderAt("/impressum");
    fireEvent.click(screen.getByRole("button", { name: "DE" }));
    expect(
      screen.getByText("Privates, nicht-kommerzielles Hobbyprojekt.")
    ).toBeInTheDocument();
    expect(screen.getByText(/kurze Anfrage/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "EN" }));
    expect(screen.getByText(/non-commercial hobby project/)).toBeInTheDocument();
  });
});
