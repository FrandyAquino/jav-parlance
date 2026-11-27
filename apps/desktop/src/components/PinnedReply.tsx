import { BookmarkCheck } from "lucide-react";
import { useCopilot } from "@/store/copilot";
import { ReplyCard } from "./ReplyCard";
import { Button } from "./ui/button";

export function PinnedReply() {
  const pinned = useCopilot((s) => s.pinned);
  if (!pinned) return null;
  return (
    <section
      aria-label="Pinned answer"
      className="glass-accent selectable min-h-0 flex-1 overflow-y-auto rounded-xl p-5"
    >
      <div className="mb-4 flex flex-wrap items-center justify-between gap-2">
        <h2 className="flex items-center gap-2 text-[12px] font-medium text-accent">
          <BookmarkCheck className="size-3.5" aria-hidden /> Pinned answer
        </h2>
        <Button size="sm" onClick={() => useCopilot.getState().unpin()}>
          Unpin
        </Button>
      </div>
      <ReplyCard key={pinned.requestId} suggestion={pinned} reading />
      <div className="mt-5 flex flex-wrap gap-2">
        <Button variant="subtle" size="sm" onClick={() => useCopilot.getState().setMode("expanded")}>
          Back to preparation
        </Button>
        <Button size="sm" onClick={() => useCopilot.getState().setMode("compact")}>
          Back to live
        </Button>
      </div>
    </section>
  );
}
