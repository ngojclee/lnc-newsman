import { useEffect, useRef, useState } from "react";
import type { ContextBudgetV2 } from "./agentTypesV2";

export interface AgentContextBudgetChipProps {
  budget: ContextBudgetV2 | null;
}

const SEGMENT_COLORS: Record<string, string> = {
  messages: "#6366f1",
  system_prompt: "#0ea5e9",
  system_tools: "#14b8a6",
  mcp_tools: "#10b981",
  skills: "#06b6d4",
  memory: "#a855f7",
  sources: "#0891b2",
  workspace: "#65a30d",
};

const SEGMENT_LABELS: Record<string, string> = {
  messages: "Conversation",
  system_prompt: "System",
  system_tools: "App Tools",
  mcp_tools: "MCP Tools",
  skills: "Skills",
  memory: "Memory",
  sources: "Sources",
  workspace: "Workspace",
};

function formatTokens(n: number): string {
  if (!Number.isFinite(n) || n <= 0) return "0";
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1).replace(/\.0$/, "")}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1).replace(/\.0$/, "")}k`;
  return String(Math.round(n));
}

/**
 * 16px SVG context usage ring + breakdown popover (`v2.2`):
 * shows stacked token allocation across System, Skills, MCP Tools, Memory,
 * Sources, and Conversation alongside remaining free window.
 */
export function AgentContextBudgetChip({ budget }: AgentContextBudgetChipProps) {
  const [open, setOpen] = useState(false);
  const wrapRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDocClick = (e: MouseEvent) => {
      if (wrapRef.current && !wrapRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onDocClick);
    window.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDocClick);
      window.removeEventListener("keydown", onKey);
    };
  }, [open]);

  if (!budget || !budget.window) return null;

  const pct = Math.min(100, Math.max(0, (budget.used_tokens / budget.window) * 100));
  const r = 6.5;
  const circ = 2 * Math.PI * r;
  const offset = circ * (1 - pct / 100);

  return (
    <div ref={wrapRef} className="ak-ctx-wrap">
      <button
        type="button"
        className="ak-btn ak-btn-sm ak-ctx-btn"
        aria-expanded={open}
        title={`${formatTokens(budget.used_tokens)} / ${formatTokens(budget.window)} tokens`}
        onClick={() => setOpen((v) => !v)}
      >
        <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" className="ak-ctx-ring">
          <circle cx="8" cy="8" r={r} fill="none" stroke="currentColor" strokeWidth="2" opacity="0.22" />
          <circle
            cx="8"
            cy="8"
            r={r}
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeDasharray={circ}
            strokeDashoffset={offset}
          />
        </svg>
        <span>{pct < 1 && pct > 0 ? "<1" : Math.round(pct)}% ctx</span>
      </button>

      {open ? (
        <div className="ak-ctx-popover" role="dialog" aria-label="Context budget breakdown">
          <div className="ak-ctx-hdr">
            <strong>Context Window</strong>
            <span>
              {formatTokens(budget.used_tokens)} / {formatTokens(budget.window)}
            </span>
          </div>
          <div className="ak-ctx-bar" aria-hidden="true">
            {budget.segments.map((seg) => {
              const segPct = Math.max(0.5, (seg.tokens / budget.window) * 100);
              return (
                <span
                  key={seg.key}
                  style={{
                    width: `${segPct}%`,
                    background: SEGMENT_COLORS[seg.key] || "#64748b",
                  }}
                />
              );
            })}
          </div>
          <ul className="ak-ctx-list">
            {budget.segments.map((seg) => (
              <li key={seg.key} className="ak-ctx-row">
                <span className="ak-ctx-swatch" style={{ background: SEGMENT_COLORS[seg.key] || "#64748b" }} />
                <span className="ak-truncate">{seg.label || SEGMENT_LABELS[seg.key] || seg.key}</span>
                <span className="ak-ctx-val">{formatTokens(seg.tokens)}</span>
              </li>
            ))}
            <li className="ak-ctx-row ak-ctx-free">
              <span className="ak-ctx-swatch" style={{ background: "var(--ak-border)" }} />
              <span>Free space</span>
              <span className="ak-ctx-val">{formatTokens(budget.free_tokens)}</span>
            </li>
          </ul>
        </div>
      ) : null}
    </div>
  );
}

export default AgentContextBudgetChip;
