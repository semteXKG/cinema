import { render, screen, fireEvent, within, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter } from "react-router-dom";
import i18n from "../i18n";
import { AuthProvider } from "../hooks/useAuth";
import { PreferencesPage } from "./PreferencesPage";
import type { NotificationPreferences } from "../types";

const mockPrefs: NotificationPreferences = {
  emailFrequency: "immediately",
  telegramFrequency: "never",
  telegramHandle: "",
  telegramVerified: false,
  digestAnchor: "2026-08-09T09:00:00+02:00",
  digestHour: 9,
};

function mockFetch(prefs: NotificationPreferences | Error) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      if (url.startsWith("/api/auth/me")) {
        return { ok: true, json: async () => ({ id: 1, email: "a@b.c" }) };
      }
      if (url.startsWith("/api/auth/providers")) {
        return { ok: true, json: async () => ({ email: true, google: true, github: true, dev: false }) };
      }
      if (url.startsWith("/api/preferences")) {
        if (init?.method === "PUT") {
          return { ok: true, json: async () => JSON.parse(String(init.body)) };
        }
        if (prefs instanceof Error) return { ok: false, status: 500 };
        return { ok: true, json: async () => prefs };
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
  it("shows a loading state while preferences are being fetched", async () => {
    mockFetch(mockPrefs);
    renderPage();
    expect(screen.getByText("Loading…")).toBeInTheDocument();
    expect(await screen.findByRole("heading", { name: "Notification preferences" })).toBeInTheDocument();
  });

  it("renders both channels with frequencies from the fetched preferences", async () => {
    mockFetch(mockPrefs);
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
    mockFetch(mockPrefs);
    renderPage();
    await screen.findByRole("heading", { name: "Notification preferences" });
    const email = screen.getByLabelText("Email");
    fireEvent.change(email, { target: { value: "3" } });
    expect(email).toHaveValue("3");
    expect(within(email).getByRole("option", { name: "3 days" })).toBeInTheDocument();
  });

  it("updates the telegram handle as the user types", async () => {
    mockFetch(mockPrefs);
    renderPage();
    await screen.findByRole("heading", { name: "Notification preferences" });
    const input = screen.getByPlaceholderText("@yourhandle");
    expect(input).toHaveValue("");
    fireEvent.change(input, { target: { value: "@myhandle" } });
    expect(input).toHaveValue("@myhandle");
  });

  it("shows the telegram handle input only in the telegram card", async () => {
    mockFetch(mockPrefs);
    renderPage();
    await screen.findByRole("heading", { name: "Notification preferences" });
    const emailCard = screen.getByRole("heading", { name: "Email" }).closest(".pref-card")! as HTMLElement;
    const telegramCard = screen.getByRole("heading", { name: "Telegram" }).closest(".pref-card")! as HTMLElement;
    expect(within(emailCard).queryByPlaceholderText("@yourhandle")).toBeNull();
    expect(within(telegramCard).getByPlaceholderText("@yourhandle")).toBeInTheDocument();
  });

  it("shows the verified state when telegram is linked", async () => {
    mockFetch({ ...mockPrefs, telegramHandle: "@ov", telegramVerified: true });
    renderPage();
    await screen.findByRole("heading", { name: "Notification preferences" });
    expect(await screen.findByText("Telegram account linked.")).toBeInTheDocument();
  });

  it("shows the verify prompt when telegram is not yet linked", async () => {
    mockFetch({ ...mockPrefs, telegramHandle: "@ov" });
    renderPage();
    await screen.findByRole("heading", { name: "Notification preferences" });
    expect(
      await screen.findByText(/Send any message to @ov_linzz_bot to link your account/)
    ).toBeInTheDocument();
  });

  it("saves preferences via PUT and shows the saved confirmation", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      if (url.startsWith("/api/auth/me")) {
        return { ok: true, json: async () => ({ id: 1, email: "a@b.c" }) };
      }
      if (url.startsWith("/api/auth/providers")) {
        return { ok: true, json: async () => ({ email: true, google: true, github: true, dev: false }) };
      }
      if (url.startsWith("/api/preferences")) {
        if (init?.method === "PUT") {
          return { ok: true, json: async () => JSON.parse(String(init.body)) };
        }
        return { ok: true, json: async () => mockPrefs };
      }
      return { ok: false, status: 404 };
    });
    vi.stubGlobal("fetch", fetchMock);

    renderPage();
    await screen.findByRole("heading", { name: "Notification preferences" });
    const email = screen.getByLabelText("Email");
    fireEvent.change(email, { target: { value: "1" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      const putCall = fetchMock.mock.calls.find(([url, init]) =>
        String(url).startsWith("/api/preferences") && init?.method === "PUT"
      );
      expect(putCall).toBeDefined();
      expect(JSON.parse(String(putCall![1]!.body))).toMatchObject({
        emailFrequency: "1",
        telegramFrequency: "never",
        telegramHandle: "",
        telegramVerified: false,
      });
    });
    expect(await screen.findByText("Saved")).toBeInTheDocument();
  });

  it("shows the loadError text when fetching preferences fails", async () => {
    mockFetch(new Error("boom"));
    renderPage();
    expect(
      await screen.findByText("Could not load preferences.")
    ).toBeInTheDocument();
  });
});
