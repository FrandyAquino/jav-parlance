import { Check, Download, X } from "lucide-react";
import type { ModelDescriptor } from "@/bindings/ModelDescriptor";
import type { ModelKind } from "@/bindings/ModelKind";
import { formatBytes } from "@/lib/format";
import { useApp } from "@/store/app";
import { Button } from "./ui/button";

const KIND_LABEL: Record<ModelKind, string> = {
  speech: "Speech recognition",
  voiceActivity: "Speech detection",
  language: "Translation & copilot",
  runtime: "Runtime",
};

export function ModelList() {
  const models = useApp((s) => s.models);
  const settings = useApp((s) => s.settings);
  const saveSettings = useApp((s) => s.saveSettings);
  const kinds = [...new Set(models.map((m) => m.kind))];

  const isSelected = (model: ModelDescriptor) =>
    model.id === settings?.speechModel || model.id === settings?.languageModel;
  const select = (model: ModelDescriptor) => {
    if (model.kind === "speech") void saveSettings({ speechModel: model.id });
    if (model.kind === "language") void saveSettings({ languageModel: model.id });
  };

  return (
    <div className="space-y-4">
      {kinds.map((kind) => (
        <div key={kind}>
          <h4 className="mb-1 text-[11px] font-medium text-ink-faint">{KIND_LABEL[kind]}</h4>
          <ul className="divide-y divide-line rounded-lg border border-line">
            {models
              .filter((m) => m.kind === kind)
              .map((model) => (
                <ModelRow
                  key={model.id}
                  model={model}
                  selectable={kind === "speech" || kind === "language"}
                  selected={isSelected(model)}
                  onSelect={() => select(model)}
                />
              ))}
          </ul>
        </div>
      ))}
    </div>
  );
}

interface ModelRowProps {
  model: ModelDescriptor;
  selectable: boolean;
  selected: boolean;
  onSelect: () => void;
}

function ModelRow({ model, selectable, selected, onSelect }: ModelRowProps) {
  const progress = useApp((s) => s.downloads[model.id]);
  const download = useApp((s) => s.download);
  const cancel = useApp((s) => s.cancelDownload);
  const installed = model.status === "installed";
  const percent = progress ? Math.round((progress.downloadedBytes / Math.max(1, progress.totalBytes)) * 100) : 0;

  return (
    <li className="flex items-center gap-3 px-3 py-2">
      {selectable ? (
        <input
          type="radio"
          aria-label={`Use ${model.name}`}
          checked={selected}
          disabled={!installed}
          onChange={onSelect}
          className="accent-[var(--accent)]"
        />
      ) : (
        <span className="w-[13px]" aria-hidden />
      )}
      <div className="min-w-0 flex-1">
        <div className="text-[13px] text-ink">
          {model.name}
          {model.isDefault && <span className="ml-1.5 text-[11px] text-ink-faint">recommended</span>}
        </div>
        <div className="truncate text-[11px] text-ink-muted">
          {model.description} · {formatBytes(model.sizeBytes)} · {model.license}
        </div>
        {progress && (
          <div className="mt-1 h-1 overflow-hidden rounded-full bg-surface-raised" role="progressbar" aria-valuenow={percent}>
            <div className="h-full bg-accent transition-[width]" style={{ width: `${percent}%` }} />
          </div>
        )}
      </div>
      {installed ? (
        <Check className="size-4 text-accent" aria-label="Installed" />
      ) : progress ? (
        <Button size="sm" variant="subtle" onClick={() => void cancel(model.id)}>
          <X className="size-3" /> {percent}%
        </Button>
      ) : (
        <Button size="sm" variant="subtle" onClick={() => void download(model.id)}>
          <Download className="size-3" /> Get
        </Button>
      )}
    </li>
  );
}
