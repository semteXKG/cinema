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
      dev: false,
    });
    mockSendMagicLink.mockResolvedValue(undefined);
    mockFetchLoginStatus.mockResolvedValue(false);
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
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

  it("renders provider logos in the SSO buttons", async () => {
    renderModal();
    await act(async () => {});
    const google = screen.getByText("Sign in with Google").closest("button");
    const github = screen.getByText("Sign in with GitHub").closest("button");
    expect(google?.querySelector("svg")).not.toBeNull();
    expect(github?.querySelector("svg")).not.toBeNull();
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

  it("shows the dev login button only when the backend enables it", async () => {
    mockFetchProviders.mockResolvedValue({
      email: true,
      google: true,
      github: true,
      dev: true,
    });
    renderModal();
    await act(async () => {});
    expect(screen.getByText("Dev: sign in as dev@ov.local")).toBeInTheDocument();
  });

  it("hides the dev login button when dev login is disabled", async () => {
    mockFetchProviders.mockResolvedValue({
      email: true,
      google: true,
      github: true,
      dev: false,
    });
    renderModal();
    await act(async () => {});
    expect(screen.queryByText("Dev: sign in as dev@ov.local")).toBeNull();
  });

  it("navigates to the dev-login endpoint when clicked", async () => {
    const locationStub = { href: "" };
    vi.stubGlobal("location", locationStub);
    mockFetchProviders.mockResolvedValue({
      email: true,
      google: true,
      github: true,
      dev: true,
    });
    renderModal();
    await act(async () => {});
    fireEvent.click(screen.getByText("Dev: sign in as dev@ov.local"));
    expect(locationStub.href).toBe("/api/auth/dev-login");
  });
});
