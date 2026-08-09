import { render, screen, fireEvent, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter } from "react-router-dom";
import i18n from "../i18n";
import { AuthProvider } from "../hooks/useAuth";
import { PreferencesPage } from "./PreferencesPage";

function mockAuthFetch() {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.startsWith("/api/auth/me")) {
        return { ok: true, json: async () => ({ id: 1, email: "a@b.c" }) };
      }
      if (url.startsWith("/api/auth/providers")) {
        return { ok: true, json: async () => ({ email: true, google: true, github: true }) };
      }
      return { ok: false, status: 404 };
    })
  );
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
  it("renders both channels with default frequencies", async () => {
    mockAuthFetch();
    renderPage();
    expect(
      await screen.findByRole("heading", { name: "Notification preferences" })
    ).toBeInTheDocument();
    const email = screen.getByLabelText("Email");
    const telegram = screen.getByLabelText("Telegram");
    expect(email).toHaveValue("immediately");
    expect(telegram).toHaveValue("never");
  });

  it("updates a channel frequency when the select changes", async () => {
    mockAuthFetch();
    renderPage();
    await screen.findByRole("heading", { name: "Notification preferences" });
    const email = screen.getByLabelText("Email");
    fireEvent.change(email, { target: { value: "3" } });
    expect(email).toHaveValue("3");
    expect(within(email).getByRole("option", { name: "3 days" })).toBeInTheDocument();
  });

  it("shows a saved confirmation when Save is clicked", async () => {
    mockAuthFetch();
    renderPage();
    await screen.findByRole("heading", { name: "Notification preferences" });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText("Saved")).toBeInTheDocument();
  });
});
