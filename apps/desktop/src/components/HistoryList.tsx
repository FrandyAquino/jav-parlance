import { Copy, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import type { HistoryEntry } from "@/bindings/HistoryEntry";
import type { HistoryLine } from "@/bindings/HistoryLine";
import { formatClock } from "@/lib/format";
import { errorMessage, ipc } from "@/lib/ipc";
import { Button } from "./ui/button";
import { Tooltip } from "./ui/tooltip";

export function toPlainText(lines: HistoryLine[]): string {
  return lines
    .map((line) => {
      const stamp = `[${formatClock(line.startMs)}]`;
      return line.translation ? `${stamp} ${line.text}\n        ${line.translation}` : `${stamp} ${line.text}`;
    })
    .join("\n");
}

export function HistoryList() {
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [version, setVersion] = useState(0);

  useEffect(() => {
    let current = true;
    ipc.listHistory().then(
      (list) => current && setEntries(list),
      (e: unknown) => current && setError(errorMessage(e)),
    );
    return () => {
      current = false;
    };
  }, [version]);

  const copy = async (id: number) => navigator.clipboard.writeText(toPlainText(await ipc.historyTranscript(id)));
  const remove = async (id: number) => {
    await ipc.deleteHistory(id);
    setVersion((v) => v + 1);
  };

  if (error) return <p className="text-[12px] text-record">{error}</p>;
  if (entries.length === 0) return <p className="text-[12px] text-ink-faint">No saved meetings yet.</p>;

  return (
    <ul className="divide-y divide-[var(--glass-border)] overflow-hidden rounded-lg border border-[var(--glass-border)] bg-[var(--glass)]">
      {entries.map((entry) => (
        <li key={entry.id} className="flex items-center gap-2 px-3 py-2">
          <div className="min-w-0 flex-1">
            <div className="text-[13px] text-ink">{new Date(entry.startedAtMs).toLocaleString()}</div>
            <div className="text-[11px] text-ink-muted">
              {entry.lineCount} lines
              {entry.endedAtMs !== null && ` · ${formatClock(entry.endedAtMs - entry.startedAtMs)}`}
            </div>
          </div>
          <Tooltip label="Copy transcript">
            <Button size="icon" aria-label="Copy transcript" onClick={() => void copy(entry.id)}>
              <Copy className="size-3.5" />
            </Button>
          </Tooltip>
          <Tooltip label="Delete">
            <Button size="icon" aria-label="Delete meeting" onClick={() => void remove(entry.id)}>
              <Trash2 className="size-3.5" />
            </Button>
          </Tooltip>
        </li>
      ))}
    </ul>
  );
}
