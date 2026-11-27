import { AppWindow, Mic, MonitorSpeaker } from "lucide-react";
import type { SourceDescriptor } from "@/bindings/SourceDescriptor";
import { useApp } from "@/store/app";
import { Select, type SelectOption } from "./ui/select";

const NO_MICROPHONE = "__none__";

function groupOf(source: SourceDescriptor): string {
  switch (source.kind.type) {
    case "systemOutput":
      return "System audio";
    case "application":
      return "Applications";
    case "browserTab":
      return "Browser tabs";
    case "microphone":
      return "Microphones";
  }
}

export function SourcePicker({ disabled }: { disabled: boolean }) {
  const sources = useApp((s) => s.sources);
  const selected = useApp((s) => s.selectedSourceId);
  const microphone = useApp((s) => s.microphoneId);
  const selectSource = useApp((s) => s.selectSource);
  const selectMicrophone = useApp((s) => s.selectMicrophone);
  const refresh = useApp((s) => s.refreshSources);

  const primary: SelectOption[] = sources.map((s) => ({ value: s.id, label: s.name, group: groupOf(s) }));
  const microphones: SelectOption[] = [
    { value: NO_MICROPHONE, label: "No mic" },
    ...sources.filter((s) => s.kind.type === "microphone").map((s) => ({ value: s.id, label: s.name })),
  ];
  const selectedKind = sources.find((s) => s.id === selected)?.kind.type;

  return (
    <div className="flex min-w-0 items-center" onPointerDown={() => void refresh()}>
      <Select
        label="Audio source"
        value={selected}
        options={primary}
        onChange={selectSource}
        placeholder="Choose source"
        disabled={disabled}
        className="max-w-44"
        icon={
          selectedKind === "application" ? (
            <AppWindow className="size-3.5 shrink-0" aria-hidden />
          ) : selectedKind === "microphone" ? (
            <Mic className="size-3.5 shrink-0" aria-hidden />
          ) : (
            <MonitorSpeaker className="size-3.5 shrink-0" aria-hidden />
          )
        }
      />
      {selectedKind !== "microphone" && (
        <Select
          label="Include microphone"
          value={microphone ?? NO_MICROPHONE}
          options={microphones}
          onChange={(value) => selectMicrophone(value === NO_MICROPHONE ? null : value)}
          disabled={disabled}
          className={microphone ? "max-w-32 text-ink" : "max-w-32"}
          icon={<Mic className="size-3.5 shrink-0" aria-hidden />}
        />
      )}
    </div>
  );
}
