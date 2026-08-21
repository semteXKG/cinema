import { render, screen, fireEvent } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { PillDropdown } from "./PillDropdown";

const opts = [
  { value: "a", label: "Alpha" },
  { value: "b", label: "Beta" },
];

describe("PillDropdown", () => {
  it("shows the current value's label and opens on click", () => {
    const onChange = vi.fn();
    render(<PillDropdown value="a" options={opts} onChange={onChange} ariaLabel="pick" />);
    expect(screen.getByRole("button", { name: "pick" })).toHaveTextContent("Alpha");
    expect(screen.queryByText("Beta")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "pick" }));
    expect(screen.getByText("Beta")).toBeInTheDocument();
  });

  it("selects an option and closes", () => {
    const onChange = vi.fn();
    render(<PillDropdown value="a" options={opts} onChange={onChange} ariaLabel="pick" />);
    fireEvent.click(screen.getByRole("button", { name: "pick" }));
    fireEvent.click(screen.getByText("Beta"));
    expect(onChange).toHaveBeenCalledWith("b");
    expect(screen.queryByText("Alpha")).not.toBeInTheDocument();
  });

  it("closes on outside click without selecting", () => {
    const onChange = vi.fn();
    render(
      <div>
        <PillDropdown value="a" options={opts} onChange={onChange} ariaLabel="pick" />
        <div data-testid="outside">outside</div>
      </div>
    );
    fireEvent.click(screen.getByRole("button", { name: "pick" }));
    fireEvent.mouseDown(screen.getByTestId("outside"));
    expect(screen.queryByText("Beta")).not.toBeInTheDocument();
    expect(onChange).not.toHaveBeenCalled();
  });

  it("closes on Escape without selecting", () => {
    const onChange = vi.fn();
    render(<PillDropdown value="a" options={opts} onChange={onChange} ariaLabel="pick" />);
    fireEvent.click(screen.getByRole("button", { name: "pick" }));
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(screen.queryByText("Beta")).not.toBeInTheDocument();
    expect(onChange).not.toHaveBeenCalled();
  });
});
