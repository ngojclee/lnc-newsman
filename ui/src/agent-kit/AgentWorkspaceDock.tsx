import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type MouseEvent as ReactMouseEvent,
  type ReactNode,
} from "react";
import type { SessionArtifactFileV2, WorkspaceTabKindV2, WorkspaceTabV2 } from "./agentTypesV2";

export interface DockLauncherOption {
  kind: WorkspaceTabKindV2;
  label: string;
  targetId?: string;
}

export interface AgentWorkspaceDockProps {
  storageKey: string;
  threadId: string;
  open?: boolean;
  onCloseDock?: () => void;
  onBackToConversation?: () => void;
  backLabel?: string;
  tabs: WorkspaceTabV2[];
  activeTabId: string | null;
  onSelectTab: (tabId: string) => void;
  onCloseTab?: (tabId: string) => void;
  onLaunchTab?: (opt: DockLauncherOption) => void;
  launcherOptions?: DockLauncherOption[];
  recentlyClosed?: WorkspaceTabV2[];
  onReopenClosed?: (tab: WorkspaceTabV2) => void;
  renderTabBody: (tab: WorkspaceTabV2) => ReactNode;
  onPromptTrigger?: (prompt: string, target: HTMLElement) => void;
  defaultWidth?: number;
  minWidth?: number;
  maxWidth?: number;
  /** When rendered inside `AgentSplitLayout`'s right pane, fills 100% width without a duplicate resizer. */
  embeddedInSplitPane?: boolean;
  emptyState?: ReactNode;
  className?: string;
}

export interface ThreadDockStateV2 {
  tabs: WorkspaceTabV2[];
  activeId: string | null;
  closed: WorkspaceTabV2[];
}

/**
 * Hook that stashes open & recently-closed dock tabs per `threadId`,
 * plus 1-line helpers (`openFileTab`, `closeTab`, `reopenClosed`) wired
 * directly to `AgentSessionActivity.onOpenFile`.
 */
export function useThreadTabStash(initialTabs: WorkspaceTabV2[] = []) {
  const [stash, setStash] = useState<Record<string, ThreadDockStateV2>>({});

  const defaultState = (): ThreadDockStateV2 => ({
    tabs: initialTabs,
    activeId: initialTabs[0]?.id ?? null,
    closed: [],
  });

  const getThreadTabs = (threadId: string): ThreadDockStateV2 =>
    stash[threadId] ?? defaultState();

  const setThreadTabs = (
    threadId: string,
    updater: (prev: ThreadDockStateV2) => ThreadDockStateV2,
  ) => {
    setStash((prev) => {
      const cur = prev[threadId] ?? defaultState();
      return { ...prev, [threadId]: updater(cur) };
    });
  };

  const openFileTab = (threadId: string, file: SessionArtifactFileV2) => {
    const tabId = `file:${file.id || file.filename}`;
    setThreadTabs(threadId, (cur) => {
      const exists = cur.tabs.some((t) => t.id === tabId);
      const nextTab: WorkspaceTabV2 = {
        id: tabId,
        kind: "preview",
        title: file.filename,
        targetId: file.uri || file.id,
        closable: true,
        meta: { file },
      };
      return {
        tabs: exists
          ? cur.tabs.map((t) => (t.id === tabId ? nextTab : t))
          : [...cur.tabs, nextTab],
        activeId: tabId,
        closed: cur.closed.filter((c) => c.id !== tabId),
      };
    });
  };

  const closeTab = (threadId: string, tabId: string) => {
    setThreadTabs(threadId, (cur) => {
      const target = cur.tabs.find((t) => t.id === tabId);
      if (!target || target.closable === false) return cur;
      const remaining = cur.tabs.filter((t) => t.id !== tabId);
      const nextActive =
        cur.activeId === tabId ? (remaining[remaining.length - 1]?.id ?? null) : cur.activeId;
      return {
        tabs: remaining,
        activeId: nextActive,
        closed: [target, ...cur.closed.filter((c) => c.id !== tabId)].slice(0, 8),
      };
    });
  };

  const reopenClosed = (threadId: string, tab: WorkspaceTabV2) => {
    setThreadTabs(threadId, (cur) => ({
      tabs: cur.tabs.some((t) => t.id === tab.id) ? cur.tabs : [...cur.tabs, tab],
      activeId: tab.id,
      closed: cur.closed.filter((c) => c.id !== tab.id),
    }));
  };

  const selectTab = (threadId: string, tabId: string) => {
    setThreadTabs(threadId, (cur) => ({
      ...cur,
      activeId: tabId,
    }));
  };

  return {
    stash,
    getThreadTabs,
    ensure: getThreadTabs,
    selectTab,
    setThreadTabs,
    openFileTab,
    closeTab,
    reopenClosed,
  };
}

/**
 * Right-side browser-tabbed workspace dock (`lnc-app-agents v2.2`):
 * - Drag-resizable via CSS variable `--ak-dock-width` (Zero React state churn on pointermove)
 * - Deferred body mount for heavy preview tabs while keeping Activity/Notes snappy
 * - `+` DockLauncher menu and `[data-prompt]` delegated prompt triggers
 */
export function AgentWorkspaceDock({
  storageKey,
  threadId,
  open = true,
  onCloseDock,
  onBackToConversation,
  backLabel = "Back to conversation",
  tabs,
  activeTabId,
  onSelectTab,
  onCloseTab,
  onLaunchTab,
  launcherOptions = [],
  recentlyClosed = [],
  onReopenClosed,
  renderTabBody,
  onPromptTrigger,
  defaultWidth = 420,
  minWidth = 300,
  maxWidth = 760,
  embeddedInSplitPane = false,
  emptyState,
  className = "",
}: AgentWorkspaceDockProps) {
  const shellRef = useRef<HTMLDivElement>(null);
  const draggingRef = useRef(false);
  const startXRef = useRef(0);
  const startWidthRef = useRef(defaultWidth);
  const [launcherOpen, setLauncherOpen] = useState(false);

  const clampWidth = (w: number) => Math.max(minWidth, Math.min(maxWidth, Math.round(w)));

  const readSavedWidth = (): number => {
    try {
      const raw = localStorage.getItem(`lnc-ak-dock:${storageKey}`);
      const parsed = raw ? Number(raw) : NaN;
      return Number.isFinite(parsed) ? clampWidth(parsed) : defaultWidth;
    } catch {
      return defaultWidth;
    }
  };

  useEffect(() => {
    if (embeddedInSplitPane) return;
    const el = shellRef.current;
    if (!el) return;
    el.style.setProperty("--ak-dock-width", `${readSavedWidth()}px`);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [storageKey, embeddedInSplitPane]);

  const activeTab = useMemo(
    () => tabs.find((t) => t.id === activeTabId) ?? tabs[0] ?? null,
    [tabs, activeTabId],
  );

  // Defer mounting heavy preview/artifact DOM on rapid tab changes while keeping Activity/Notes immediate.
  const mountKey = activeTab ? `${threadId}::${activeTab.id}` : "";
  const [mountedKey, setMountedKey] = useState(mountKey);
  useEffect(() => {
    if (!mountKey) {
      setMountedKey("");
      return;
    }
    if (activeTab?.kind === "activity" || activeTab?.kind === "notes") {
      setMountedKey(mountKey);
      return;
    }
    const timer = window.setTimeout(() => setMountedKey(mountKey), 180);
    return () => window.clearTimeout(timer);
  }, [mountKey, activeTab?.kind]);

  const handleContainerClick = (e: ReactMouseEvent<HTMLDivElement>) => {
    if (!onPromptTrigger) return;
    const target = (e.target as HTMLElement | null)?.closest<HTMLElement>("[data-prompt]");
    if (!target) return;
    const prompt = target.getAttribute("data-prompt")?.trim();
    if (prompt) {
      e.preventDefault();
      onPromptTrigger(prompt, target);
    }
  };

  if (!open) return null;

  return (
    <aside
      ref={shellRef}
      className={`ak-dock-shell${embeddedInSplitPane ? " ak-dock-shell--embedded" : ""} ${className}`.trim()}
      style={
        embeddedInSplitPane
          ? undefined
          : ({ "--ak-dock-width": `${defaultWidth}px` } as CSSProperties)
      }
      aria-label="Agent workspace dock"
      onClick={handleContainerClick}
    >
      {!embeddedInSplitPane ? (
        <div
          role="separator"
          tabIndex={0}
          aria-orientation="vertical"
          aria-label="Resize workspace dock"
          className="ak-resizer"
          onDoubleClick={() => {
            const el = shellRef.current;
            if (!el) return;
            el.style.setProperty("--ak-dock-width", `${defaultWidth}px`);
            try {
              localStorage.setItem(`lnc-ak-dock:${storageKey}`, String(defaultWidth));
            } catch {
              // ignore
            }
          }}
          onPointerDown={(e) => {
            const el = shellRef.current;
            if (!el) return;
            e.currentTarget.setPointerCapture(e.pointerId);
            draggingRef.current = true;
            startXRef.current = e.clientX;
            startWidthRef.current = el.getBoundingClientRect().width || defaultWidth;
          }}
          onPointerMove={(e) => {
            if (!draggingRef.current || !shellRef.current) return;
            const next = clampWidth(startWidthRef.current + (startXRef.current - e.clientX));
            shellRef.current.style.setProperty("--ak-dock-width", `${next}px`);
          }}
          onPointerUp={() => {
            if (!draggingRef.current || !shellRef.current) return;
            draggingRef.current = false;
            const finalW = clampWidth(shellRef.current.getBoundingClientRect().width);
            try {
              localStorage.setItem(`lnc-ak-dock:${storageKey}`, String(finalW));
            } catch {
              // ignore
            }
          }}
          onKeyDown={(e) => {
            const el = shellRef.current;
            if (!el) return;
            const cur = el.getBoundingClientRect().width || defaultWidth;
            if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
              e.preventDefault();
              const next = clampWidth(cur + (e.key === "ArrowLeft" ? 24 : -24));
              el.style.setProperty("--ak-dock-width", `${next}px`);
              try {
                localStorage.setItem(`lnc-ak-dock:${storageKey}`, String(next));
              } catch {
                // ignore
              }
            }
          }}
        />
      ) : null}

      <div className="ak-dock-main">
        <div className="ak-dock-tabbar" role="tablist" aria-label="Workspace dock tabs">
          <div className="ak-dock-tabs-scroll">
            {tabs.map((tab) => {
              const active = activeTab?.id === tab.id;
              return (
                <div key={tab.id} className={`ak-dock-tab${active ? " active" : ""}`}>
                  <button
                    type="button"
                    role="tab"
                    aria-selected={active}
                    className="ak-dock-tab-btn"
                    onClick={() => onSelectTab(tab.id)}
                    title={tab.reboundFromKey ? `${tab.title} (rebound)` : tab.title}
                  >
                    <span className="ak-truncate">{tab.title}</span>
                    {tab.reboundFromKey ? <span className="ak-select-badge">↺</span> : null}
                  </button>
                  {tab.closable !== false && onCloseTab ? (
                    <button
                      type="button"
                      className="ak-dock-tab-close"
                      aria-label={`Close ${tab.title}`}
                      onClick={() => onCloseTab(tab.id)}
                    >
                      ×
                    </button>
                  ) : null}
                </div>
              );
            })}
          </div>

          {(launcherOptions.length > 0 && onLaunchTab) || (recentlyClosed.length > 0 && onReopenClosed) ? (
            <div className="ak-dock-launcher-wrap">
              <button
                type="button"
                className="ak-btn ak-btn-sm"
                aria-haspopup="menu"
                aria-expanded={launcherOpen}
                aria-label="Open dock tab"
                onClick={() => setLauncherOpen((v) => !v)}
              >
                +
              </button>
              {launcherOpen ? (
                <div className="engine-select-panel ak-select-panel ak-dock-launcher-menu" role="menu">
                  {launcherOptions.map((opt) => (
                    <button
                      key={`${opt.kind}:${opt.targetId ?? opt.label}`}
                      type="button"
                      role="menuitem"
                      className="engine-select-option ak-select-option"
                      onClick={() => {
                        onLaunchTab?.(opt);
                        setLauncherOpen(false);
                      }}
                    >
                      <span className="ak-truncate">{opt.label}</span>
                    </button>
                  ))}
                  {recentlyClosed.length > 0 && onReopenClosed ? (
                    <>
                      <div className="ak-dock-menu-divider">Recently closed</div>
                      {recentlyClosed.slice(0, 6).map((tab) => (
                        <button
                          key={`reopen:${tab.id}`}
                          type="button"
                          role="menuitem"
                          className="engine-select-option ak-select-option"
                          onClick={() => {
                            onReopenClosed(tab);
                            setLauncherOpen(false);
                          }}
                        >
                          <span className="ak-truncate">↺ {tab.title}</span>
                        </button>
                      ))}
                    </>
                  ) : null}
                </div>
              ) : null}
            </div>
          ) : null}

          {onCloseDock ? (
            <button
              type="button"
              className="ak-dock-close-btn"
              aria-label="Close workspace dock"
              title="Close panel"
              onClick={onCloseDock}
            >
              ×
            </button>
          ) : null}
        </div>

        {onBackToConversation ? (
          <div className="ak-dock-backbar">
            <button
              type="button"
              className="ak-dock-back-btn"
              onClick={onBackToConversation}
            >
              <span aria-hidden="true">←</span>
              <span>{backLabel}</span>
            </button>
          </div>
        ) : null}

        <div className="ak-dock-body">
          {!activeTab ? (
            emptyState ?? <div className="ak-dock-empty">No open workspace tab</div>
          ) : mountedKey !== mountKey ? (
            <div className="ak-dock-loading" aria-busy="true">
              Loading {activeTab.title}...
            </div>
          ) : (
            renderTabBody(activeTab)
          )}
        </div>
      </div>
    </aside>
  );
}

export default AgentWorkspaceDock;
