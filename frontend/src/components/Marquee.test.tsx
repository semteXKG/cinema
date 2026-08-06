import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { MemoryRouter } from "react-router-dom";
import { Marquee } from "./Marquee";
import { AuthProvider } from "../hooks/useAuth";
import i18n from "../i18n";
import * as api from "../api";

vi.mock("../api");

const mockFetchMe = vi.mocked(api.fetchMe);
const mockFetchProviders = vi.mocked(api.fetchProviders);
const mockLogout = vi.mocked(api.logout);

function renderMarquee() {
  return render(
    <MemoryRouter>
      <AuthProvider>
        <Marquee />
      </AuthProvider>
    </MemoryRouter>
  );
}

describe("Marquee auth", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    i18n.changeLanguage("en");
  });

  it("shows sign in button when not authenticated", async () => {
    mockFetchMe.mockRejectedValue(new Error("not auth"));
    mockFetchProviders.mockResolvedValue({
      email: true,
      google: true,
      apple: false,
      github: false,
    });
    renderMarquee();
    await waitFor(() => {
      expect(screen.getByText("Sign in")).toBeDefined();
    });
  });

  it("shows login panel with email and Google SSO buttons", async () => {
    mockFetchMe.mockRejectedValue(new Error("not auth"));
    mockFetchProviders.mockResolvedValue({
      email: true,
      google: true,
      apple: false,
      github: false,
    });
    renderMarquee();
    await waitFor(() => {
      expect(screen.getByText("Sign in")).toBeDefined();
    });
    fireEvent.click(screen.getByText("Sign in"));
    await waitFor(() => {
      expect(screen.getByPlaceholderText("your@email.com")).toBeDefined();
      expect(screen.getByText("Sign in with Google")).toBeDefined();
    });
    expect(screen.queryByText("Sign in with Apple")).toBeNull();
  });

  it("shows sign out when authenticated", async () => {
    mockFetchMe.mockResolvedValue({ id: 1, email: "a@b.com" });
    mockFetchProviders.mockResolvedValue({
      email: true,
      google: false,
      apple: false,
      github: false,
    });
    renderMarquee();
    await waitFor(() => {
      expect(screen.getByText("Sign out")).toBeDefined();
    });
  });

  it("calls logout API on sign out click", async () => {
    mockFetchMe.mockResolvedValue({ id: 1, email: "a@b.com" });
    mockFetchProviders.mockResolvedValue({
      email: true,
      google: false,
      apple: false,
      github: false,
    });
    renderMarquee();
    await waitFor(() => {
      expect(screen.getByText("Sign out")).toBeDefined();
    });
    fireEvent.click(screen.getByText("Sign out"));
    await waitFor(() => {
      expect(mockLogout).toHaveBeenCalled();
    });
  });
});
