import { useCallback, useEffect, useMemo, useState, type RefObject } from "react";
import type { TurnOutlineEntryV2 } from "./agentTypesV2";

export interface AgentTurnNavigatorProps {
  entries: TurnOutlineEntryV2[];
  scrollRootRef: RefObject<HTMLElement | null>;
  onJump: (key: string) => void;
  onJumpToBottom?: () => void;
}

/**
 * Left-gutter conversation turn minimap (`v2.2`):
 * - Quiet hairline ticks at rest, expanding on hover with a floating turn preview
 * - Supports `Alt + ArrowUp` / `Alt + ArrowDown` keyboard jumps even when narrow
 */
export function AgentTurnNavigator({
  entries,
  scrollRootRef,
  onJump,
  onJumpToBottom,
}: AgentTurnNavigatorProps) {
  const [activeKey, setActiveKey] = useState<string | null>(null);
  const [hoveredKey, setHoveredKey] = useState<string | null>(null);
  const [hot, setHot] = useState(false);

  const keyList = useMemo(() => entries.map((e) => e.key), [entries]);

  const jumpBy = useCallback(
    (delta: -1 | 1) => {
      if (!keyList.length) return;
      const curIdx = activeKey ? keyList.indexOf(activeKey) : keyList.length - 1;
      const nextIdx = Math.max(0, Math.min(keyList.length - 1, curIdx + delta));
      const nextKey = keyList[nextIdx];
      if (nextKey) {
        setActiveKey(nextKey);
        onJump(nextKey);
      } else if (delta > 0 && onJumpToBottom) {
        onJumpToBottom();
      }
    },
    [activeKey, keyList, onJump, onJumpToBottom],
  );

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (!e.altKey) return;
      if (e.key === "ArrowUp") {
        e.preventDefault();
        jumpBy(-1);
      } else if (e.key === "ArrowDown") {
        e.preventDefault();
        jumpBy(1);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [jumpBy]);

  useEffect(() => {
    const root = scrollRootRef.current;
    if (!root || !keyList.length) return;
    const onScroll = () => {
      const rootTop = root.getBoundingClientRect().top;
      let current: string | null = keyList[0] ?? null;
      for (const k of keyList) {
        const node = root.querySelector<HTMLElement>(`[data-turn-key="${k}"]`);
        if (!node) continue;
        if (node.getBoundingClientRect().top - rootTop <= 140) current = k;
      }
      setActiveKey(current);
    };
    root.addEventListener("scroll", onScroll, { passive: true });
    onScroll();
    return () => root.removeEventListener("scroll", onScroll);
  }, [scrollRootRef, keyList]);

  if (entries.length < 2) return null;

  const hoveredEntry = entries.find((e) => e.key === hoveredKey) ?? null;

  return (
    <nav
      className={`ak-turn-rail${hot ? " is-hot" : ""}`}
      aria-label="Conversation turn navigator"
      onMouseEnter={() => setHot(true)}
      onMouseLeave={() => {
        setHot(false);
        setHoveredKey(null);
      }}
    >
      <div className="ak-turn-track">
        {entries.map((entry, idx) => {
          const isActive = entry.key === activeKey;
          const weight = Math.max(0.2, Math.min(1, entry.weight ?? 0.5));
          const widthPx = Math.round((hot ? 10 : 6) + weight * (hot ? 12 : 6) + (isActive ? 4 : 0));
          return (
            <button
              key={entry.key}
              type="button"
              className={`ak-turn-tick${isActive ? " active" : ""}`}
              style={{ width: `${widthPx}px` }}
              aria-label={`Jump to turn ${idx + 1}: ${entry.prompt.slice(0, 48)}`}
              onMouseEnter={() => setHoveredKey(entry.key)}
              onFocus={() => setHoveredKey(entry.key)}
              onClick={() => {
                setActiveKey(entry.key);
                onJump(entry.key);
              }}
            />
          );
        })}
      </div>

      {hoveredEntry ? (
        <div className="ak-turn-preview" role="tooltip">
          {hoveredEntry.mode ? (
            <span className="ak-select-badge">{hoveredEntry.mode}</span>
          ) : null}
          <p className="ak-turn-preview-text">{hoveredEntry.prompt}</p>
          <span className="ak-turn-preview-hint">Alt + ↑ / ↓</span>
        </div>
      ) : null}
    </nav>
  );
}

export default AgentTurnNavigator;
