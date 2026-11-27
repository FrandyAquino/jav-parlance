import { Bookmark, MessageSquare, Radio, Send, Square } from "lucide-react";
import { LANGUAGES } from "@/lib/languages";
import { useApp } from "@/store/app";
import { isPending, useCopilot } from "@/store/copilot";
import { useProfiles } from "@/store/profiles";
import { LiveBeam } from "./LiveBeam";
import { ProjectProfiles } from "./ProjectProfiles";
import { ReplyCard } from "./ReplyCard";
import { Button } from "./ui/button";
import { Select } from "./ui/select";

export function CopilotPanel({ enabled, onOpenSettings }: { enabled: boolean; onOpenSettings: () => void }) {
  const tab = useCopilot((s) => s.tab);
  const question = useCopilot((s) => s.question);
  const idea = useCopilot((s) => s.idea);
  const replyLanguage = useCopilot((s) => s.replyLanguage);
  const manual = useCopilot((s) => s.manualResult);
  const automatic = useCopilot((s) => s.latestAutomatic);
  const submitting = useCopilot((s) => s.submitting);
  const submissionError = useCopilot((s) => s.submissionError);
  const settings = useApp((s) => s.settings);
  const activeProfileId = useProfiles((s) => s.activeProfileId);
  const profilesReady = useProfiles((s) => s.loaded && !s.busy && !s.error);
  const result = tab === "manual" ? manual : automatic;
  const running = submitting || isPending(manual);
  const submit = () => {
    if (settings) void useCopilot.getState().ask(settings.targetLanguage, activeProfileId);
  };
  return (
    <section
      aria-label="Copilot"
      className="glass-accent relative min-h-0 flex-[2] overflow-y-auto rounded-xl px-4 py-3"
    >
      <LiveBeam active={isPending(result)} tone="accent" />
      <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
        <div className="flex items-center gap-1 rounded-lg bg-surface-raised p-0.5" aria-label="Question source">
          <Button
            size="sm"
            variant={tab === "live" ? "subtle" : "ghost"}
            aria-pressed={tab === "live"}
            onClick={() => useCopilot.getState().setTab("live")}
          >
            <Radio className="size-3" aria-hidden /> Live
          </Button>
          <Button
            size="sm"
            variant={tab === "manual" ? "subtle" : "ghost"}
            aria-pressed={tab === "manual"}
            onClick={() => useCopilot.getState().setTab("manual")}
          >
            <MessageSquare className="size-3" aria-hidden /> Ask a question
          </Button>
        </div>
        <ProjectProfiles />
      </div>
      {tab === "manual" && (
        <form
          onSubmit={(event) => {
            event.preventDefault();
            submit();
          }}
          onKeyDown={(event) => {
            if (event.key === "Enter" && (event.metaKey || event.ctrlKey) && !event.nativeEvent.isComposing) {
              event.preventDefault();
              event.stopPropagation();
              if (profilesReady && !submitting) submit();
            }
          }}
          className="mb-4 space-y-2"
        >
          <label className="block text-[11px] text-ink-muted">
            Your question
            <textarea
              value={question}
              onChange={(event) => useCopilot.getState().setDraft({ question: event.target.value })}
              placeholder="Paste a question, or ask about your tools and processes…"
              rows={2}
              className="input-field mt-1 min-h-16 w-full resize-y"
            />
          </label>
          <label className="block text-[11px] text-ink-muted">
            Your idea (optional)
            <textarea
              value={idea}
              onChange={(event) => useCopilot.getState().setDraft({ idea: event.target.value })}
              placeholder="What should the answer emphasize?"
              rows={1}
              className="input-field mt-1 w-full resize-y"
            />
          </label>
          <div className="flex flex-wrap items-center justify-between gap-2">
            <Select
              label="Reply language"
              value={replyLanguage}
              options={LANGUAGES.map((language) => ({
                value: language.code,
                label: language.name,
              }))}
              onChange={(replyLanguage) => useCopilot.getState().setDraft({ replyLanguage })}
            />
            <div className="flex gap-1">
              {running && (
                <Button size="sm" onClick={() => void useCopilot.getState().cancel()} aria-label="Cancel answer">
                  <Square className="size-3" aria-hidden /> Stop
                </Button>
              )}
              <Button
                type="submit"
                variant="primary"
                size="sm"
                disabled={!question.trim() || !settings || submitting || !profilesReady}
              >
                <Send className="size-3" aria-hidden /> Prepare answer
              </Button>
            </div>
          </div>
          <p className="text-[10px] text-ink-faint">Runs on this device · No audio needed · Review before using</p>
          {submissionError && (
            <p role="alert" className="break-words text-[12px] text-warn">
              {submissionError}
            </p>
          )}
        </form>
      )}
      {result ? (
        <div className="border-t border-line pt-3">
          <ReplyCard key={result.requestId} suggestion={result} />
          <div className="mt-3 flex flex-wrap gap-2">
            <Button
              size="sm"
              variant="subtle"
              disabled={result.status !== "complete"}
              onClick={() => useCopilot.getState().pin(result)}
              aria-label="Pin answer to read"
            >
              <Bookmark className="size-3" aria-hidden /> Pin to read
            </Button>
            {tab === "live" && (
              <Button size="sm" onClick={() => useCopilot.getState().prepare(result)}>
                Work on this question
              </Button>
            )}
          </div>
        </div>
      ) : tab === "live" ? (
        <div className="py-4 text-[12px] text-ink-muted">
          <p>
            {enabled
              ? "Questions from the conversation will appear here. You choose what to say."
              : "Automatic suggestions are off. You can still ask a written question."}
          </p>
          {!enabled && (
            <Button size="sm" className="mt-2" onClick={onOpenSettings}>
              Assistant settings
            </Button>
          )}
        </div>
      ) : submitting ? (
        <p role="status" className="text-[12px] text-ink-muted">
          Preparing your question…
        </p>
      ) : null}
      <Button size="sm" className="mt-2 text-[11px]" onClick={onOpenSettings}>
        Models and languages
      </Button>
    </section>
  );
}
