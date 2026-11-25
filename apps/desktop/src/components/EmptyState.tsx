import { Download } from "lucide-react";
import { formatBytes } from "@/lib/format";
import { missingRequiredModels, useApp } from "@/store/app";
import { Button } from "./ui/button";

/** First-run guidance: get the required models, then pick a source. */
export function EmptyState() {
  const models = useApp((s) => s.models);
  const settings = useApp((s) => s.settings);
  const downloads = useApp((s) => s.downloads);
  const download = useApp((s) => s.download);
  const missing = missingRequiredModels(models, settings);

  if (missing.length > 0) {
    const total = missing.reduce((sum, m) => sum + m.sizeBytes, 0);
    const busy = missing.some((m) => downloads[m.id]);
    const getAll = async () => {
      for (const model of missing) await download(model.id);
    };
    return (
      <div className="max-w-sm text-center">
        <p className="text-[14px] text-ink">Parlance needs its speech models before it can listen.</p>
        <p className="mt-1 text-[12px] text-ink-muted">
          {formatBytes(total)}, downloaded once and verified. After that everything works offline.
        </p>
        <Button variant="primary" className="mt-4" onClick={() => void getAll()} disabled={busy}>
          <Download className="size-3.5" /> {busy ? "Downloading…" : "Download models"}
        </Button>
      </div>
    );
  }

  return (
    <div className="max-w-sm text-center">
      <p className="text-[14px] text-ink">Pick what to listen to, then press Listen.</p>
      <p className="mt-1 text-[12px] text-ink-muted">
        System audio captures your meeting app. Add your microphone to transcribe yourself too.
      </p>
    </div>
  );
}
