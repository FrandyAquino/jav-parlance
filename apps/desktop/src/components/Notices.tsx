import { X } from "lucide-react";
import { useEffect } from "react";
import { cn } from "@/lib/cn";
import { type ActiveNotice, dismissNotice, useSession } from "@/store/session";

const AUTO_DISMISS_MS = 6000;

export function Notices({ error }: { error: string | null }) {
  const notices = useSession((s) => s.notices);
  return (
    <div className="pointer-events-none absolute bottom-3 right-3 z-40 flex max-w-80 flex-col gap-2" role="status">
      {error && <Toast level="error" message={error} />}
      {notices.map((notice) => (
        <AutoDismiss key={notice.id} notice={notice} />
      ))}
    </div>
  );
}

function AutoDismiss({ notice }: { notice: ActiveNotice }) {
  useEffect(() => {
    if (notice.level === "error") return;
    const timer = window.setTimeout(() => dismissNotice(notice.id), AUTO_DISMISS_MS);
    return () => window.clearTimeout(timer);
  }, [notice]);
  return <Toast level={notice.level} message={notice.message} onDismiss={() => dismissNotice(notice.id)} />;
}

function Toast({ level, message, onDismiss }: { level: string; message: string; onDismiss?: () => void }) {
  return (
    <div
      className={cn(
        "pointer-events-auto flex items-start gap-2 rounded-lg border border-line bg-[var(--surface)] px-3 py-2 text-[12px] text-ink shadow-lg backdrop-blur",
        level === "error" && "border-record/40",
        level === "warning" && "border-warn/40",
      )}
    >
      <span
        className={cn("mt-1 size-1.5 shrink-0 rounded-full bg-accent", level === "error" && "bg-record", level === "warning" && "bg-warn")}
        aria-hidden
      />
      <span className="flex-1">{message}</span>
      {onDismiss && (
        <button type="button" onClick={onDismiss} aria-label="Dismiss" className="text-ink-faint hover:text-ink">
          <X className="size-3" />
        </button>
      )}
    </div>
  );
}
