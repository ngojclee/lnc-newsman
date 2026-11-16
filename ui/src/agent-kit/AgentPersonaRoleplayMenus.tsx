import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import {
  foldVi,
  type ComposerModeOptionV2,
  type DeclaredAppAgentV2,
  type ResourceCategoryItemV2,
  type ResponseStyleOptionV2,
} from "./agentTypesV2";

function useOutsideClose(open: boolean, onClose: () => void) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    const onRoute = () => onClose();
    document.addEventListener("mousedown", handler);
    window.addEventListener("keydown", onKey);
    window.addEventListener("hashchange", onRoute);
    window.addEventListener("popstate", onRoute);
    return () => {
      document.removeEventListener("mousedown", handler);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("hashchange", onRoute);
      window.removeEventListener("popstate", onRoute);
    };
  }, [open, onClose]);
  return ref;
}

/* 1. `+ Resources ▾` Searchable Reference & Knowledge Popover (Image 2) */
export function AgentResourcesPopover({
  items,
  selectedTotal = 0,
  onSelectCategory,
  onAttachFiles,
  label = "Resources",
  searchPlaceholder = "Search resource types",
}: {
  items: ResourceCategoryItemV2[];
  selectedTotal?: number;
  onSelectCategory: (key: string) => void;
  onAttachFiles?: () => void;
  label?: string;
  searchPlaceholder?: string;
}) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const wrapRef = useOutsideClose(open, () => setOpen(false));

  const filtered = useMemo(() => {
    const q = foldVi(query);
    return q ? items.filter((i) => foldVi(i.label).includes(q)) : items;
  }, [items, query]);

  const gridItems = filtered.filter((i) => i.group === "reference" && !i.fullRow);
  const fullRows = filtered.filter((i) => i.group === "linked" || i.fullRow);

  return (
    <div ref={wrapRef} className="ak-pop-anchor">
      <button
        type="button"
        className={`ak-tray-pill${selectedTotal > 0 ? " is-active" : ""}`}
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        <span className="ak-tray-plus">+</span>
        {selectedTotal > 0 ? <span className="ak-tray-active-dot" /> : null}
        <span>{label}</span>
        <span className="ak-tray-chev">▾</span>
      </button>

      {open ? (
        <div className="ak-pop-card ak-pop-card--resources" role="dialog" aria-label={label}>
          <div className="ak-pop-hdr">
            <strong>{label}</strong>
            <button type="button" className="ak-dock-close-btn" onClick={() => setOpen(false)}>
              ×
            </button>
          </div>
          <div className="ak-pop-search-row">
            <input
              type="search"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={searchPlaceholder}
              className="ak-pop-search"
              autoFocus
            />
          </div>
          <div className="ak-pop-section-lbl">Reference materials</div>
          {onAttachFiles && (!query || foldVi("Attach files").includes(foldVi(query))) ? (
            <button
              type="button"
              className="ak-pop-row"
              onClick={() => {
                onAttachFiles();
                setOpen(false);
              }}
            >
              <span>📎</span>
              <span className="ak-pop-row-title">Attach files</span>
            </button>
          ) : null}

          {gridItems.length > 0 ? (
            <div className="ak-res-grid">
              {gridItems.map((it) => (
                <button
                  key={it.key}
                  type="button"
                  className={`ak-res-cell${it.selectedCount ? " is-selected" : ""}`}
                  onClick={() => {
                    onSelectCategory(it.key);
                    setOpen(false);
                  }}
                >
                  <span className="ak-truncate">{it.label}</span>
                  <span className="ak-tray-chev">›</span>
                </button>
              ))}
            </div>
          ) : null}

          {fullRows.map((it) => (
            <button
              key={it.key}
              type="button"
              className={`ak-pop-row${it.selectedCount ? " is-selected" : ""}`}
              onClick={() => {
                onSelectCategory(it.key);
                setOpen(false);
              }}
            >
              <span className="ak-pop-row-title ak-truncate">{it.label}</span>
              <span className="ak-pop-row-meta">
                {it.selectedCount ? `${it.selectedCount} selected` : it.summary || ""} ›
              </span>
            </button>
          ))}
        </div>
      ) : null}
    </div>
  );
}

/* 2. `Mode ^` 2-Line Capability Cards Popover (Image 3) */
export function AgentModePopover({
  modes,
  value,
  onChange,
  label = "Mode",
}: {
  modes: ComposerModeOptionV2[];
  value: string;
  onChange: (nextMode: string) => void;
  label?: string;
}) {
  const [open, setOpen] = useState(false);
  const wrapRef = useOutsideClose(open, () => setOpen(false));
  const active = modes.find((m) => m.value === value);

  return (
    <div ref={wrapRef} className="ak-pop-anchor">
      <button
        type="button"
        className={`ak-tray-pill${active ? " is-active" : ""}`}
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        <span>{active?.icon || "❖"}</span>
        <span>{active ? active.title : label}</span>
        <span className="ak-tray-chev">{open ? "▴" : "▾"}</span>
      </button>

      {open ? (
        <div className="ak-pop-card ak-pop-card--modes" role="menu" aria-label={label}>
          {modes.map((m) => {
            const selected = m.value === value;
            return (
              <button
                key={m.value}
                type="button"
                role="menuitemradio"
                aria-checked={selected}
                className={`ak-pop-twoline${selected ? " is-selected" : ""}`}
                onClick={() => {
                  // Clicking the active mode again returns cleanly to plain conversation ("")
                  onChange(selected ? "" : m.value);
                  setOpen(false);
                }}
              >
                <span className="ak-pop-lead-icon" aria-hidden="true">
                  {m.icon || "❖"}
                </span>
                <span className="ak-pop-twoline-body">
                  <span className="ak-pop-row-title">{m.title}</span>
                  <span className="ak-pop-row-sub ak-truncate">{m.description}</span>
                </span>
                {selected ? <span className="ak-pop-check">✓</span> : null}
              </button>
            );
          })}
        </div>
      ) : null}
    </div>
  );
}

/* 3. `Partners ^` Behind-the-Scenes Consult Menu (`Ask partner` vs `Ask subagent` — Image 5) */
export function AgentPartnersConsultPopover({
  agents,
  selectedIds,
  everyTurn,
  onToggleAgent,
  onEveryTurnChange,
  onCreateGroup,
  onManageAgents,
  label = "Partners",
}: {
  agents: DeclaredAppAgentV2[];
  selectedIds: string[];
  everyTurn: boolean;
  onToggleAgent: (id: string) => void;
  onEveryTurnChange: (next: boolean) => void;
  onCreateGroup?: () => void;
  onManageAgents?: () => void;
  label?: string;
}) {
  const [open, setOpen] = useState(false);
  const wrapRef = useOutsideClose(open, () => setOpen(false));
  const partners = agents.filter((a) => a.kind === "partner");
  const subagents = agents.filter((a) => a.kind === "subagent");

  return (
    <div ref={wrapRef} className="ak-pop-anchor">
      <button
        type="button"
        className={`ak-tray-pill${selectedIds.length > 0 ? " is-active" : ""}`}
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        <span>👥</span>
        <span>
          {label}
          {selectedIds.length > 0 ? ` (${selectedIds.length})` : ""}
        </span>
        <span className="ak-tray-chev">{open ? "▴" : "▾"}</span>
      </button>

      {open ? (
        <div className="ak-pop-card ak-pop-card--agents" role="dialog" aria-label="Consult partners">
          {partners.length > 0 ? (
            <div className="ak-pop-group">
              <div className="ak-pop-section-lbl">
                <strong>Ask partner</strong>{" "}
                <span>Assistant consults them, then answers</span>
              </div>
              {partners.map((p) => {
                const checked = selectedIds.includes(p.id);
                return (
                  <button
                    key={p.id}
                    type="button"
                    className={`ak-pop-twoline${checked ? " is-selected" : ""}`}
                    onClick={() => onToggleAgent(p.id)}
                  >
                    <span
                      className="ak-agent-avatar"
                      style={{ background: p.color || "var(--ak-surface-soft)" }}
                    >
                      {p.emoji || p.name.slice(0, 1)}
                    </span>
                    <span className="ak-pop-twoline-body">
                      <span className="ak-pop-row-title">{p.name}</span>
                      {p.description ? (
                        <span className="ak-pop-row-sub ak-truncate">{p.description}</span>
                      ) : null}
                    </span>
                    {checked ? <span className="ak-pop-check">✓</span> : null}
                  </button>
                );
              })}
            </div>
          ) : null}

          {subagents.length > 0 ? (
            <div className="ak-pop-group">
              <div className="ak-pop-section-lbl">
                <strong>Ask subagent</strong>{" "}
                <span>Delegates sub-questions, then answers</span>
              </div>
              {subagents.map((s) => {
                const checked = selectedIds.includes(s.id);
                return (
                  <button
                    key={s.id}
                    type="button"
                    className={`ak-pop-twoline${checked ? " is-selected" : ""}`}
                    onClick={() => onToggleAgent(s.id)}
                  >
                    <span className="ak-agent-avatar">🤖</span>
                    <span className="ak-pop-twoline-body">
                      <span className="ak-pop-row-title">{s.name}</span>
                      {s.description ? (
                        <span className="ak-pop-row-sub ak-truncate">{s.description}</span>
                      ) : null}
                    </span>
                    {checked ? <span className="ak-pop-check">✓</span> : null}
                  </button>
                );
              })}
            </div>
          ) : null}

          <div className="ak-pop-foot">
            <label className="ak-pop-toggle">
              <input
                type="checkbox"
                checked={everyTurn}
                onChange={(e) => onEveryTurnChange(e.target.checked)}
              />
              <span>Every turn</span>
            </label>
            {onCreateGroup ? (
              <button
                type="button"
                className="ak-pop-foot-link"
                onClick={() => {
                  onCreateGroup();
                  setOpen(false);
                }}
              >
                + Create Group
              </button>
            ) : null}
            {onManageAgents ? (
              <button
                type="button"
                className="ak-pop-foot-link"
                onClick={() => {
                  onManageAgents();
                  setOpen(false);
                }}
              >
                Partners ↗
              </button>
            ) : null}
          </div>
        </div>
      ) : null}
    </div>
  );
}

/* 4. `Preferences [Agent] ^` Response Style + Direct Role-Play (`Who answers` — Image 6) */
export function AgentRoleplayPreferencesPopover({
  hostAssistantName = "Assistant",
  styles,
  selectedStyleId,
  onSelectStyle,
  declaredAgents,
  activeResponderId,
  onSelectResponder,
  everyTurn,
  onEveryTurnChange,
  onOpenPersonalization,
  label = "Preferences",
}: {
  hostAssistantName?: string;
  styles: ResponseStyleOptionV2[];
  selectedStyleId: string;
  onSelectStyle: (styleId: string) => void;
  declaredAgents: DeclaredAppAgentV2[];
  /** `null` = Default Host Assistant; otherwise `<agent.id>` answers directly in first-person role-play */
  activeResponderId: string | null;
  onSelectResponder: (agentId: string | null) => void;
  everyTurn: boolean;
  onEveryTurnChange: (next: boolean) => void;
  onOpenPersonalization?: () => void;
  label?: string;
}) {
  const [open, setOpen] = useState(false);
  const wrapRef = useOutsideClose(open, () => setOpen(false));

  const activeAgent = declaredAgents.find((a) => a.id === activeResponderId);
  const activeStyle = styles.find((s) => s.id === selectedStyleId);
  const pillValue = activeAgent?.name || activeStyle?.name || "Default";

  return (
    <div ref={wrapRef} className="ak-pop-anchor">
      <button
        type="button"
        className={`ak-tray-pill${activeAgent || selectedStyleId ? " is-active" : ""}`}
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        <span>{activeAgent?.emoji || "👤"}</span>
        <span>{label}</span>
        <strong className="ak-tray-pill-accent">{pillValue}</strong>
        <span className="ak-tray-chev">{open ? "▴" : "▾"}</span>
      </button>

      {open ? (
        <div
          className="ak-pop-card ak-pop-card--agents"
          role="dialog"
          aria-label="Response style and role-play responder"
        >
          <div className="ak-pop-group">
            <div className="ak-pop-section-lbl">
              <strong>Response style</strong>
            </div>
            <button
              type="button"
              className={`ak-pop-twoline${!selectedStyleId ? " is-selected" : ""}`}
              onClick={() => {
                onSelectStyle("");
                setOpen(false);
              }}
            >
              <span className="ak-agent-avatar">💬</span>
              <span className="ak-pop-twoline-body">
                <span className="ak-pop-row-title">Default</span>
                <span className="ak-pop-row-sub ak-truncate">
                  No persona — standard assistant voice
                </span>
              </span>
              {!selectedStyleId ? <span className="ak-pop-check">✓</span> : null}
            </button>
            {styles.map((st) => {
              const selected = st.id === selectedStyleId;
              return (
                <button
                  key={st.id}
                  type="button"
                  className={`ak-pop-twoline${selected ? " is-selected" : ""}`}
                  onClick={() => {
                    onSelectStyle(st.id);
                    setOpen(false);
                  }}
                >
                  <span className="ak-agent-avatar">👤</span>
                  <span className="ak-pop-twoline-body">
                    <span className="ak-pop-row-title">{st.name}</span>
                    <span className="ak-pop-row-sub ak-truncate">{st.description}</span>
                  </span>
                  {selected ? <span className="ak-pop-check">✓</span> : null}
                </button>
              );
            })}
          </div>

          <div className="ak-pop-group">
            <div className="ak-pop-section-lbl">
              <strong>Who answers</strong>{" "}
              <span>The message goes directly to them; they role-play &amp; answer</span>
            </div>
            <button
              type="button"
              className={`ak-pop-twoline${!activeResponderId ? " is-selected" : ""}`}
              onClick={() => {
                onSelectResponder(null);
                setOpen(false);
              }}
            >
              <span className="ak-agent-avatar">✨</span>
              <span className="ak-pop-twoline-body">
                <span className="ak-pop-row-title">{hostAssistantName}</span>
                <span className="ak-pop-row-sub">Answers as usual</span>
              </span>
              {!activeResponderId ? <span className="ak-pop-check">✓</span> : null}
            </button>
            {declaredAgents.map((agent) => {
              const selected = agent.id === activeResponderId;
              return (
                <button
                  key={agent.id}
                  type="button"
                  className={`ak-pop-twoline${selected ? " is-selected" : ""}`}
                  onClick={() => {
                    onSelectResponder(agent.id);
                    setOpen(false);
                  }}
                >
                  <span
                    className="ak-agent-avatar"
                    style={{ background: agent.color || "var(--ak-surface-soft)" }}
                  >
                    {agent.emoji || agent.name.slice(0, 1)}
                  </span>
                  <span className="ak-pop-twoline-body">
                    <span className="ak-pop-row-title">{agent.name}</span>
                    {agent.description ? (
                      <span className="ak-pop-row-sub ak-truncate">{agent.description}</span>
                    ) : null}
                  </span>
                  {selected ? <span className="ak-pop-check">✓</span> : null}
                </button>
              );
            })}
          </div>

          <div className="ak-pop-foot">
            <label className="ak-pop-toggle">
              <input
                type="checkbox"
                checked={everyTurn}
                onChange={(e) => onEveryTurnChange(e.target.checked)}
              />
              <span>Every turn</span>
            </label>
            {onOpenPersonalization ? (
              <button
                type="button"
                className="ak-pop-foot-link"
                onClick={() => {
                  onOpenPersonalization();
                  setOpen(false);
                }}
              >
                Personalization ↗
              </button>
            ) : null}
          </div>
        </div>
      ) : null}
    </div>
  );
}

/* 5. Foldable Composer Tray with Bottom Lip Grip (`ComposerTrayFold` pattern) */
export function AgentFoldableComposerTray({
  storageKey = "lnc-ak-composer-tray",
  leftCluster,
  rightCluster,
}: {
  storageKey?: string;
  leftCluster: ReactNode;
  rightCluster?: ReactNode;
}) {
  const [folded, setFolded] = useState<boolean>(() => {
    try {
      return typeof window !== "undefined" && localStorage.getItem(storageKey) === "folded";
    } catch {
      return false;
    }
  });

  const toggle = () => {
    setFolded((prev) => {
      const next = !prev;
      try {
        localStorage.setItem(storageKey, next ? "folded" : "open");
      } catch {
        // ignore
      }
      return next;
    });
  };

  return (
    <div className={`ak-composer-tray${folded ? " is-folded" : ""}`}>
      {!folded ? (
        <div className="ak-composer-tray-bar">
          <div className="ak-composer-tray-left">{leftCluster}</div>
          {rightCluster ? <div className="ak-composer-tray-right">{rightCluster}</div> : null}
        </div>
      ) : null}
      <button
        type="button"
        className="ak-composer-tray-grip"
        aria-expanded={!folded}
        aria-label={folded ? "Expand composer toolbar" : "Hide composer toolbar"}
        title={folded ? "Show composer controls" : "Hide composer controls"}
        onClick={toggle}
      >
        <span className="ak-composer-tray-grip-pill" />
      </button>
    </div>
  );
}
