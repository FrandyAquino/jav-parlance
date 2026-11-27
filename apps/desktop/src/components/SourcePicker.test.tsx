import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { useApp } from "@/store/app";
import { SourcePicker } from "./SourcePicker";

afterEach(cleanup);

it("offers the microphone in the primary source picker", () => {
  useApp.setState({
    sources: [
      { id: "system:default", kind: { type: "systemOutput" }, name: "System audio", isDefault: true },
      { id: "mic:default", kind: { type: "microphone" }, name: "MacBook microphone", isDefault: true },
    ],
    selectedSourceId: "system:default",
    microphoneId: null,
  });
  render(<SourcePicker disabled={false} />);
  fireEvent.keyDown(screen.getByRole("combobox", { name: "Audio source" }), { key: "ArrowDown" });
  expect(screen.getByRole("option", { name: "MacBook microphone" })).toBeInTheDocument();
});

it("hides the extra microphone selector for microphone-only capture", () => {
  useApp.setState({
    sources: [{ id: "mic:default", kind: { type: "microphone" }, name: "MacBook microphone", isDefault: true }],
    selectedSourceId: "mic:default",
    microphoneId: null,
  });
  render(<SourcePicker disabled={false} />);
  expect(screen.getByRole("combobox", { name: "Audio source" })).toHaveTextContent("MacBook microphone");
  expect(screen.queryByRole("combobox", { name: "Include microphone" })).not.toBeInTheDocument();
});
