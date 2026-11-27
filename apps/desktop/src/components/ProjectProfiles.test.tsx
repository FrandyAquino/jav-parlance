import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { ipc } from "@/lib/ipc";
import { useProfiles } from "@/store/profiles";
import { ProjectProfiles } from "./ProjectProfiles";

afterEach(cleanup);
beforeEach(() => {
  vi.restoreAllMocks();
  useProfiles.setState({
    profiles: [{ id: "a", name: "API", content: "Uses Redis", revision: 1 }],
    activeProfileId: "a",
    loaded: true,
    busy: false,
    error: null,
  });
});

it("preserves edits on storage error and Cancel does not persist them", async () => {
  vi.spyOn(ipc, "saveProjectProfile").mockRejectedValue(new Error("Disk unavailable"));
  render(<ProjectProfiles />);
  fireEvent.click(screen.getByRole("button", { name: "Edit project context" }));
  fireEvent.change(screen.getByLabelText("Project facts"), {
    target: { value: "Uses Redis and Postgres" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Save context" }));
  await waitFor(() => expect(screen.getByLabelText("Project facts")).not.toBeDisabled());
  expect(screen.getByLabelText("Project facts")).toHaveValue("Uses Redis and Postgres");
  expect(useProfiles.getState().profiles[0]?.content).toBe("Uses Redis");
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  expect(ipc.saveProjectProfile).toHaveBeenCalledTimes(1);
});

it("requires the explicit delete action and restores focus after closing", async () => {
  const remove = vi.spyOn(ipc, "deleteProjectProfile").mockResolvedValue({ profiles: [], activeProfileId: null });
  render(<ProjectProfiles />);
  const edit = screen.getByRole("button", { name: "Edit project context" });
  edit.focus();
  fireEvent.click(edit);
  fireEvent.click(screen.getByRole("button", { name: "Delete" }));
  fireEvent.click(screen.getByRole("button", { name: "Keep profile" }));
  expect(remove).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  await waitFor(() => expect(edit).toHaveFocus());
  fireEvent.click(edit);
  fireEvent.click(screen.getByRole("button", { name: "Delete" }));
  fireEvent.click(screen.getByRole("button", { name: "Delete profile" }));
  await waitFor(() => expect(useProfiles.getState().activeProfileId).toBeNull());
  expect(remove).toHaveBeenCalledWith("a");
});
