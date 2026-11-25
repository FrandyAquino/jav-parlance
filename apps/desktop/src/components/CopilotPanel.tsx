import { Copy, Sparkles } from "lucide-react";
import type { Suggestion } from "@/bindings/Suggestion";
import { languageName } from "@/lib/languages";
import { useSession } from "@/store/session";
import { Button } from "./ui/button";
import { Tooltip } from "./ui/tooltip";

export function CopilotPanel({ enabled }: { enabled: boolean }) {
  const latest = useSession((s) => s.suggestions[0]);

  return (
    <section aria-label="Copilot" className="max-h-[45%] shrink-0 overflow-y-auto border-t border-line px-5 py-3">
      {!enabled ? (
        <Hint>The copilot is off. Turn it on in Settings to get suggested replies.</Hint>
      ) : latest ? (
        <SuggestionCard suggestion={latest} />
      ) : (
        <Hint>When someone asks a question, a suggested reply appears here in both languages.</Hint>
      )}
    </section>
  );
}

function Hint({ children }: { children: string }) {
  return (
    <p className="flex items-center gap-2 text-[12px] text-ink-muted">
      <Sparkles className="size-3.5 shrink-0 text-accent" aria-hidden />
      {children}
    </p>
  );
}

function SuggestionCard({ suggestion }: { suggestion: Suggestion }) {
  const sameLanguage = suggestion.speakerLanguage === suggestion.userLanguage;
  return (
    <div className="selectable space-y-2" aria-busy={!suggestion.isComplete}>
      <p className="text-[12px] text-ink-muted">
        <Sparkles className="mr-1.5 inline size-3.5 align-[-2px] text-accent" aria-hidden />
        {suggestion.question}
      </p>
      <div className={`grid gap-6 ${sameLanguage ? "grid-cols-1" : "grid-cols-2"}`}>
        <Reply label={`Say it · ${languageName(suggestion.speakerLanguage)}`} text={suggestion.replyInSpeakerLanguage} />
        {!sameLanguage && (
          <Reply label={`Meaning · ${languageName(suggestion.userLanguage)}`} text={suggestion.replyInUserLanguage} />
        )}
      </div>
    </div>
  );
}

function Reply({ label, text }: { label: string; text: string }) {
  return (
    <div className="group relative">
      <div className="mb-0.5 flex items-center justify-between">
        <span className="text-[11px] font-medium text-ink-faint">{label}</span>
        <Tooltip label="Copy">
          <Button
            size="icon"
            className="size-5 opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
            aria-label={`Copy: ${label}`}
            onClick={() => void navigator.clipboard.writeText(text)}
          >
            <Copy className="size-3" />
          </Button>
        </Tooltip>
      </div>
      <p className="text-[14px] leading-relaxed text-ink">{text || "…"}</p>
    </div>
  );
}
