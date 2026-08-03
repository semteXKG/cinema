import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { MovieCard } from "./MovieCard";
import type { MovieView } from "../types";
import i18n from "../i18n";

const movie: MovieView = {
  title: "The Odyssey",
  badge: "OV",
  metaLine: "Abenteuer, Historie · 180 Min",
  poster: "a1b2.jpg",
  showings: [
    { start: "2026-08-04T19:30:00+02:00", detail: "Saal 7", url: "https://x/1" },
    { start: "2026-08-05T20:15:00+02:00", detail: "", url: "https://x/2" },
  ],
};

describe("MovieCard", () => {
  beforeEach(() => i18n.changeLanguage("en"));

  it("renders title, badge, meta line and poster", () => {
    render(<MovieCard movie={movie} />);
    expect(screen.getByText("The Odyssey")).toBeInTheDocument();
    expect(screen.getByText("OV")).toHaveClass("badge");
    expect(screen.getByText("Abenteuer, Historie · 180 Min")).toHaveClass("filmmeta");
    expect(screen.getByAltText("")).toHaveAttribute("src", "/posters/a1b2.jpg");
  });

  it("renders one link per showing, omitting empty details", () => {
    render(<MovieCard movie={movie} />);
    const links = screen.getAllByRole("link");
    expect(links).toHaveLength(2);
    expect(links[0]).toHaveAttribute("href", "https://x/1");
    expect(links[0].querySelector(".when")?.textContent?.trim()).toBeTruthy();
    expect(links[0]).toHaveTextContent("Saal 7");
    expect(links[1].querySelector(".detail")).toBeNull();
  });

  it("omits badge, meta and poster when absent", () => {
    render(
      <MovieCard movie={{ title: "F1", badge: null, metaLine: "", poster: null, showings: [] }} />
    );
    expect(screen.queryByRole("img")).toBeNull();
    expect(screen.getByText("F1")).toBeInTheDocument();
  });
});
