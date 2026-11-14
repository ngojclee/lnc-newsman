import React, { useCallback, useEffect, useRef, useState } from "react";

export interface AgentSplitLayoutProps {
  /** Unique persistence key, e.g. `"newsman:chat"` or `"newsman:studio"`. */
  storageKey: string;
  /** Left rail content (Thread list, Agent list, Settings nav). */
  left?: React.ReactNode;
  leftDefaultWidth?: number;
  leftMinWidth?: number;
  leftMaxWidth?: number;
  leftCollapsed?: boolean;
  /** Center main workspace (always `flex: 1; min-width: 0`). */
  center: React.ReactNode;
  /** Optional right drawer (Activity/Notes drawer, Co-Writer companion). */
  right?: React.ReactNode;
  rightDefaultWidth?: number;
  rightMinWidth?: number;
  rightMaxWidth?: number;
  rightCollapsed?: boolean;
  height?: string | number;
  className?: string;
}

function clamp(val: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, Math.round(val)));
}

function readStoredWidth(key: string, fallback: number, min: number, max: number): number {
  if (typeof window === "undefined") return fallback;
  try {
    const raw = window.localStorage.getItem(`lnc-ak-split:${key}`);
    if (!raw) return fallback;
    const parsed = Number(raw);
    return Number.isFinite(parsed) ? clamp(parsed, min, max) : fallback;
  } catch {
    return fallback;
  }
}

function saveStoredWidth(key: string, value: number): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(`lnc-ak-split:${key}`, String(Math.round(value)));
  } catch {
    /* ignore quota / private mode */
  }
}

export const AgentSplitLayout: React.FC<AgentSplitLayoutProps> = ({
  storageKey,
  left,
  leftDefaultWidth = 264,
  leftMinWidth = 190,
  leftMaxWidth = 440,
  leftCollapsed = false,
  center,
  right,
  rightDefaultWidth = 308,
  rightMinWidth = 220,
  rightMaxWidth = 500,
  rightCollapsed = false,
  height = "100%",
  className = "",
}) => {
  const [leftW, setLeftW] = useState<number>(() =>
    readStoredWidth(`${storageKey}:left`, leftDefaultWidth, leftMinWidth, leftMaxWidth)
  );
  const [rightW, setRightW] = useState<number>(() =>
    readStoredWidth(`${storageKey}:right`, rightDefaultWidth, rightMinWidth, rightMaxWidth)
  );
  const [activeSide, setActiveSide] = useState<"left" | "right" | null>(null);
  const dragRef = useRef<{
    side: "left" | "right";
    startX: number;
    startW: number;
  } | null>(null);

  useEffect(() => {
    setLeftW(readStoredWidth(`${storageKey}:left`, leftDefaultWidth, leftMinWidth, leftMaxWidth));
    setRightW(readStoredWidth(`${storageKey}:right`, rightDefaultWidth, rightMinWidth, rightMaxWidth));
  }, [storageKey, leftDefaultWidth, leftMinWidth, leftMaxWidth, rightDefaultWidth, rightMinWidth, rightMaxWidth]);

  const beginDrag = useCallback(
    (side: "left" | "right", e: React.PointerEvent<HTMLDivElement>) => {
      e.preventDefault();
      e.currentTarget.setPointerCapture(e.pointerId);
      dragRef.current = {
        side,
        startX: e.clientX,
        startW: side === "left" ? leftW : rightW,
      };
      setActiveSide(side);
    },
    [leftW, rightW]
  );

  const moveDrag = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      const d = dragRef.current;
      if (!d) return;
      const dx = e.clientX - d.startX;
      if (d.side === "left") {
        const next = clamp(d.startW + dx, leftMinWidth, leftMaxWidth);
        setLeftW(next);
        saveStoredWidth(`${storageKey}:left`, next);
      } else {
        const next = clamp(d.startW - dx, rightMinWidth, rightMaxWidth);
        setRightW(next);
        saveStoredWidth(`${storageKey}:right`, next);
      }
    },
    [storageKey, leftMinWidth, leftMaxWidth, rightMinWidth, rightMaxWidth]
  );

  const endDrag = useCallback(() => {
    dragRef.current = null;
    setActiveSide(null);
  }, []);

  const resetSide = useCallback(
    (side: "left" | "right") => {
      if (side === "left") {
        setLeftW(leftDefaultWidth);
        saveStoredWidth(`${storageKey}:left`, leftDefaultWidth);
      } else {
        setRightW(rightDefaultWidth);
        saveStoredWidth(`${storageKey}:right`, rightDefaultWidth);
      }
    },
    [storageKey, leftDefaultWidth, rightDefaultWidth]
  );

  const handleKey = useCallback(
    (side: "left" | "right", e: React.KeyboardEvent<HTMLDivElement>) => {
      if (e.key === "Enter") {
        e.preventDefault();
        resetSide(side);
        return;
      }
      if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
      e.preventDefault();
      const step = e.shiftKey ? 32 : 16;
      const dir = e.key === "ArrowRight" ? 1 : -1;
      if (side === "left") {
        const next = clamp(leftW + dir * step, leftMinWidth, leftMaxWidth);
        setLeftW(next);
        saveStoredWidth(`${storageKey}:left`, next);
      } else {
        const next = clamp(rightW - dir * step, rightMinWidth, rightMaxWidth);
        setRightW(next);
        saveStoredWidth(`${storageKey}:right`, next);
      }
    },
    [leftW, rightW, leftMinWidth, leftMaxWidth, rightMinWidth, rightMaxWidth, resetSide, storageKey]
  );

  const showLeft = Boolean(left) && !leftCollapsed;
  const showRight = Boolean(right) && !rightCollapsed;

  return (
    <div
      className={`ak-split-shell ${activeSide ? "is-resizing" : ""} ${className}`.trim()}
      style={{ height }}
    >
      {showLeft && (
        <>
          <aside className="ak-split-side ak-split-side--left" style={{ width: `${leftW}px` }}>
            {left}
          </aside>
          <div
            role="separator"
            aria-orientation="vertical"
            aria-label="Resize left column (double-click to reset)"
            tabIndex={0}
            title="Drag to resize · Double-click to reset"
            className={`ak-resizer ${activeSide === "left" ? "is-active" : ""}`}
            onPointerDown={(e) => beginDrag("left", e)}
            onPointerMove={moveDrag}
            onPointerUp={endDrag}
            onLostPointerCapture={endDrag}
            onDoubleClick={() => resetSide("left")}
            onKeyDown={(e) => handleKey("left", e)}
          >
            <span className="ak-resizer-grip" />
          </div>
        </>
      )}

      <section className="ak-split-center">{center}</section>

      {showRight && (
        <>
          <div
            role="separator"
            aria-orientation="vertical"
            aria-label="Resize right column (double-click to reset)"
            tabIndex={0}
            title="Drag to resize · Double-click to reset"
            className={`ak-resizer ${activeSide === "right" ? "is-active" : ""}`}
            onPointerDown={(e) => beginDrag("right", e)}
            onPointerMove={moveDrag}
            onPointerUp={endDrag}
            onLostPointerCapture={endDrag}
            onDoubleClick={() => resetSide("right")}
            onKeyDown={(e) => handleKey("right", e)}
          >
            <span className="ak-resizer-grip" />
          </div>
          <aside className="ak-split-side ak-split-side--right" style={{ width: `${rightW}px` }}>
            {right}
          </aside>
        </>
      )}
    </div>
  );
};

export default AgentSplitLayout;
