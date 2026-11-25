import { type CaptureStatus, type PopupRequest, isStatusUpdate } from "./lib/messages";
import { statusView } from "./lib/status-view";

const toggle = element<HTMLButtonElement>("toggle");
const statusText = element<HTMLParagraphElement>("status");
const dot = document.querySelector<HTMLElement>(".dot");

let current: CaptureStatus = { state: "idle" };

toggle.addEventListener("click", () => {
  void request(current.state === "capturing" ? "stop" : "start");
});

chrome.runtime.onMessage.addListener((message: unknown) => {
  if (isStatusUpdate(message)) render(message.status);
  return false;
});

void request("status");

async function request(kind: PopupRequest["kind"]): Promise<void> {
  const message: PopupRequest = { target: "background", kind };
  render(await chrome.runtime.sendMessage<PopupRequest, CaptureStatus>(message));
}

function render(status: CaptureStatus): void {
  current = status;
  const view = statusView(status);
  statusText.textContent = view.message;
  statusText.dataset["state"] = status.state;
  toggle.textContent = view.buttonLabel;
  toggle.disabled = view.busy;
  toggle.dataset["active"] = String(view.active);
  toggle.setAttribute("aria-busy", String(view.busy));
  if (dot) dot.dataset["state"] = status.state;
}

function element<T extends HTMLElement>(id: string): T {
  const found = document.getElementById(id);
  if (!found) throw new Error(`popup markup is missing #${id}`);
  return found as T;
}
