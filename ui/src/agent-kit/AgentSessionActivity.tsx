import type { ReactNode } from "react";
import type { SessionActivityV2, SessionArtifactFileV2 } from "./agentTypesV2";

export interface AgentSessionActivityProps {
  activity: SessionActivityV2;
  onOpenFile: (file: SessionArtifactFileV2) => void;
  onNavigateSpace?: (href: string) => void;
  configSection?: ReactNode;
  emptyText?: string;
}

function formatBytes(bytes?: number): string {
  if (!bytes || !Number.isFinite(bytes) || bytes <= 0) return "";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function fileBadge(file: SessionArtifactFileV2): string {
  const ext = (file.ext || file.filename.split(".").pop() || "FILE").toUpperCase();
  const size = formatBytes(file.sizeBytes);
  return size ? `${ext} · ${size}` : ext;
}

function fileGlyph(file: SessionArtifactFileV2): string {
  const ext = (file.ext || file.filename.split(".").pop() || "").toLowerCase();
  if (["md", "json", "yaml", "yml", "toml", "py", "rs", "ts", "tsx"].includes(ext)) return "{}";
  if (["tsv", "csv", "xlsx"].includes(ext)) return "⊞";
  if (["png", "jpg", "jpeg", "webp", "svg"].includes(ext)) return "▣";
  return "📄";
}

function ActivitySection({
  icon,
  title,
  count,
  children,
}: {
  icon: string;
  title: string;
  count?: number;
  children: ReactNode;
}) {
  return (
    <section className="ak-activity-section">
      <header className="ak-activity-section-hdr">
        <span className="ak-activity-icon" aria-hidden="true">{icon}</span>
        <h3 className="ak-activity-title">{title}</h3>
        {typeof count === "number" && count > 0 ? (
          <span className="ak-activity-count">{count}</span>
        ) : null}
      </header>
      {children}
    </section>
  );
}

/**
 * Rich Session Activity index rendered inside `AgentWorkspaceDock` (`v2.2`):
 * - `Tools used` (`×count` right-aligned)
 * - `Knowledge bases`
 * - `@ Space` nested subsections (`Persona`, `Regions & Topics`, `Memory`)
 * - `Generated files` & `Attachments` clickable cards that open as dock tabs
 */
export function AgentSessionActivity({
  activity,
  onOpenFile,
  onNavigateSpace,
  configSection,
  emptyText = "As you chat, the tools, references, and generated files from this session will appear here.",
}: AgentSessionActivityProps) {
  const isEmpty =
    activity.tools.length === 0 &&
    activity.knowledgeBases.length === 0 &&
    activity.space.length === 0 &&
    activity.generatedFiles.length === 0 &&
    activity.attachments.length === 0;

  if (isEmpty && !configSection) {
    return (
      <div className="ak-activity-wrap">
        <div className="ak-activity-group-label">Session activity</div>
        <div className="ak-activity-tile ak-activity-empty">{emptyText}</div>
      </div>
    );
  }

  return (
    <div className="ak-activity-wrap">
      {!isEmpty ? (
        <div>
          <div className="ak-activity-group-label">Session activity</div>
          <div className="ak-activity-tile">
            {activity.tools.length > 0 ? (
              <ActivitySection icon="🔧" title="Tools used" count={activity.tools.length}>
                <ul className="ak-activity-list">
                  {activity.tools.map((tool) => (
                    <li key={tool.name} className="ak-activity-row">
                      <span className="ak-truncate">{tool.name}</span>
                      <span className="ak-activity-count">×{tool.count}</span>
                    </li>
                  ))}
                </ul>
              </ActivitySection>
            ) : null}

            {activity.knowledgeBases.length > 0 ? (
              <ActivitySection
                icon="🗄️"
                title="Knowledge bases"
                count={activity.knowledgeBases.length}
              >
                <ul className="ak-activity-list">
                  {activity.knowledgeBases.map((kb) => (
                    <li key={kb} className="ak-activity-row">
                      <span className="ak-truncate">{kb}</span>
                    </li>
                  ))}
                </ul>
              </ActivitySection>
            ) : null}

            {activity.space.length > 0 ? (
              <ActivitySection icon="@" title="Space">
                <div className="ak-activity-space-list">
                  {activity.space.map((sub) => (
                    <div key={sub.key} className="ak-activity-space-sub">
                      <button
                        type="button"
                        className="ak-activity-space-hdr"
                        onClick={() => sub.href && onNavigateSpace?.(sub.href)}
                      >
                        <span className="ak-truncate">{sub.label}</span>
                        <span className="ak-activity-count">{sub.items.length}</span>
                      </button>
                      <ul className="ak-activity-space-items">
                        {sub.items.map((item) => (
                          <li key={item.id} className="ak-activity-space-item">
                            <span className="ak-truncate">{item.title}</span>
                            {item.subtitle ? (
                              <span className="ak-activity-count">{item.subtitle}</span>
                            ) : null}
                          </li>
                        ))}
                      </ul>
                    </div>
                  ))}
                </div>
              </ActivitySection>
            ) : null}

            {activity.generatedFiles.length > 0 ? (
              <ActivitySection
                icon="✨"
                title="Generated files"
                count={activity.generatedFiles.length}
              >
                <ul className="ak-activity-files">
                  {activity.generatedFiles.map((file, idx) => (
                    <li key={`${file.id || file.filename}-${idx}`}>
                      <button
                        type="button"
                        className="ak-activity-file-btn"
                        onClick={() => onOpenFile(file)}
                      >
                        <span className="ak-activity-file-glyph" aria-hidden="true">
                          {fileGlyph(file)}
                        </span>
                        <span className="ak-activity-file-meta">
                          <span className="ak-activity-file-name ak-truncate">
                            {file.filename}
                          </span>
                          <span className="ak-activity-file-sub">{fileBadge(file)}</span>
                        </span>
                      </button>
                    </li>
                  ))}
                </ul>
              </ActivitySection>
            ) : null}

            {activity.attachments.length > 0 ? (
              <ActivitySection
                icon="📎"
                title="Attachments"
                count={activity.attachments.length}
              >
                <ul className="ak-activity-files">
                  {activity.attachments.map((file, idx) => (
                    <li key={`${file.id || file.filename}-${idx}`}>
                      <button
                        type="button"
                        className="ak-activity-file-btn"
                        onClick={() => onOpenFile(file)}
                      >
                        <span className="ak-activity-file-glyph" aria-hidden="true">
                          {fileGlyph(file)}
                        </span>
                        <span className="ak-activity-file-meta">
                          <span className="ak-activity-file-name ak-truncate">
                            {file.filename}
                          </span>
                          <span className="ak-activity-file-sub">{fileBadge(file)}</span>
                        </span>
                      </button>
                    </li>
                  ))}
                </ul>
              </ActivitySection>
            ) : null}
          </div>
        </div>
      ) : null}

      {configSection}
    </div>
  );
}

export default AgentSessionActivity;
