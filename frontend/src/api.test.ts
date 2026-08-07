import { afterEach, describe, expect, it, vi } from "vitest";
import { fetchLoginStatus, fetchShowings } from "./api";

afterEach(() => vi.unstubAllGlobals());

describe("fetchShowings", () => {
  it("returns the parsed payload", async () => {
    const payload = { generatedAt: "2026-08-02T12:00:00+02:00", sources: { cineplexx: "ok" }, cinemas: [] };
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({ ok: true, json: async () => payload }))
    );
    await expect(fetchShowings()).resolves.toEqual(payload);
    expect(fetch).toHaveBeenCalledWith("/api/showings");
  });

  it("throws on http errors", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => ({ ok: false, status: 500 })));
    await expect(fetchShowings()).rejects.toThrow("500");
  });
});

describe("fetchLoginStatus", () => {
  it("returns the loggedIn flag", async () => {
    const fetchMock = vi.fn(async () => ({ ok: true, json: async () => ({ loggedIn: true }) }));
    vi.stubGlobal("fetch", fetchMock);
    await expect(fetchLoginStatus()).resolves.toBe(true);
    expect(fetchMock).toHaveBeenCalledWith("/api/auth/login/status");
  });
});
