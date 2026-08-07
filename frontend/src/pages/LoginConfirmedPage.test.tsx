import { render, screen, act } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { MemoryRouter } from "react-router-dom";
import { LoginConfirmedPage } from "./LoginConfirmedPage";
import { AuthProvider } from "../hooks/useAuth";
import i18n from "../i18n";
import * as api from "../api";

vi.mock("../api");

const mockFetchMe = vi.mocked(api.fetchMe);
const mockFetchProviders = vi.mocked(api.fetchProviders);
const mockFetchLoginStatus = vi.mocked(api.fetchLoginStatus);

function renderPage() {
  return render(
    <MemoryRouter>
      <AuthProvider>
        <LoginConfirmedPage />
      </AuthProvider>
    </MemoryRouter>
  );
}

describe("LoginConfirmedPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useRealTimers();
    i18n.changeLanguage("en");
    mockFetchProviders.mockResolvedValue({ email: true, google: false, apple: false, github: false });
  });

  it("shows the brand header and no showings content", async () => {
    mockFetchMe.mockRejectedValue(new Error("not auth"));
    mockFetchLoginStatus.mockResolvedValue(false);
    const { container } = renderPage();
    await screen.findByText(/waiting|Sign-in confirmed|logged in/i);
    expect(screen.getByRole("heading", { name: "OV Cinema Linz" })).toBeInTheDocument();
    expect(container.querySelector(".marquee-logo")).toHaveAttribute("src", "/projector-logo.svg");
    // no showings content
    expect(screen.queryByText("Megaplex PlusCity")).toBeNull();
    expect(screen.queryByText("Impressum")).toBeNull();
  });

  it("shows the other-device message when not logged in", async () => {
    mockFetchMe.mockRejectedValue(new Error("not auth"));
    mockFetchLoginStatus.mockResolvedValue(false);
    renderPage();
    expect(await screen.findByText(/Sign-in confirmed/)).toBeInTheDocument();
  });

  it("shows the logged-in message after the mount poll succeeds", async () => {
    vi.useFakeTimers();
    mockFetchMe
      .mockRejectedValueOnce(new Error("not auth"))
      .mockResolvedValueOnce({ id: 1, email: "a@b.com" });
    mockFetchLoginStatus.mockResolvedValueOnce(false).mockResolvedValueOnce(true);
    renderPage();
    await act(async () => {});
    expect(screen.queryByText(/You have been logged in/)).toBeNull();
    await act(async () => { vi.advanceTimersByTime(1000); });
    await act(async () => { vi.advanceTimersByTime(1000); });
    await act(async () => {});
    await act(async () => {});
    expect(screen.getByText(/You have been logged in/)).toBeInTheDocument();
  });
});
