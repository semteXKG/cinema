import { render, screen, fireEvent, act } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { MemoryRouter } from "react-router-dom";
import { LoginModal } from "./LoginModal";
import { AuthProvider } from "../hooks/useAuth";
import i18n from "../i18n";
import * as api from "../api";

vi.mock("../api");

const mockFetchMe = vi.mocked(api.fetchMe);
const mockFetchProviders = vi.mocked(api.fetchProviders);
const mockSendMagicLink = vi.mocked(api.sendMagicLink);
const mockFetchLoginStatus = vi.mocked(api.fetchLoginStatus);

const onClose = vi.fn();

function renderModal(open = true) {
  return render(
    <MemoryRouter>
      <AuthProvider>
        <LoginModal open={open} onClose={onClose} />
      </AuthProvider>
    </MemoryRouter>
  );
}

describe("LoginModal", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useRealTimers();
    i18n.changeLanguage("en");
    mockFetchMe.mockRejectedValue(new Error("not auth"));
    mockFetchProviders.mockResolvedValue({
      email: true,
      google: true,
      github: true,
    });
    mockSendMagicLink.mockResolvedValue(undefined);
    mockFetchLoginStatus.mockResolvedValue(false);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("renders email form and config-driven SSO buttons", async () => {
    renderModal();
    await act(async () => {});
    expect(screen.getByPlaceholderText("your@email.com")).toBeInTheDocument();
    expect(screen.getByText("Sign in with Google")).toBeInTheDocument();
    expect(screen.getByText("Sign in with GitHub")).toBeInTheDocument();
    expect(screen.queryByText("Sign in with Apple")).toBeNull();
  });

  it("renders nothing when closed", async () => {
    renderModal(false);
    await act(async () => {});
    expect(screen.queryByPlaceholderText("your@email.com")).toBeNull();
  });

  it("closes on overlay click", async () => {
    renderModal();
    await act(async () => {});
    fireEvent.click(screen.getByTestId("modal-overlay"));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("closes on Escape", async () => {
    renderModal();
    await act(async () => {});
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("closes via the close button", async () => {
    renderModal();
    await act(async () => {});
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("shows the waiting state after email submit", async () => {
    vi.useFakeTimers();
    renderModal();
    await act(async () => {});
    fireEvent.change(screen.getByPlaceholderText("your@email.com"), {
      target: { value: "a@b.com" },
    });
    fireEvent.click(screen.getByText("Send link"));
    expect(screen.getByText(/waiting for confirmation/)).toBeInTheDocument();
  });
});
