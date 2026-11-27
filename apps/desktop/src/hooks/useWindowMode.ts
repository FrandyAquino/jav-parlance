import { currentMonitor, getCurrentWindow, LogicalSize, PhysicalPosition } from "@tauri-apps/api/window";
import { useEffect, useRef, useState } from "react";
import { errorMessage } from "@/lib/ipc";
import type { WindowMode } from "@/store/copilot";

const DEFAULT_SIZES = {
  compact: { width: 560, height: 320 },
  expanded: { width: 900, height: 680 },
  reading: { width: 620, height: 440 },
};

/** Serialize native resizes so rapid mode changes cannot restore an obsolete size. */
export function useWindowMode(mode: WindowMode): string | null {
  const sizes = useRef({ ...DEFAULT_SIZES });
  const applied = useRef<WindowMode | null>(null);
  const queue = useRef(Promise.resolve());
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let disposed = false;
    queue.current = queue.current
      .then(async () => {
        if (disposed) return;
        const window = getCurrentWindow();
        const [size, scale, monitor, position] = await Promise.all([
          window.innerSize(),
          window.scaleFactor(),
          currentMonitor(),
          window.outerPosition(),
        ]);
        if (disposed) return;
        if (applied.current) sizes.current[applied.current] = size.toLogical(scale);
        const target = sizes.current[mode];
        const area = monitor?.workArea;
        const width = Math.max(440, Math.min(target.width, area ? area.size.width / scale : target.width));
        const height = Math.max(320, Math.min(target.height, area ? area.size.height / scale : target.height));
        await window.setSize(new LogicalSize(width, height));
        applied.current = mode;
        if (area) {
          const x = Math.max(area.position.x, Math.min(position.x, area.position.x + area.size.width - width * scale));
          const y = Math.max(
            area.position.y,
            Math.min(position.y, area.position.y + area.size.height - height * scale),
          );
          if (x !== position.x || y !== position.y) await window.setPosition(new PhysicalPosition(x, y));
        }
        if (!disposed) setError(null);
      })
      .catch((cause: unknown) => {
        if (!disposed) setError(`Window size could not be adjusted: ${errorMessage(cause)}`);
      });
    return () => {
      disposed = true;
    };
  }, [mode]);
  return error;
}
