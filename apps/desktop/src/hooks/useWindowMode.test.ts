import { renderHook, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import type { WindowMode } from "@/store/copilot";
import { useWindowMode } from "./useWindowMode";

const native = vi.hoisted(() => ({
  size: { width: 560, height: 320 },
  setSize: vi.fn(),
  setPosition: vi.fn(),
  innerSize: vi.fn(),
  scaleFactor: vi.fn(),
  outerPosition: vi.fn(),
  currentMonitor: vi.fn(),
}));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => native,
  currentMonitor: native.currentMonitor,
  LogicalSize: class {
    constructor(
      public width: number,
      public height: number,
    ) {}
  },
  PhysicalPosition: class {
    constructor(
      public x: number,
      public y: number,
    ) {}
  },
}));

beforeEach(() => {
  vi.clearAllMocks();
  native.size = { width: 560, height: 320 };
  native.innerSize.mockImplementation(async () => ({
    toLogical: () => ({ ...native.size }),
  }));
  native.scaleFactor.mockResolvedValue(1);
  native.outerPosition.mockResolvedValue({ x: 50, y: 50 });
  native.currentMonitor.mockResolvedValue({
    scaleFactor: 1,
    workArea: { position: { x: 0, y: 0 }, size: { width: 1280, height: 800 } },
  });
  native.setSize.mockImplementation(async (size: { width: number; height: number }) => {
    native.size = size;
  });
  native.setPosition.mockResolvedValue(undefined);
});

it("restores the user's size for each mode and keeps the window on screen", async () => {
  const { rerender, unmount } = renderHook(({ mode }: { mode: WindowMode }) => useWindowMode(mode), {
    initialProps: { mode: "compact" as WindowMode },
  });
  await waitFor(() => expect(native.setSize).toHaveBeenCalledTimes(1));
  native.size = { width: 600, height: 350 };
  rerender({ mode: "expanded" });
  await waitFor(() => expect(native.size).toMatchObject({ width: 900, height: 680 }));
  rerender({ mode: "compact" });
  await waitFor(() => expect(native.size).toMatchObject({ width: 600, height: 350 }));
  unmount();
});

it("reports native failure while permitting the presentation mode to change", async () => {
  native.setSize.mockRejectedValue(new Error("Window unavailable"));
  const { result, unmount } = renderHook(() => useWindowMode("expanded"));
  await waitFor(() => expect(result.current).toContain("Window unavailable"));
  unmount();
});
