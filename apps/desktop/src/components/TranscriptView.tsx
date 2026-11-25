import { ArrowDown } from "lucide-react";
import { memo, type ReactNode } from "react";
import { useStickToBottom } from "@/hooks/useStickToBottom";
import { formatClock } from "@/lib/format";
import { languageName } from "@/lib/languages";
import { useApp } from "@/store/app";
import { type TranscriptRow, useSession } from "@/store/session";

/**
 * Original and translation share one scroll container and one grid row per
 * segment, so the columns stay aligned by construction rather than by
 * synchronizing two scroll positions.
 */
export function TranscriptView({ emptyState }: { emptyState: ReactNode }) {
  const order = useSession((s) => s.order);
  const rows = useSession((s) => s.rows);
  const detected = useSession((s) => s.detectedLanguage);
  const settings = useApp((s) => s.settings);
  const translating = settings?.translationEnabled ?? false;
  const lastRow = order.length > 0 ? rows[order[order.length - 1] ?? 0] : undefined;
  const { ref, following, onScroll, jumpToLatest } = useStickToBottom<HTMLDivElement>(
    `${order.length}:${lastRow?.segment.revision}:${lastRow?.translation?.text.length}`,
  );

  if (order.length === 0) return <div className="flex flex-1 items-center justify-center p-6">{emptyState}</div>;

  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      <div className={`grid shrink-0 gap-6 px-5 pb-1 pt-2.5 ${translating ? "grid-cols-2" : "grid-cols-1"}`}>
        <ColumnHeading>Original{detected ? ` · ${languageName(detected)}` : ""}</ColumnHeading>
        {translating && <ColumnHeading>{languageName(settings?.targetLanguage)}</ColumnHeading>}
      </div>
      <div
        ref={ref}
        onScroll={onScroll}
        className="selectable min-h-0 flex-1 overflow-y-auto px-5 pb-4"
        role="log"
        aria-live="polite"
        aria-label="Live transcript"
      >
        {order.map((id) => {
          const row = rows[id];
          return row ? <Row key={id} row={row} translating={translating} /> : null;
        })}
      </div>
      {!following && (
        <button
          type="button"
          onClick={jumpToLatest}
          className="absolute bottom-3 left-1/2 flex -translate-x-1/2 items-center gap-1 rounded-full border border-line bg-[var(--surface)] px-3 py-1 text-[12px] text-ink shadow-lg backdrop-blur"
        >
          <ArrowDown className="size-3" aria-hidden /> Live
        </button>
      )}
    </div>
  );
}

function ColumnHeading({ children }: { children: ReactNode }) {
  return <h2 className="text-[11px] font-medium text-ink-faint">{children}</h2>;
}

const Row = memo(function Row({ row, translating }: { row: TranscriptRow; translating: boolean }) {
  const { segment, translation } = row;
  return (
    <div className={`group grid gap-6 py-1.5 ${translating ? "grid-cols-2" : "grid-cols-1"}`}>
      <p className="relative text-[14px] leading-relaxed text-ink">
        <time className="absolute -left-4 top-1 hidden text-[10px] tabular-nums text-ink-faint group-hover:block">
          {formatClock(segment.startMs)}
        </time>
        {segment.stableText}
        {segment.unstableText && (
          <span className="text-ink-muted">
            {segment.stableText ? " " : ""}
            {segment.unstableText}
          </span>
        )}
      </p>
      {translating && (
        <p
          className={`text-[14px] leading-relaxed ${translation?.isFinal ? "text-ink" : "text-ink-muted"}`}
          lang={translation?.targetLanguage}
        >
          {translation?.text ?? <span className="text-ink-faint">…</span>}
        </p>
      )}
    </div>
  );
});
