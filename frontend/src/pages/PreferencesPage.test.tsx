import { render, screen, fireEvent, within, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter } from "react-router-dom";
import i18n from "../i18n";
import { AuthProvider } from "../hooks/useAuth";
import { PreferencesPage } from "./PreferencesPage";
import type { NotificationPreferences } from "../types";

const mockPrefs: NotificationPreferences = {
  emailEnabled: true,
  telegramEnabled: false,
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
      if (init && init.method === "PUT") return { ok: true, json: async () => JSON.parse(String(init.body)) };
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

  it("renders both channels with enable toggles from the fetched preferences", async () => {
    mockFetch(mockPrefs);
    renderPage();
    await screen.findByRole("heading", { name: "Notification preferences" });
    expect(screen.getByLabelText("Email")).toBeChecked();
    expect(screen.getByLabelText("Telegram")).not.toBeChecked();
  });

  it("adds a rule, sets frequency, and saves the ordered list", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      if (url.startsWith("/api/auth/me")) return { ok: true, json: async () => ({ id: 1, email: "a@b.c" }) };
      if (url.startsWith("/api/auth/providers")) return { ok: true, json: async () => ({ email: true, google: true, github: true, dev: false }) };
      if (url.startsWith("/api/preferences/rules")) {
        if (init && init.method === "PUT") return { ok: true, json: async () => JSON.parse(String(init.body)) };
        return { ok: true, json: async () => ({ rules: [], cinemas: [{ id: 1, name: "Cineplexx Linz" }, { id: 2, name: "Megaplex PlusCity" }] }) };
      }
      if (url.startsWith("/api/preferences")) {
        if (init && init.method === "PUT") return { ok: true, json: async () => JSON.parse(String(init.body)) };
        return { ok: true, json: async () => mockPrefs };
      }
      return { ok: false, status: 404 };
    });
    vi.stubGlobal("fetch", fetchMock);
    renderPage();
    await screen.findByRole("heading", { name: "Notification preferences" });
    fireEvent.click(screen.getByRole("button", { name: "Add rule" }));
    const freq = await screen.findByLabelText("Rule 1 frequency");
    fireEvent.change(freq, { target: { value: "immediately" } });
    fireEvent.click(screen.getByRole("button", { name: "Save rules" }));
    await waitFor(() => {
      const put = fetchMock.mock.calls.find(([u, i]) => String(u).startsWith("/api/preferences/rules") && i && i.method === "PUT");
      expect(put).toBeDefined();
      const body = JSON.parse(String(put![1]!.body));
      expect(body.rules[0].frequency).toBe("immediately");
    });
  });

  it("shows the loadError text when fetching preferences fails", async () => {
    mockFetch(new Error("boom"));
    renderPage();
    expect(
      await screen.findByText("Could not load preferences.")
    ).toBeInTheDocument();
  });
});