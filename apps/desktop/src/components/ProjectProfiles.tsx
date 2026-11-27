import * as Dialog from "@radix-ui/react-dialog";
import { Folder, Pencil, Plus, Trash2, X } from "lucide-react";
import { useRef, useState } from "react";
import type { ProjectProfile } from "@/bindings/ProjectProfile";
import { useProfiles } from "@/store/profiles";
import { Button } from "./ui/button";
import { Select } from "./ui/select";

const NONE = "__none__";

export function ProjectProfiles() {
  const profiles = useProfiles((s) => s.profiles);
  const activeId = useProfiles((s) => s.activeProfileId);
  const busy = useProfiles((s) => s.busy);
  const loaded = useProfiles((s) => s.loaded);
  const error = useProfiles((s) => s.error);
  const [open, setOpen] = useState(false);
  const [editing, setEditing] = useState<ProjectProfile | null>(null);
  const opener = useRef<HTMLButtonElement | null>(null);
  const newButton = useRef<HTMLButtonElement | null>(null);
  const active = profiles.find((p) => p.id === activeId);
  function edit(profile: ProjectProfile | null, button: HTMLButtonElement) {
    opener.current = button;
    setEditing(profile);
    setOpen(true);
  }
  return (
    <div className="min-w-0 max-w-full">
      <div className="flex items-center gap-1">
        <Select
          label="Project context"
          value={activeId ?? NONE}
          disabled={busy || !loaded}
          icon={<Folder className="size-3 shrink-0" aria-hidden />}
          options={[
            { value: NONE, label: "No project context" },
            ...profiles.map((p) => ({ value: p.id, label: p.name })),
          ]}
          onChange={(id) => void useProfiles.getState().select(id === NONE ? null : id)}
          className="max-w-48"
        />
        {active && (
          <Button
            size="icon"
            aria-label="Edit project context"
            onClick={(event) => edit(active, event.currentTarget)}
            disabled={busy}
          >
            <Pencil className="size-3" aria-hidden />
          </Button>
        )}
        <Button
          ref={newButton}
          size="icon"
          aria-label="New project context"
          onClick={(event) => edit(null, event.currentTarget)}
          disabled={busy || !loaded}
        >
          <Plus className="size-3.5" aria-hidden />
        </Button>
      </div>
      {error && (
        <div role="alert" className="max-w-72 text-[11px] text-warn">
          {error}
          <Button size="sm" disabled={busy} onClick={() => void useProfiles.getState().load()}>
            Reload profiles
          </Button>
        </div>
      )}
      <Dialog.Root
        open={open}
        onOpenChange={(next) => {
          if (!busy) setOpen(next);
        }}
      >
        <Dialog.Portal>
          <Dialog.Overlay className="fixed inset-0 z-40 bg-black/25 backdrop-blur-[2px]" />
          <Dialog.Content
            className="glass-strong fixed inset-3 z-50 flex flex-col overflow-hidden rounded-xl"
            onCloseAutoFocus={(event) => {
              event.preventDefault();
              (opener.current?.isConnected ? opener.current : newButton.current)?.focus();
            }}
          >
            <header className="flex shrink-0 items-center justify-between border-b border-line px-4 py-2">
              <Dialog.Title className="text-[14px] font-medium">
                {editing ? "Edit project context" : "New project context"}
              </Dialog.Title>
              <Dialog.Close asChild>
                <Button size="icon" disabled={busy} aria-label="Close project editor">
                  <X className="size-3.5" aria-hidden />
                </Button>
              </Dialog.Close>
            </header>
            <Dialog.Description className="px-4 pt-3 text-[12px] text-ink-muted">
              Saved only on this device. Add facts about your stack, architecture and team processes. This does not save
              meeting history.
            </Dialog.Description>
            {open && <ProfileEditor key={editing?.id ?? "new"} profile={editing} onDone={() => setOpen(false)} />}
          </Dialog.Content>
        </Dialog.Portal>
      </Dialog.Root>
    </div>
  );
}

function ProfileEditor({ profile, onDone }: { profile: ProjectProfile | null; onDone: () => void }) {
  const [name, setName] = useState(profile?.name ?? "");
  const [content, setContent] = useState(profile?.content ?? "");
  const [confirmDelete, setConfirmDelete] = useState(false);
  const busy = useProfiles((s) => s.busy);
  const error = useProfiles((s) => s.error);
  async function save() {
    if (await useProfiles.getState().save({ id: profile?.id ?? null, name, content })) onDone();
  }
  async function remove() {
    if (profile && (await useProfiles.getState().remove(profile.id))) onDone();
  }
  return (
    <form
      className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto p-4"
      onSubmit={(event) => {
        event.preventDefault();
        void save();
      }}
    >
      <label className="text-[12px] text-ink-muted">
        Project name
        <input
          className="input-field mt-1 w-full"
          value={name}
          onChange={(event) => setName(event.target.value)}
          placeholder="Payments API"
          disabled={busy}
        />
      </label>
      <label className="flex min-h-36 flex-1 flex-col text-[12px] text-ink-muted">
        Project facts
        <textarea
          className="input-field mt-1 min-h-28 flex-1 resize-y"
          value={content}
          onChange={(event) => setContent(event.target.value)}
          disabled={busy}
          placeholder="Stack: Rust, PostgreSQL, Redis. We cache frequent reads in Redis. Deployments require a review and staging checks."
        />
      </label>
      <p className="text-[11px] text-ink-faint">
        Keep it concise. A saved profile may need shortening to fit alongside a longer question. Facts are captured
        separately for each answer.
      </p>
      {error && (
        <p role="alert" className="text-[12px] text-warn">
          {error}
        </p>
      )}
      {confirmDelete ? (
        <div className="flex flex-wrap items-center gap-2 text-[12px] text-warn">
          <span>Delete this saved profile?</span>
          <Button variant="record" size="sm" disabled={busy} onClick={() => void remove()}>
            Delete profile
          </Button>
          <Button size="sm" disabled={busy} onClick={() => setConfirmDelete(false)}>
            Keep profile
          </Button>
        </div>
      ) : (
        <div className="flex items-center justify-between gap-2">
          {profile ? (
            <Button size="sm" disabled={busy} onClick={() => setConfirmDelete(true)}>
              <Trash2 className="size-3" aria-hidden /> Delete
            </Button>
          ) : (
            <span />
          )}
          <div className="flex gap-2">
            <Button size="sm" disabled={busy} onClick={onDone}>
              Cancel
            </Button>
            <Button type="submit" variant="primary" size="sm" disabled={busy || !name.trim()}>
              {busy ? "Saving…" : "Save context"}
            </Button>
          </div>
        </div>
      )}
    </form>
  );
}
