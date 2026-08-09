import { render, screen, fireEvent } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter } from "react-router-dom";
import App from "./App";
import i18n from "./i18n";

const payload = {
  generatedAt: "2026-08-02T12:00:00+02:00",
  sources: { cineplexx: "ok", megaplex: "error" },
  cinemas: [
    {
      name: "Megaplex PlusCity",
      movies: [
        {
          title: "Die Odyssee",
          badge: "OV",
          metaLine: "Drama · 173 Min",
          poster: null,
          showings: [
            { start: "2026-08-04T19:30:00+02:00", detail: "IMAX 2D", url: "https://x" },
          ],
        },
      ],
    },
  ],
};

function mockFetch(body: unknown, authed = false) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.startsWith("/api/auth/me")) {
        return authed
          ? { ok: true, json: async () => ({ id: 1, email: "a@b.c" }) }
          : { ok: false, status: 401 };
      }
      if (url.startsWith("/api/auth/providers")) {
        return { ok: true, json: async () => ({ email: true, google: true, github: true }) };
      }
      if (url.startsWith("/api/auth")) return { ok: false, status: 401 };
      return { ok: true, json: async () => body };
    })
  );
}

function renderAt(path: string) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <App />
    </MemoryRouter>
  );
}

afterEach(() => vi.unstubAllGlobals());
beforeEach(() => i18n.changeLanguage("en"));

describe("App", () => {
  it("renders the projector logo instead of the clapperboard emoji", async () => {
    mockFetch({ generatedAt: null, sources: {}, cinemas: [] });
    const { container } = renderAt("/");

    await screen.findByRole("button", { name: "Sign in" });
    expect(screen.getByRole("heading", { name: "OV Cinema Linz" })).toBeInTheDocument();
    expect(container.querySelector(".marquee-logo")).toHaveAttribute(
      "src",
      "/projector-logo.svg"
    );
    expect(container.querySelector(".marquee-logo")).toHaveAttribute("alt", "");
    expect(container).not.toHaveTextContent("🎬");
  });

  it("shows 'first check' state when cinemas is null", async () => {
    mockFetch({ generatedAt: null, sources: null, cinemas: null });
    renderAt("/");
    expect(await screen.findByText(/first check is running/)).toBeInTheDocument();
  });

  it("shows the empty state", async () => {
    mockFetch({ generatedAt: null, sources: {}, cinemas: [] });
    renderAt("/");
    expect(await screen.findByText(/No OV showings found/)).toBeInTheDocument();
  });

  it("renders cinemas, footer and source health", async () => {
    mockFetch(payload);
    renderAt("/");
    expect(await screen.findByText("Megaplex PlusCity")).toBeInTheDocument();
    expect(screen.getByText("Die Odyssee")).toBeInTheDocument();
    expect(screen.getByText("error")).toHaveClass("err");
    expect(screen.getByText("ok")).toHaveClass("ok");
    expect(screen.getByText(/Last checked/)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Impressum" })).toBeInTheDocument();
  });

  it("renders the invalid-link page for ?error=invalid_token", async () => {
    mockFetch({ generatedAt: null, sources: {}, cinemas: [] });
    renderAt("/?error=invalid_token");
    expect(
      await screen.findByText(/This sign-in link has expired or was already used/)
    ).toBeInTheDocument();
    expect(screen.queryByText("Impressum")).toBeNull();
  });

  it("hides the Preferences link when logged out", async () => {
    mockFetch({ generatedAt: null, sources: {}, cinemas: [] });
    renderAt("/");
    await screen.findByRole("button", { name: "Sign in" });
    expect(screen.queryByRole("link", { name: "Preferences" })).toBeNull();
  });

  it("shows the Preferences link and page when logged in", async () => {
    mockFetch({ generatedAt: null, sources: {}, cinemas: [] }, true);
    renderAt("/");
    const link = await screen.findByRole("link", { name: "Preferences" });
    expect(link).toHaveAttribute("href", "/preferences");
    fireEvent.click(link);
    expect(
      await screen.findByRole("heading", { name: "Notification preferences" })
    ).toBeInTheDocument();
  });
});
