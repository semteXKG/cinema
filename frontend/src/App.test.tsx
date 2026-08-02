import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import App from "./App";

const payload = {
  generatedAt: "2026-08-02 12:00",
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
          showings: [{ date: "Mo 04.08.", time: "19:30", detail: "IMAX 2D", url: "https://x" }],
        },
      ],
    },
  ],
};

function mockFetch(body: unknown) {
  vi.stubGlobal("fetch", vi.fn(async () => ({ ok: true, json: async () => body })));
}

afterEach(() => vi.unstubAllGlobals());

describe("App", () => {
  it("shows 'first check' state when cinemas is null", async () => {
    mockFetch({ generatedAt: null, sources: null, cinemas: null });
    render(<App />);
    expect(await screen.findByText(/first check is running/)).toBeInTheDocument();
  });

  it("shows the empty state", async () => {
    mockFetch({ generatedAt: "x", sources: {}, cinemas: [] });
    render(<App />);
    expect(await screen.findByText(/No OV showings found/)).toBeInTheDocument();
  });

  it("renders cinemas, footer and source health", async () => {
    mockFetch(payload);
    render(<App />);
    expect(await screen.findByText("Megaplex PlusCity")).toBeInTheDocument();
    expect(screen.getByText("Die Odyssee")).toBeInTheDocument();
    expect(screen.getByText("error")).toHaveClass("err");
    expect(screen.getByText("ok")).toHaveClass("ok");
    expect(screen.getByText(/Last checked: 2026-08-02 12:00/)).toBeInTheDocument();
  });
});
