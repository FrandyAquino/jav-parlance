import { PROTOCOL_VERSION } from "./constants";
import type { HostMessage } from "./wire";

export function describeError(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Why the handshake failed, given the host's first reply (`null` if unparseable). */
export function handshakeFailureReason(reply: HostMessage | null): string {
  switch (reply?.type) {
    case "unavailable":
      return reply.reason;
    case "error":
      return `Parlance's browser bridge reported an error: ${reply.message}`;
    case "ready":
      return `Parlance's browser bridge speaks protocol ${reply.protocolVersion}, this extension speaks ${PROTOCOL_VERSION}. Update both to the same release.`;
    default:
      return "Parlance's browser bridge sent an unexpected reply.";
  }
}

/** Turns Chrome's terse native messaging errors into something the user can act on. */
export function disconnectReason(lastError: string | undefined, extensionId: string): string {
  const install = `run \`parlance-native-host --install ${extensionId}\``;
  if (lastError?.includes("not found")) {
    return `Parlance's browser bridge is not installed. Open a terminal and ${install}.`;
  }
  if (lastError?.includes("forbidden")) {
    return `Parlance's browser bridge does not allow this extension yet. ${capitalize(install)}.`;
  }
  return "Parlance's browser bridge closed the connection. Check that Parlance is open, then try again.";
}

function capitalize(text: string): string {
  return text.charAt(0).toUpperCase() + text.slice(1);
}
