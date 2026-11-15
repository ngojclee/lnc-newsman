import { useState } from "react";
import { createPortal } from "react-dom";
import type { SelectionAnchorV2, SideChatResponderV2 } from "./agentTypesV2";

export interface AgentSelectionToolbarProps {
  anchor: SelectionAnchorV2 | null;
  coords: { x: number; y: number } | null;
  responders?: SideChatResponderV2[];
  selectedResponderSlug?: string;
  onSelectResponder?: (slug: string) => void;
  onListen?: (anchor: SelectionAnchorV2) => void;
  onAddToChat: (anchor: SelectionAnchorV2) => void;
  onAsk: (anchor: SelectionAnchorV2, responderSlug?: string) => void;
  onDismiss?: () => void;
  showListen?: boolean;
  className?: string;
}

/**
 * Floating 272×36px selection bar (`lnc-app-agents v2.2`).
 * Uses `onPointerDown` `preventDefault()` across every control so clicking
 * `Listen`, `Add to chat`, or `Ask` preserves the user's DOM selection.
 */
export function AgentSelectionToolbar({
  anchor,
  coords,
  responders = [],
  selectedResponderSlug,
  onSelectResponder,
  onListen,
  onAddToChat,
  onAsk,
  onDismiss,
  showListen = true,
  className = "",
}: AgentSelectionToolbarProps) {
  const [pickerOpen, setPickerOpen] = useState(false);

  if (!anchor || !coords || typeof document === "undefined") return null;

  const toolbarW = 272;
  const toolbarH = 36;
  const clampedLeft = Math.max(
    12,
    Math.min(Math.round(coords.x - toolbarW / 2), window.innerWidth - toolbarW - 12),
  );
  const openBelow = coords.y < toolbarH + 20;
  const clampedTop = openBelow
    ? Math.round(coords.y + 12)
    : Math.max(12, Math.round(coords.y - toolbarH - 10));

  const activeResponder =
    responders.find((r) => r.slug === selectedResponderSlug) ?? responders[0];

  return createPortal(
    <div
      role="toolbar"
      aria-label="Selection actions"
      className={`ak-selection-bar ${className}`.trim()}
      style={{
        left: clampedLeft,
        top: clampedTop,
        width: toolbarW,
        height: toolbarH,
      }}
      onPointerDown={(e) => e.preventDefault()}
    >
      {showListen && onListen ? (
        <button
          type="button"
          className="ak-selection-btn"
          onPointerDown={(e) => e.preventDefault()}
          onClick={() => {
            onListen(anchor);
            onDismiss?.();
          }}
        >
          Listen
        </button>
      ) : null}

      <button
        type="button"
        className="ak-selection-btn"
        onPointerDown={(e) => e.preventDefault()}
        onClick={() => {
          onAddToChat(anchor);
          onDismiss?.();
        }}
      >
        Add to chat
      </button>

      <div className="ak-selection-ask-group">
        <button
          type="button"
          className="ak-selection-btn ak-selection-btn--primary"
          onPointerDown={(e) => e.preventDefault()}
          onClick={() => {
            onAsk(anchor, activeResponder?.slug);
            onDismiss?.();
          }}
        >
          Ask
        </button>

        {responders.length > 0 ? (
          <button
            type="button"
            className="ak-selection-chevron"
            aria-haspopup="menu"
            aria-expanded={pickerOpen}
            aria-label="Choose responder"
            onPointerDown={(e) => e.preventDefault()}
            onClick={() => setPickerOpen((v) => !v)}
          >
            ▾
          </button>
        ) : null}
      </div>

      {pickerOpen && responders.length > 0 ? (
        <div
          className="engine-select-panel ak-select-panel ak-selection-menu"
          role="menu"
          onPointerDown={(e) => e.preventDefault()}
        >
          {responders.map((resp) => (
            <button
              key={resp.slug}
              type="button"
              role="menuitem"
              className={`engine-select-option ak-select-option${
                resp.slug === activeResponder?.slug ? " selected" : ""
              }`}
              onPointerDown={(e) => e.preventDefault()}
              onClick={() => {
                onSelectResponder?.(resp.slug);
                onAsk(anchor, resp.slug);
                setPickerOpen(false);
                onDismiss?.();
              }}
            >
              <span className="ak-truncate">
                {resp.emoji ? `${resp.emoji} ` : ""}
                {resp.displayName}
              </span>
              {resp.roleHint ? <span className="ak-select-badge">{resp.roleHint}</span> : null}
            </button>
          ))}
        </div>
      ) : null}
    </div>,
    document.body,
  );
}

export default AgentSelectionToolbar;
