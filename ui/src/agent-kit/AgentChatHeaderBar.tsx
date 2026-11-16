import { useEffect, useRef, useState } from "react";
import type { ChatHeaderControlsConfigV2 } from "./agentTypesV2";

export interface SessionMenuActionV2 {
  key: string;
  label: string;
  danger?: boolean;
  onSelect: () => void;
}

export interface AgentChatHeaderBarProps {
  title: string;
  subtitle?: string;
  lang?: "vi" | "en";
  leftSidebarOpen?: boolean;
  onToggleLeftSidebar?: () => void;
  rightDockOpen: boolean;
  onToggleRightDock: () => void;
  onOpenNoteTab?: () => void;
  controlsConfig?: ChatHeaderControlsConfigV2;
  onChangeControlsConfig?: (next: ChatHeaderControlsConfigV2) => void;
  /** Optional TTS playback speed — ONLY pass when `caps.voice_tts === true` */
  ttsSpeed?: number;
  onChangeTtsSpeed?: (speed: number) => void;
  moreActions?: SessionMenuActionV2[];
}

/**
 * Minimalist Claude / Antigravity / DeepTutor 32x32 Ghost Icon Header (`v2.3` — Image 7):
 * - Replaces noisy text buttons (`Pin | Branch | Notes | Dock`) with 4 calm 32x32 SVG icons:
 *   1. `Sliders` (Customize visible composer controls)
 *   2. `NotebookPen` (Open Markdown Note scratchpad in right dock)
 *   3. `PanelRight` (`[◫]` Toggle right Activity / Files / Notes dock open or hidden)
 *   4. `MoreHorizontal` (`⋯` menu for Pin, Rename, Branch/Fork, Export, Delete)
 * - Optional `[◧]` Left Sidebar collapse button for Zen full-width reading/writing.
 */
export function AgentChatHeaderBar({
  title,
  subtitle,
  lang = "en",
  leftSidebarOpen,
  onToggleLeftSidebar,
  rightDockOpen,
  onToggleRightDock,
  onOpenNoteTab,
  controlsConfig,
  onChangeControlsConfig,
  ttsSpeed,
  onChangeTtsSpeed,
  moreActions = [],
}: AgentChatHeaderBarProps) {
  const [customizeOpen, setCustomizeOpen] = useState(false);
  const [moreOpen, setMoreOpen] = useState(false);
  const rightRef = useRef<HTMLDivElement>(null);
  const isVi = lang === "vi";

  useEffect(() => {
    if (!customizeOpen && !moreOpen) return;
    const onDoc = (e: MouseEvent) => {
      if (rightRef.current && !rightRef.current.contains(e.target as Node)) {
        setCustomizeOpen(false);
        setMoreOpen(false);
      }
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setCustomizeOpen(false);
        setMoreOpen(false);
      }
    };
    const onRoute = () => {
      setCustomizeOpen(false);
      setMoreOpen(false);
    };
    document.addEventListener("mousedown", onDoc);
    window.addEventListener("keydown", onKey);
    window.addEventListener("hashchange", onRoute);
    window.addEventListener("popstate", onRoute);
    return () => {
      document.removeEventListener("mousedown", onDoc);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("hashchange", onRoute);
      window.removeEventListener("popstate", onRoute);
    };
  }, [customizeOpen, moreOpen]);

  const showMoreMenu =
    moreActions.length > 0 || (typeof ttsSpeed === "number" && Boolean(onChangeTtsSpeed));

  return (
    <header className="ak-hdr-bar">
      <div className="ak-hdr-left">
        {onToggleLeftSidebar ? (
          <button
            type="button"
            className={`ak-hdr-icon-btn${leftSidebarOpen ? " is-active" : ""}`}
            aria-label={isVi ? "Ẩn/Hiện danh sách phiên" : "Toggle threads sidebar"}
            title={
              isVi
                ? leftSidebarOpen
                  ? "Ẩn danh sách phiên"
                  : "Hiện danh sách phiên"
                : leftSidebarOpen
                  ? "Hide threads sidebar"
                  : "Show threads sidebar"
            }
            onClick={onToggleLeftSidebar}
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.75">
              <rect x="3" y="4" width="18" height="16" rx="2.5" />
              <line x1="9" y1="4" x2="9" y2="20" />
            </svg>
          </button>
        ) : null}
        <div className="ak-hdr-titles">
          <div className="ak-hdr-title ak-truncate">{title}</div>
          {subtitle ? <div className="ak-hdr-sub ak-truncate">{subtitle}</div> : null}
        </div>
      </div>

      <div ref={rightRef} className="ak-hdr-actions">
        {controlsConfig && onChangeControlsConfig ? (
          <div className="ak-pop-anchor">
            <button
              type="button"
              className={`ak-hdr-icon-btn${customizeOpen ? " is-open" : ""}`}
              aria-label={isVi ? "Tùy biến công cụ soạn thảo" : "Customize chat controls"}
              title={isVi ? "Tùy biến công cụ soạn thảo" : "Customize chat controls"}
              onClick={() => {
                setMoreOpen(false);
                setCustomizeOpen((v) => !v);
              }}
            >
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.75">
                <line x1="4" y1="6" x2="20" y2="6" />
                <circle cx="9" cy="6" r="2" fill="var(--ak-surface)" />
                <line x1="4" y1="12" x2="20" y2="12" />
                <circle cx="15" cy="12" r="2" fill="var(--ak-surface)" />
                <line x1="4" y1="18" x2="20" y2="18" />
                <circle cx="10" cy="18" r="2" fill="var(--ak-surface)" />
              </svg>
            </button>
            {customizeOpen ? (
              <div className="ak-hdr-menu" role="dialog" aria-label={isVi ? "Tùy biến công cụ" : "Customize controls"}>
                <div className="ak-dock-menu-divider">
                  {isVi ? "Công cụ hiển thị ở khung chat" : "Visible composer controls"}
                </div>
                {(
                  [
                    ["showSkills", isVi ? "Bộ kỹ năng (/Skill)" : "Skills picker"],
                    ["showMcp", isVi ? "Trình kết nối MCP" : "MCP connectors"],
                    ["showPartners", isVi ? "Mời đối tác cùng bàn" : "Partners consult"],
                    ["showPreferences", isVi ? "Phong cách & Vai" : "Role-play & style"],
                    ["showContextRing", isVi ? "Vòng % ngữ cảnh" : "Context % ring"],
                  ] as const
                ).map(([k, lbl]) => (
                  <label key={k} className="ak-hdr-check-row">
                    <input
                      type="checkbox"
                      checked={controlsConfig[k]}
                      onChange={(e) =>
                        onChangeControlsConfig({ ...controlsConfig, [k]: e.target.checked })
                      }
                    />
                    <span>{lbl}</span>
                  </label>
                ))}
              </div>
            ) : null}
          </div>
        ) : null}

        {onOpenNoteTab ? (
          <button
            type="button"
            className="ak-hdr-icon-btn"
            aria-label={isVi ? "Ghi chú Markdown" : "Markdown note"}
            title={isVi ? "Mở ghi chú Markdown ở bảng bên" : "Write Markdown note in dock"}
            onClick={onOpenNoteTab}
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.75">
              <path d="M12 20h9" />
              <path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4Z" />
            </svg>
          </button>
        ) : null}

        <button
          type="button"
          className={`ak-hdr-icon-btn${rightDockOpen ? " is-active" : ""}`}
          aria-label={isVi ? "Ẩn/Hiện bảng hoạt động" : "Toggle workspace dock"}
          aria-pressed={rightDockOpen}
          title={isVi ? "Bảng hoạt động, tệp & ghi chú (Ctrl+J)" : "Session activity, files & previews (Ctrl+J)"}
          onClick={onToggleRightDock}
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.75">
            <rect x="3" y="4" width="18" height="16" rx="2.5" />
            <line x1="15" y1="4" x2="15" y2="20" />
          </svg>
        </button>

        {showMoreMenu ? (
          <div className="ak-pop-anchor">
            <button
              type="button"
              className={`ak-hdr-icon-btn${moreOpen ? " is-open" : ""}`}
              aria-label={isVi ? "Tác vụ phiên" : "Session menu"}
              title={isVi ? "Thêm tác vụ (Ghim, Rẽ nhánh, Xuất…)" : "More actions (Pin, Branch, Export…)"}
              onClick={() => {
                setCustomizeOpen(false);
                setMoreOpen((v) => !v);
              }}
            >
              <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
                <circle cx="5" cy="12" r="1.7" />
                <circle cx="12" cy="12" r="1.7" />
                <circle cx="19" cy="12" r="1.7" />
              </svg>
            </button>
            {moreOpen ? (
              <div className="ak-hdr-menu" role="menu">
                {moreActions.map((act) => (
                  <button
                    key={act.key}
                    type="button"
                    role="menuitem"
                    className={`ak-hdr-menu-item${act.danger ? " is-danger" : ""}`}
                    onClick={() => {
                      act.onSelect();
                      setMoreOpen(false);
                    }}
                  >
                    {act.label}
                  </button>
                ))}
                {typeof ttsSpeed === "number" && onChangeTtsSpeed ? (
                  <>
                    <div className="ak-dock-menu-divider">TTS Speed</div>
                    <div className="ak-btn-group" style={{ padding: "4px 6px" }}>
                      {[0.8, 1.0, 1.25, 1.5].map((sp) => (
                        <button
                          key={sp}
                          type="button"
                          className={`ak-chip${ttsSpeed === sp ? " active" : ""}`}
                          onClick={() => onChangeTtsSpeed(sp)}
                        >
                          {sp}×
                        </button>
                      ))}
                    </div>
                  </>
                ) : null}
              </div>
            ) : null}
          </div>
        ) : null}
      </div>
    </header>
  );
}
export default AgentChatHeaderBar;
