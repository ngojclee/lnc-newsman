import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { foldVi, type SkillOptionV2 } from "./agentTypesV2";

export type SkillSourceFilter = "all" | "app" | "hub" | "pinned";

export interface AgentSkillPickerProps {
  skills: SkillOptionV2[];
  selectedSlugs: string[];
  onToggleSkill: (slug: string) => void;
  applyEveryTurn?: boolean;
  onToggleApplyEveryTurn?: (next: boolean) => void;
  pinnedSlugs?: string[];
  onTogglePin?: (slug: string) => void;
  onCreateSkill?: () => void;
  onDeleteCustomSkill?: (slug: string) => void;
  label?: string;
  searchPlaceholder?: string;
  size?: "md" | "sm";
  disabled?: boolean;
  ariaLabel?: string;
  className?: string;
}

interface PortalRect {
  left: number;
  top?: number;
  bottom?: number;
  minWidth: number;
  maxHeight: number;
}

/**
 * Portaled multi-select Skill Picker (`lnc-app-agents v2.2`):
 * - Always-on `foldVi()` search bar (even when `< 7` skills exist)
 * - Source filter chips (`All | App | Hub | Pinned`)
 * - Inline `SKILL.md` preview pane
 * - `Apply every turn` toggle and optional `+ Create Skill` hook
 */
export function AgentSkillPicker({
  skills,
  selectedSlugs,
  onToggleSkill,
  applyEveryTurn = true,
  onToggleApplyEveryTurn,
  pinnedSlugs,
  onTogglePin,
  onCreateSkill,
  onDeleteCustomSkill,
  label = "Skills",
  searchPlaceholder = "Search skills...",
  size = "sm",
  disabled = false,
  ariaLabel = "Select skills",
  className = "",
}: AgentSkillPickerProps) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<SkillSourceFilter>("all");
  const [previewSlug, setPreviewSlug] = useState<string | null>(null);
  const [rect, setRect] = useState<PortalRect | null>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);

  const selectedSet = useMemo(() => new Set(selectedSlugs), [selectedSlugs]);
  const pinnedSet = useMemo(() => {
    const out = new Set(pinnedSlugs ?? []);
    for (const s of skills) {
      if (s.pinned) out.add(s.slug);
    }
    return out;
  }, [pinnedSlugs, skills]);

  const filtered = useMemo(() => {
    const q = foldVi(query);
    return skills.filter((sk) => {
      if (filter === "app" && sk.source !== "app" && sk.source !== "custom") return false;
      if (filter === "hub" && sk.source !== "hub") return false;
      if (filter === "pinned" && !pinnedSet.has(sk.slug)) return false;
      if (!q) return true;
      const hay = foldVi(
        `${sk.name} ${sk.slug} ${sk.description} ${(sk.tags ?? []).join(" ")} ${sk.slash_command ?? ""}`,
      );
      return hay.includes(q);
    });
  }, [skills, filter, pinnedSet, query]);

  const previewSkill = useMemo(
    () => skills.find((s) => s.slug === previewSlug) ?? null,
    [skills, previewSlug],
  );

  const computeRect = () => {
    const el = triggerRef.current;
    if (!el || !el.isConnected || !el.offsetParent) {
      setOpen(false);
      return;
    }
    const r = el.getBoundingClientRect();
    const SAFE_MARGIN = 32;
    const GAP = 6;
    const estHeight = previewSkill ? 420 : 340;
    const spaceBelow = window.innerHeight - r.bottom - SAFE_MARGIN;
    const spaceAbove = r.top - 16;
    const openUpward =
      (spaceBelow < estHeight && spaceAbove > spaceBelow) ||
      r.bottom > window.innerHeight - 210;
    const panelWidth = Math.max(360, previewSkill ? 540 : 380);
    const clampedLeft = Math.max(
      12,
      Math.min(Math.round(r.left), window.innerWidth - panelWidth - 12),
    );
    if (openUpward) {
      setRect({
        left: clampedLeft,
        bottom: Math.max(SAFE_MARGIN, Math.round(window.innerHeight - r.top + GAP)),
        minWidth: panelWidth,
        maxHeight: Math.max(200, Math.min(440, Math.floor(spaceAbove - GAP))),
      });
    } else {
      setRect({
        left: clampedLeft,
        top: Math.round(r.bottom + GAP),
        minWidth: panelWidth,
        maxHeight: Math.max(200, Math.min(440, Math.floor(spaceBelow - GAP))),
      });
    }
  };

  useLayoutEffect(() => {
    if (open) computeRect();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, filtered.length, Boolean(previewSkill)]);

  useEffect(() => {
    if (!open) {
      setQuery("");
      setPreviewSlug(null);
      return;
    }
    const raf = requestAnimationFrame(() => searchInputRef.current?.focus());
    const sync = () => computeRect();
    const onRouteOrHash = () => setOpen(false);
    window.addEventListener("scroll", sync, true);
    window.addEventListener("resize", sync);
    window.addEventListener("hashchange", onRouteOrHash);
    window.addEventListener("popstate", onRouteOrHash);
    const onOutside = (e: MouseEvent) => {
      const t = e.target as Node;
      if (triggerRef.current?.contains(t) || panelRef.current?.contains(t)) return;
      setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setOpen(false);
        triggerRef.current?.focus();
      }
    };
    document.addEventListener("mousedown", onOutside);
    document.addEventListener("keydown", onKey);
    return () => {
      cancelAnimationFrame(raf);
      window.removeEventListener("scroll", sync, true);
      window.removeEventListener("resize", sync);
      window.removeEventListener("hashchange", onRouteOrHash);
      window.removeEventListener("popstate", onRouteOrHash);
      document.removeEventListener("mousedown", onOutside);
      document.removeEventListener("keydown", onKey);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  const portalHost = typeof document !== "undefined" ? document.body : null;

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-label={ariaLabel}
        disabled={disabled}
        onClick={() => setOpen((v) => !v)}
        className={`ak-btn ${size === "sm" ? "ak-btn-sm" : ""} ${open ? "open" : ""} ${className}`.trim()}
      >
        <span>{label}</span>
        {selectedSlugs.length > 0 ? (
          <span className="ak-select-badge">{selectedSlugs.length}</span>
        ) : null}
      </button>

      {open && rect && portalHost
        ? createPortal(
            <div
              ref={panelRef}
              role="dialog"
              aria-label={ariaLabel}
              className="engine-select-panel engine-select-panel--portal ak-select-panel ak-skill-picker"
              style={{
                left: rect.left,
                ...(rect.bottom !== undefined ? { bottom: rect.bottom } : { top: rect.top }),
                width: rect.minWidth,
                maxWidth: "calc(100vw - 24px)",
                maxHeight: rect.maxHeight,
              }}
            >
              <div className="ak-skill-picker-head">
                <input
                  ref={searchInputRef}
                  data-tray-search=""
                  type="search"
                  className="ak-select-search"
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  placeholder={searchPlaceholder}
                  aria-label={searchPlaceholder}
                />
                <div className="ak-skill-chips" role="tablist" aria-label="Skill filters">
                  {(["all", "app", "hub", "pinned"] as SkillSourceFilter[]).map((f) => (
                    <button
                      key={f}
                      type="button"
                      role="tab"
                      aria-selected={filter === f}
                      onClick={() => setFilter(f)}
                      className={`ak-chip${filter === f ? " active" : ""}`}
                    >
                      {f === "all" ? "All" : f === "app" ? "App" : f === "hub" ? "Hub" : "Pinned"}
                    </button>
                  ))}
                </div>
              </div>

              <div className={`ak-skill-picker-body${previewSkill ? " has-preview" : ""}`}>
                <div className="ak-skill-list">
                  {filtered.length === 0 ? (
                    <div className="ak-skill-empty">No matching skills</div>
                  ) : (
                    filtered.map((sk) => {
                      const active = selectedSet.has(sk.slug);
                      const pinned = pinnedSet.has(sk.slug);
                      const previewing = previewSlug === sk.slug;
                      return (
                        <div
                          key={sk.slug}
                          className={`ak-skill-row${active ? " selected" : ""}${previewing ? " previewing" : ""}`}
                        >
                          <button
                            type="button"
                            className="ak-skill-row-main"
                            onClick={() => onToggleSkill(sk.slug)}
                          >
                            <span className="ak-skill-check" aria-hidden="true">
                              {active ? "✓" : ""}
                            </span>
                            <span className="ak-skill-meta">
                              <span className="ak-skill-title-row">
                                <span className="ak-truncate">{sk.name}</span>
                                <span className="ak-select-badge">{sk.source}</span>
                              </span>
                              <span className="ak-skill-desc">{sk.description}</span>
                            </span>
                          </button>
                          <div className="ak-skill-row-actions">
                            {onTogglePin ? (
                              <button
                                type="button"
                                className={`ak-btn ak-btn-sm ak-skill-icon-btn${pinned ? " active" : ""}`}
                                title={pinned ? "Unpin skill" : "Pin skill"}
                                onClick={() => onTogglePin(sk.slug)}
                              >
                                {pinned ? "★" : "☆"}
                              </button>
                            ) : null}
                            <button
                              type="button"
                              className={`ak-btn ak-btn-sm ak-skill-icon-btn${previewing ? " active" : ""}`}
                              title="Preview SKILL.md"
                              onClick={() =>
                                setPreviewSlug((cur) => (cur === sk.slug ? null : sk.slug))
                              }
                            >
                              👁
                            </button>
                            {sk.source === "custom" && onDeleteCustomSkill ? (
                              <button
                                type="button"
                                className="ak-btn ak-btn-sm ak-skill-icon-btn"
                                title="Delete custom skill"
                                onClick={() => onDeleteCustomSkill(sk.slug)}
                              >
                                ×
                              </button>
                            ) : null}
                          </div>
                        </div>
                      );
                    })
                  )}
                </div>

                {previewSkill ? (
                  <div className="ak-skill-preview" aria-label="SKILL.md preview">
                    <div className="ak-skill-preview-head">
                      <strong className="ak-truncate">{previewSkill.name}</strong>
                      <button
                        type="button"
                        className="ak-btn ak-btn-sm"
                        onClick={() => setPreviewSlug(null)}
                      >
                        Close
                      </button>
                    </div>
                    <pre className="ak-skill-preview-md">
                      {previewSkill.skill_md?.trim() ||
                        `# ${previewSkill.name}\n\n${previewSkill.description}`}
                    </pre>
                  </div>
                ) : null}
              </div>

              <div className="ak-skill-picker-foot">
                {onToggleApplyEveryTurn ? (
                  <label className="ak-skill-toggle">
                    <input
                      type="checkbox"
                      checked={applyEveryTurn}
                      onChange={(e) => onToggleApplyEveryTurn(e.target.checked)}
                    />
                    <span>Apply every turn</span>
                  </label>
                ) : (
                  <span />
                )}
                {onCreateSkill ? (
                  <button
                    type="button"
                    className="ak-btn ak-btn-sm"
                    onClick={() => {
                      setOpen(false);
                      onCreateSkill();
                    }}
                  >
                    + Create Skill
                  </button>
                ) : null}
              </div>
            </div>,
            portalHost,
          )
        : null}
    </>
  );
}

export interface AgentComposerLeadProps {
  modeLabel?: string | null;
  onClearMode?: () => void;
  selectedSkills: SkillOptionV2[];
  onRemoveSkill?: (slug: string) => void;
  applyEveryTurn?: boolean;
  className?: string;
}

/**
 * Removable mode and active skill pills rendered directly above or inside
 * the composer input area.
 */
export function AgentComposerLead({
  modeLabel,
  onClearMode,
  selectedSkills,
  onRemoveSkill,
  applyEveryTurn = true,
  className = "",
}: AgentComposerLeadProps) {
  if (!modeLabel && selectedSkills.length === 0) return null;
  return (
    <div className={`ak-composer-lead ${className}`.trim()}>
      {modeLabel ? (
        <span className="ak-lead-pill ak-lead-pill--mode">
          <span className="ak-truncate">{modeLabel}</span>
          {onClearMode ? (
            <button
              type="button"
              className="ak-lead-remove"
              aria-label={`Clear ${modeLabel}`}
              onClick={onClearMode}
            >
              ×
            </button>
          ) : null}
        </span>
      ) : null}
      {selectedSkills.map((sk) => (
        <span key={sk.slug} className="ak-lead-pill">
          <span className="ak-truncate">
            /{sk.slug}
            {applyEveryTurn ? "" : " (1x)"}
          </span>
          {onRemoveSkill ? (
            <button
              type="button"
              className="ak-lead-remove"
              aria-label={`Remove ${sk.name}`}
              onClick={() => onRemoveSkill(sk.slug)}
            >
              ×
            </button>
          ) : null}
        </span>
      ))}
    </div>
  );
}

export default AgentSkillPicker;
