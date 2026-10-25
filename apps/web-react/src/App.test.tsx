import { afterEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { App } from "./App.tsx";

afterEach(cleanup);

function setup() {
  render(<App />);
}

describe("web-react baseline", () => {
  it("renders the dashboard", () => {
    setup();
    expect(screen.getByText("What if the web had no legacy?")).toBeTruthy();
    expect(screen.getByText("Local time")).toBeTruthy();
  });

  it("counts up and down", () => {
    setup();
    fireEvent.click(screen.getByText("+1"));
    fireEvent.click(screen.getByText("+1"));
    fireEvent.click(screen.getByText("+1"));
    fireEvent.click(screen.getByText("−1"));
    expect(screen.getByText("double = 4")).toBeTruthy();
    fireEvent.click(screen.getByText("Reset"));
    expect(screen.getByText("double = 0")).toBeTruthy();
  });

  it("adds and toggles todos", () => {
    setup();
    const input = screen.getByPlaceholderText("What needs to be done?");
    fireEvent.change(input, { target: { value: "Write tests" } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(screen.getByText("Write tests")).toBeTruthy();
    expect(screen.getByText(/3 left/)).toBeTruthy();
    expect((input as HTMLInputElement).value).toBe("");
    fireEvent.click(screen.getByText("Write tests"));
    expect(screen.getByText(/2 left/)).toBeTruthy();
  });

  it("ignores empty todos", () => {
    setup();
    fireEvent.click(screen.getByText("Add"));
    expect(screen.getByText(/2 left/)).toBeTruthy();
  });

  it("switches theme", () => {
    const { container } = render(<App />);
    expect(container.querySelector(".app.dark")).toBeTruthy();
    fireEvent.click(screen.getByText("☀ Light"));
    expect(container.querySelector(".app.dark")).toBeNull();
    expect(screen.getByText("☾ Dark")).toBeTruthy();
  });

  it("runs the benchmark", () => {
    const { container } = render(<App />);
    fireEvent.click(within(container.querySelector(".tabs")!).getByText("Benchmark"));
    expect(container.querySelectorAll(".cell")).toHaveLength(1000);
    fireEvent.click(screen.getByText("Update 10%"));
    expect(screen.getByText(/^1 round, 10%: [\d.]+ ms/)).toBeTruthy();
  });
});
