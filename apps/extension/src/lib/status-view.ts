import type { CaptureStatus } from "./messages";

export interface StatusView {
  message: string;
  buttonLabel: string;
  active: boolean;
  busy: boolean;
}

export function statusView(status: CaptureStatus): StatusView {
  switch (status.state) {
    case "idle":
      return {
        message: "Ready to send this tab's audio to Parlance.",
        buttonLabel: "Start capture",
        active: false,
        busy: false,
      };
    case "starting":
      return { message: "Connecting to Parlance…", buttonLabel: "Starting…", active: false, busy: true };
    case "capturing":
      return { message: `Capturing “${status.tabTitle}”.`, buttonLabel: "Stop capture", active: true, busy: false };
    case "error":
      return { message: status.reason, buttonLabel: "Try again", active: false, busy: false };
  }
}
