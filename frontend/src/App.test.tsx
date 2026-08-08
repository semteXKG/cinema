import { render, screen } from "@testing-library/react";
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

function mockFetch(body: unknown) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      if (String(input).startsWith("/api/auth")) return { ok: false, status: 401 };
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
});
