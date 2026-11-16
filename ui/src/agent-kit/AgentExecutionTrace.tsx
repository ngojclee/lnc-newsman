import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { AgentMarkdown } from "./AgentMarkdown";
import { plainPreview, type AgentStreamPhaseV2, type TraceStepV2 } from "./agentTypesV2";

export { plainPreview };

export interface AgentExecutionTraceProps {
  phase: AgentStreamPhaseV2;
  /** Optional live reasoning/thinking markdown text */
  thinkingText?: string;
  steps: TraceStepV2[];
  durationSeconds?: number | null;
  /** Optional responder badge (when an App Agent answers in role-play mode) */
  responderLabel?: string;
  responderAvatar?: string;
  /** Optional transient reconnect status ("Reconnecting stream…") */
  reconnectNotice?: string | null;
  onCitationClick?: (articleId: number) => void;
  className?: string;
}

const PHASE_LABELS: Record<AgentStreamPhaseV2, string> = {
  idle: "Ready",
  planning: "Structuring approach…",
  reasoning: "Thinking…",
  searching: "Searching context & tools…",
  weaving: "Synthesizing evidence…",
  responding: "Writing answer…",
  settled: "Thought",
  error: "Interrupted",
};

function SemanticPhaseOrb({ phase }: { phase: AgentStreamPhaseV2 }) {
  return (
    <span className={`ak-orb ak-orb--${phase}`} aria-hidden="true">
      <span className="ak-orb-dot" />
      <span className="ak-orb-dot" />
      <span className="ak-orb-dot" />
    </span>
  );
}

/**
 * Sticky-bottom scroll box for live thinking streams: sticks to the newest
 * line while streaming, and immediately stops auto-scrolling the moment the
 * reader scrolls up (`> 30px`) to inspect earlier reasoning.
 */
function StickyThinkingBox({ active, children }: { active: boolean; children: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  const stuckRef = useRef(true);

  useEffect(() => {
    if (!active || !stuckRef.current) return;
    const el = ref.current;
    if (el) el.scrollTop = el.scrollHeight;
  });

  useEffect(() => {
    if (active) stuckRef.current = true;
  }, [active]);

  const onScroll = useCallback(() => {
    const el = ref.current;
    if (!el) return;
    stuckRef.current = el.scrollHeight - el.scrollTop - el.clientHeight < 30;
  }, []);

  return (
    <div ref={ref} onScroll={onScroll} className="ak-trace-thinking-scroll">
      {children}
    </div>
  );
}

/**
 * DeepTutor-harvested Execution Trace & Semantic Thinking Stream (`v2.3`):
 * - Semantic Phase Orb (`planning`, `reasoning`, `searching`, `weaving`, `responding`, `settled`)
 * - `followOpen` auto-expansion while actively thinking/searching, then smooth
 *   `grid-template-rows: 0fr` auto-collapse once the final answer begins streaming
 *   (unless the user manually clicked to pin it open: "user click wins")
 * - Human-verb tool rows (`Scanning latest articles` · `VN`) instead of raw machine names
 */
export function AgentExecutionTrace({
  phase,
  thinkingText = "",
  steps,
  durationSeconds,
  responderLabel,
  responderAvatar,
  reconnectNotice,
  onCitationClick,
  className = "",
}: AgentExecutionTraceProps) {
  const [userChoice, setUserChoice] = useState<boolean | null>(null);
  const [expandedStepIds, setExpandedStepIds] = useState<Record<string, boolean>>({});

  const isLive = phase !== "idle" && phase !== "settled" && phase !== "error";
  const followOpen =
    phase === "planning" ||
    phase === "reasoning" ||
    phase === "searching" ||
    phase === "weaving";
  const hasContent = Boolean(thinkingText.trim()) || steps.length > 0;
  const open = userChoice !== null ? userChoice : followOpen && hasContent;

  const summaryLabel = useMemo(() => {
    const parts: string[] = [];
    if (typeof durationSeconds === "number" && durationSeconds > 0) {
      parts.push(
        durationSeconds < 10
          ? `${durationSeconds.toFixed(1)}s`
          : `${Math.round(durationSeconds)}s`,
      );
    }
    if (steps.length > 0) {
      parts.push(`${steps.length} ${steps.length === 1 ? "step" : "steps"}`);
    }
    return parts.join(" · ");
  }, [durationSeconds, steps.length]);

  const activeHeadline = useMemo(() => {
    if (isLive) {
      const runningStep = [...steps].reverse().find((s) => s.state === "running");
      if (runningStep) {
        return runningStep.chip ? `${runningStep.verb} · ${runningStep.chip}` : runningStep.verb;
      }
      if (thinkingText.trim()) {
        return plainPreview(thinkingText, 84) || PHASE_LABELS[phase];
      }
    }
    return PHASE_LABELS[phase];
  }, [isLive, phase, steps, thinkingText]);

  if (!hasContent && phase === "idle" && !reconnectNotice) return null;

  return (
    <div className={`ak-trace-shell ${className}`.trim()}>
      <div className="ak-trace-hdr-row">
        {responderLabel ? (
          <span className="ak-trace-responder-pill">
            {responderAvatar ? <span aria-hidden="true">{responderAvatar}</span> : null}
            <span>{responderLabel}</span>
          </span>
        ) : null}

        {hasContent ? (
          <button
            type="button"
            className={`ak-trace-trigger${isLive ? " is-live" : " is-settled"}`}
            aria-expanded={open}
            aria-live="polite"
            onClick={() => setUserChoice((prev) => !(prev ?? open))}
          >
            <SemanticPhaseOrb phase={phase} />
            <span className={`ak-trace-headline${isLive ? " is-breathing" : ""}`}>
              {activeHeadline}
            </span>
            {summaryLabel ? <span className="ak-trace-meta">· {summaryLabel}</span> : null}
            <span className={`ak-trace-caret${open ? " open" : ""}`} aria-hidden="true">
              ▸
            </span>
          </button>
        ) : (
          <div className="ak-trace-trigger is-live" role="status" aria-live="polite">
            <SemanticPhaseOrb phase={phase} />
            <span className="ak-trace-headline is-breathing">{activeHeadline}</span>
          </div>
        )}

        {reconnectNotice ? (
          <span className="ak-trace-reconnect" role="status">
            ↻ {reconnectNotice}
          </span>
        ) : null}
      </div>

      <div className={`ak-trace-fold${open ? " open" : ""}`}>
        <div className="ak-trace-fold-inner">
          <div className="ak-trace-stack">
            {thinkingText.trim() ? (
              <StickyThinkingBox active={isLive}>
                <AgentMarkdown
                  content={thinkingText}
                  onCitationClick={onCitationClick}
                  className="ak-md--trace"
                />
              </StickyThinkingBox>
            ) : null}

            {steps.map((step) => {
              const stepOpen = Boolean(expandedStepIds[step.id]);
              return (
                <div key={step.id} className="ak-trace-step">
                  <button
                    type="button"
                    className="ak-trace-step-btn"
                    disabled={!step.detail}
                    onClick={() =>
                      step.detail &&
                      setExpandedStepIds((p) => ({ ...p, [step.id]: !p[step.id] }))
                    }
                  >
                    <span className={`ak-trace-dot ak-trace-dot--${step.state}`} />
                    <span className="ak-trace-step-verb">{step.verb}</span>
                    {step.chip ? (
                      <span className={`ak-trace-step-chip${step.chipMono ? " mono" : ""}`}>
                        {step.chip}
                      </span>
                    ) : null}
                    {typeof step.durationMs === "number" ? (
                      <span className="ak-trace-meta">
                        {step.durationMs >= 1000
                          ? `${(step.durationMs / 1000).toFixed(1)}s`
                          : `${step.durationMs}ms`}
                      </span>
                    ) : null}
                  </button>
                  {step.detail && stepOpen ? (
                    <pre className="ak-trace-step-detail">{step.detail}</pre>
                  ) : null}
                </div>
              );
            })}
          </div>
        </div>
      </div>
    </div>
  );
}
export default AgentExecutionTrace;
