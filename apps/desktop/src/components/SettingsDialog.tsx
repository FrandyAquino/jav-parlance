import * as Dialog from "@radix-ui/react-dialog";
import { X } from "lucide-react";
import type { ReactNode } from "react";
import type { Theme } from "@/bindings/Theme";
import { LANGUAGES } from "@/lib/languages";
import { useApp } from "@/store/app";
import { HistoryList } from "./HistoryList";
import { ModelList } from "./ModelList";
import { Button } from "./ui/button";
import { Select } from "./ui/select";
import { SwitchRow } from "./ui/switch";

const AUTO = "auto";
const THEMES: Array<{ value: Theme; label: string }> = [
  { value: "system", label: "Match system" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];

export function SettingsDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
  const settings = useApp((s) => s.settings);
  const info = useApp((s) => s.info);
  const save = useApp((s) => s.saveSettings);
  if (!settings) return null;

  const languageOptions = LANGUAGES.map((l) => ({
    value: l.code,
    label: l.name,
  }));

  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-black/25 backdrop-blur-[2px]" />
        <Dialog.Content className="glass-strong fixed inset-3 z-50 flex flex-col overflow-hidden rounded-[var(--radius-window)]">
          <header className="flex h-11 shrink-0 items-center border-b border-[var(--glass-border)] px-4">
            <Dialog.Title className="flex-1 text-[13px] font-medium text-ink">Settings</Dialog.Title>
            <Dialog.Close asChild>
              <Button size="icon" aria-label="Close settings">
                <X className="size-3.5" />
              </Button>
            </Dialog.Close>
          </header>
          <Dialog.Description className="sr-only">Languages, assistant, models and privacy</Dialog.Description>

          <div className="flex-1 space-y-3 overflow-y-auto p-3">
            <Section title="Languages">
              <Field label="Spoken language">
                <Select
                  label="Spoken language"
                  value={settings.sourceLanguage ?? AUTO}
                  options={[{ value: AUTO, label: "Detect automatically" }, ...languageOptions]}
                  onChange={(value) => void save({ sourceLanguage: value === AUTO ? null : value })}
                />
              </Field>
              <Field label="Translate into">
                <Select
                  label="Translate into"
                  value={settings.targetLanguage}
                  options={languageOptions}
                  onChange={(value) => void save({ targetLanguage: value })}
                />
              </Field>
            </Section>

            <Section title="Assistant">
              <SwitchRow
                label="Live translation"
                description="Shows each sentence in your language next to the original."
                checked={settings.translationEnabled}
                onChange={(translationEnabled) => void save({ translationEnabled })}
              />
              <SwitchRow
                label="Automatic copilot"
                description="Suggests replies to spoken questions. Written questions remain available when this is off."
                checked={settings.copilotEnabled}
                onChange={(copilotEnabled) => void save({ copilotEnabled })}
              />
            </Section>

            <Section title="Models">
              <ModelList />
              {info && <p className="mt-2 truncate text-[11px] text-ink-faint">Stored in {info.modelsDir}</p>}
            </Section>

            <Section title="Privacy">
              <p className="pb-1 text-[12px] text-ink-muted">
                Audio is processed on this device and never leaves it. Nothing is sent to any server.
              </p>
              <SwitchRow
                label="Keep meeting history"
                description="Saves transcripts locally so you can review them later. Off by default."
                checked={settings.historyEnabled}
                onChange={(historyEnabled) => void save({ historyEnabled })}
              />
              {settings.historyEnabled && <HistoryList />}
            </Section>

            <Section title="Appearance">
              <Field label="Theme">
                <Select
                  label="Theme"
                  value={settings.theme}
                  options={THEMES}
                  onChange={(value) => void save({ theme: value as Theme })}
                />
              </Field>
            </Section>

            {info && (
              <p className="text-[11px] text-ink-faint">
                Parlance {info.version} · {info.platform} · capture: {info.captureBackend}
              </p>
            )}
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="glass rounded-xl px-4 py-3">
      <h3 className="mb-1.5 text-[12px] font-medium text-ink">{title}</h3>
      {children}
    </section>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-4 py-1">
      <span className="text-[13px] text-ink-muted">{label}</span>
      {children}
    </div>
  );
}
