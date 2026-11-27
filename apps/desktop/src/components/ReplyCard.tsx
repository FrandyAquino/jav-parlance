import { Copy } from "lucide-react";
import { useState } from "react";
import type { Suggestion } from "@/bindings/Suggestion";
import { languageName } from "@/lib/languages";
import { errorMessage } from "@/lib/ipc";
import { isPending } from "@/store/copilot";
import { Button } from "./ui/button";

const statusLabel = {
  queued: "Waiting for model",
  generating: "Preparing · partial answer",
  complete: "Ready to review",
  failed: "Could not finish",
  cancelled: "Stopped · partial answer",
};

export function ReplyCard({ suggestion, reading = false }: { suggestion: Suggestion; reading?: boolean }) {
  const sameLanguage = suggestion.speakerLanguage === suggestion.userLanguage;
  return (
    <article className="selectable min-w-0 space-y-3" aria-busy={isPending(suggestion)}>
      <div className="flex flex-wrap items-center justify-between gap-2 text-[11px] text-ink-muted">
        <span>
          {suggestion.profile
            ? `${suggestion.profile.name} · revision ${suggestion.profile.revision}`
            : "No project context"}
        </span>
        <span role="status" className={suggestion.status === "failed" ? "text-warn" : ""}>
          {statusLabel[suggestion.status]}
        </span>
      </div>
      <p className="break-words text-[12px] text-ink-muted">{suggestion.question}</p>
      <div className={`grid gap-4 ${!sameLanguage && !reading ? "min-[700px]:grid-cols-2" : "grid-cols-1"}`}>
        <Reply
          label={`Say it · ${languageName(suggestion.speakerLanguage)}`}
          text={suggestion.replyInSpeakerLanguage}
          language={suggestion.speakerLanguage}
          large={reading}
        />
        {!sameLanguage && (
          <Reply
            label={`Meaning · ${languageName(suggestion.userLanguage)}`}
            text={suggestion.replyInUserLanguage}
            language={suggestion.userLanguage}
            large={false}
          />
        )}
      </div>
      {suggestion.error && (
        <p role="alert" className="break-words text-[12px] text-warn">
          {suggestion.error}
        </p>
      )}
      {suggestion.conversationTruncated && (
        <p className="text-[11px] text-warn">
          Only the most recent conversation that fits was used. Your question and project context were kept in full.
        </p>
      )}
    </article>
  );
}

function Reply({
  label,
  text,
  language,
  large,
}: {
  label: string;
  text: string;
  language: string | null;
  large: boolean;
}) {
  const [copyState, setCopyState] = useState<string | null>(null);
  async function copy() {
    try {
      await navigator.clipboard.writeText(text);
      setCopyState("Copied");
    } catch (error) {
      setCopyState(errorMessage(error));
    }
  }
  return (
    <div className="min-w-0">
      <div className="mb-1 flex items-center justify-between gap-2">
        <span className="text-[11px] font-medium text-ink-faint">{label}</span>
        <Button size="icon" aria-label={`Copy: ${label}`} disabled={!text} onClick={() => void copy()}>
          <Copy className="size-3" aria-hidden />
        </Button>
      </div>
      <p
        lang={language ?? undefined}
        className={`whitespace-pre-wrap break-words leading-relaxed text-ink ${large ? "text-[22px]" : "text-[14px]"}`}
      >
        {text || "…"}
      </p>
      {copyState && (
        <span role="status" className="text-[11px] text-ink-muted">
          {copyState}
        </span>
      )}
    </div>
  );
}
