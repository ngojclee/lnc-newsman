import { useEffect, useMemo, useRef, useState } from 'react'
import { ToolInvocationPill } from './AgentsWorkspace.tsx'
import { AGENT_COPY, type AppAgentConfig } from './agentTypes.ts'
import {
  appAgentsApi,
  boundActiveViewContext,
  extractMentions,
  MAX_ACTIVE_VIEW_CONTEXT_BYTES,
  skillDisplayName,
  useAppAgents,
  useCompanionContext,
  useThreadStream,
} from './useAppAgents.ts'

export interface CompanionDrawerProps {
  open: boolean
  onClose: () => void
  lang: 'vi' | 'en'
  kind?: 'article' | 'report'
  articleId?: number | null
  reportId?: number | null
  title: string
  sourceAttribution?: string | null
  region?: string | null
  topic?: string | null
  selectedText?: string | null
  contentText?: string | null
  excerpt?: string | null
  onOpenArticle?: (articleId: number) => void
}

export function CompanionDrawer({
  open,
  onClose,
  lang,
  kind = 'article',
  articleId = null,
  reportId = null,
  title,
  sourceAttribution,
  selectedText,
  contentText,
  excerpt,
  onOpenArticle,
}: CompanionDrawerProps) {
  const c = AGENT_COPY[lang]
  const { agents, threads, workspaces, activeWorkspaceId, skills, loadThreads, createThread } =
    useAppAgents()
  const [selectedAgentSlug, setSelectedAgentSlug] = useState<string>('macro-argus')
  const [attachedSkills, setAttachedSkills] = useState<string[]>([])
  const [threadId, setThreadId] = useState<number | string | null>(null)
  const [draft, setDraft] = useState('')
  const [creatingThread, setCreatingThread] = useState(false)
  const listRef = useRef<HTMLDivElement | null>(null)

  const activeWorkspace = useMemo(
    () => workspaces.find((w) => w.id === activeWorkspaceId) ?? workspaces[0] ?? null,
    [activeWorkspaceId, workspaces],
  )
  const grantedSkills = useMemo(
    () => skills.filter((s) => s.granted && s.enabled),
    [skills],
  )

  const activeViewContext = useCompanionContext({
    kind,
    articleId,
    reportId,
    title,
    sourceAttribution: sourceAttribution ?? null,
    selectedText: selectedText ?? null,
    contentText: contentText ?? null,
    excerpt: excerpt ?? null,
  })

  useEffect(() => {
    if (!open) return
    void loadThreads(articleId ?? null)
  }, [articleId, loadThreads, open])

  const matchingThreads = useMemo(
    () =>
      threads.filter((t) =>
        articleId ? t.article_id === articleId : reportId ? t.report_id === reportId : false,
      ),
    [articleId, reportId, threads],
  )

  useEffect(() => {
    if (!open) return
    if (
      matchingThreads.length > 0 &&
      (threadId === null || !matchingThreads.some((t) => String(t.id) === String(threadId)))
    ) {
      setThreadId(matchingThreads[0].id)
    }
  }, [matchingThreads, open, threadId])

  const { messages, streamState, sending, error, sendTurn } = useThreadStream(open ? threadId : null)

  useEffect(() => {
    const el = listRef.current
    if (el) el.scrollTop = el.scrollHeight
  }, [messages.length, streamState.drafts])

  const agentMap = useMemo(() => {
    const m = new Map<string, AppAgentConfig>()
    for (const a of agents) m.set(a.slug, a)
    return m
  }, [agents])

  if (!open) return null

  const ensureThreadAndSend = async (promptText: string) => {
    const trimmed = promptText.trim()
    if (!trimmed || sending || creatingThread) return

    let targetThreadId = threadId
    if (targetThreadId === null) {
      setCreatingThread(true)
      try {
        const created = await createThread({
          kind: 'direct',
          title: `${title.slice(0, 64)}`,
          article_id: articleId ?? null,
          report_id: reportId ?? null,
          member_slugs: [selectedAgentSlug],
          discussion_mode: 'mention_or_lead',
          lead_slug: selectedAgentSlug,
          workspace_id: activeWorkspace?.id ?? null,
          pinned_skills: attachedSkills,
        })
        targetThreadId = created.id
        setThreadId(created.id)
      } finally {
        setCreatingThread(false)
      }
    }

    setDraft('')
    if (targetThreadId !== null && String(targetThreadId) === String(threadId)) {
      await sendTurn(trimmed, {
        mentions: extractMentions(trimmed, agents.map((a) => a.slug)),
        activeViewContext,
        skillSlugs: attachedSkills,
      })
    } else if (targetThreadId !== null) {
      const bounded = activeViewContext
        ? boundActiveViewContext(activeViewContext, MAX_ACTIVE_VIEW_CONTEXT_BYTES)
        : undefined
      await appAgentsApi(`/api/app-agents/threads/${encodeURIComponent(String(targetThreadId))}/turns`, {
        method: 'POST',
        body: JSON.stringify({
          text: trimmed,
          idempotency_key: `comp_${targetThreadId}_${Date.now()}`,
          mentions: [selectedAgentSlug],
          ...(attachedSkills.length > 0 ? { skill_slugs: attachedSkills } : {}),
          ...(bounded ? { active_view_context: bounded } : {}),
        }),
      })
      setThreadId(targetThreadId)
    }
  }

  const previewBody = (contentText || excerpt || '').trim()
  const hasRichText = Boolean((selectedText && selectedText.trim()) || previewBody)
  const contextBytes =
    hasRichText && activeViewContext
      ? new TextEncoder().encode(JSON.stringify(activeViewContext)).byteLength
      : 0
  const contextUsageLabel = hasRichText
    ? `✓ ${c.companionContextReadyLoaded} (${contextBytes < 1024 ? '<1' : (contextBytes / 1024).toFixed(1)} KB)`
    : c.companionContextReadyTitle

  const renderWithCitations = (text: string) => {
    const parts = text.split(/(#article-\d+)/gi)
    return parts.map((part, idx) => {
      const m = /^#article-(\d+)$/i.exec(part)
      if (m && onOpenArticle) {
        const id = Number(m[1])
        return (
          <button
            key={idx}
            type="button"
            className="agent-citation-link"
            onClick={() => onOpenArticle(id)}
          >
            📰 {c.companionJumpArticle} #{id}
          </button>
        )
      }
      return <span key={idx}>{part}</span>
    })
  }

  return (
    <aside className="companion-drawer" role="dialog" aria-label={c.companionHeaderTitle}>
      <header className="companion-head">
        <div>
          <span className="eyebrow">{c.companionContextBadge}</span>
          <strong>{c.companionHeaderTitle}</strong>
        </div>
        <button
          type="button"
          className="icon-button"
          onClick={onClose}
          aria-label={c.cancelBtn}
          title={c.cancelBtn}
        >
          ✕
        </button>
      </header>

      <div className="companion-context-card">
        <div className="companion-ctx-row">
          <strong>📰 {title}</strong>
          <span className="scope-chip">{contextUsageLabel}</span>
        </div>
        {activeWorkspace && (
          <div className="companion-workspace-row">
            <span className="workspace-mini-badge">
              {activeWorkspace.emoji} {c.companionWorkspaceBadge} {activeWorkspace.name}
            </span>
          </div>
        )}
        {sourceAttribution && (
          <div className="companion-ctx-chapter">
            <span>{sourceAttribution}</span>
          </div>
        )}
        {selectedText && (
          <blockquote className="companion-ctx-quote">
            <small>{c.companionSelectedTextLabel}</small>
            “{selectedText.slice(0, 180)}
            {selectedText.length > 180 ? '…' : ''}”
          </blockquote>
        )}
        {!selectedText && previewBody && (
          <p className="companion-ctx-excerpt">
            <small>{c.companionArticleExcerptLabel}</small>{' '}
            {previewBody.slice(0, 140)}
            {previewBody.length > 140 ? '…' : ''}
          </p>
        )}
      </div>

      <div className="companion-controls">
        <div className="chip-row">
          {agents.map((ag) => (
            <button
              key={ag.slug}
              type="button"
              className={selectedAgentSlug === ag.slug ? 'chip active' : 'chip'}
              onClick={() => setSelectedAgentSlug(ag.slug)}
            >
              {ag.emoji} {ag.display_name}
            </button>
          ))}
        </div>

        {grantedSkills.length > 0 && (
          <div className="companion-skill-chips">
            <small className="companion-skill-label">{c.companionSkillChipLabel}:</small>
            <div className="chip-row">
              {grantedSkills.map((sk) => {
                const active = attachedSkills.includes(sk.slug)
                return (
                  <button
                    key={sk.slug}
                    type="button"
                    className={active ? 'chip active' : 'chip'}
                    onClick={() =>
                      setAttachedSkills((prev) =>
                        prev.includes(sk.slug) ? prev.filter((s) => s !== sk.slug) : [...prev, sk.slug],
                      )
                    }
                  >
                    🧩 {skillDisplayName(sk)}
                  </button>
                )
              })}
            </div>
          </div>
        )}

        <div className="companion-quick-prompts">
          <button
            type="button"
            className="chip"
            onClick={() => void ensureThreadAndSend(c.companionQuickExplain)}
          >
            ✦ {c.companionQuickExplain}
          </button>
          <button
            type="button"
            className="chip"
            onClick={() => void ensureThreadAndSend(c.companionQuickSummarize)}
          >
            ✦ {c.companionQuickSummarize}
          </button>
          <button
            type="button"
            className="chip"
            onClick={() => void ensureThreadAndSend(c.companionQuickVerifyTax)}
          >
            ✦ {c.companionQuickVerifyTax}
          </button>
        </div>
      </div>

      <div className="companion-messages" ref={listRef}>
        {messages.map((m) => {
          const isUser = m.role === 'user'
          const ag = m.sender_slug ? agentMap.get(m.sender_slug) : undefined
          return (
            <div key={m.id} className={isUser ? 'salon-turn salon-turn-user' : 'salon-turn salon-turn-agent'}>
              <div className="salon-turn-card">
                <header className="salon-turn-meta">
                  <b>
                    {isUser
                      ? lang === 'vi'
                        ? 'Bạn đọc'
                        : 'You'
                      : `${ag?.emoji ?? '📈'} ${ag?.display_name ?? m.sender_slug ?? 'Analyst'}`}
                  </b>
                </header>
                {m.reasoning && (
                  <details className="salon-reasoning">
                    <summary>{c.reasoningTraceSummary}</summary>
                    <pre>{m.reasoning}</pre>
                  </details>
                )}
                {m.tool_calls && m.tool_calls.length > 0 && (
                  <div className="salon-tool-list">
                    {m.tool_calls.map((tc) => (
                      <ToolInvocationPill key={tc.merge_id} pill={tc} lang={lang} />
                    ))}
                  </div>
                )}
                <div className="salon-turn-prose">{renderWithCitations(m.content)}</div>
              </div>
            </div>
          )
        })}

        {streamState.toolPills.length > 0 && (
          <div className="salon-live-tools">
            {streamState.toolPills.map((pill) => (
              <ToolInvocationPill key={pill.merge_id} pill={pill} lang={lang} />
            ))}
          </div>
        )}

        {Object.values(streamState.drafts).map((d) => {
          const ag = agentMap.get(d.agent_slug)
          return (
            <div key={d.agent_slug} className="salon-turn salon-turn-agent streaming">
              <div className="salon-turn-card">
                <header className="salon-turn-meta">
                  <b>
                    {ag?.emoji ?? '📈'} {ag?.display_name ?? `@${d.agent_slug}`}
                  </b>
                  <em>{c.seatSpeaking}</em>
                </header>
                <div className="salon-turn-prose">{d.text}</div>
              </div>
            </div>
          )
        })}
      </div>

      {(error || streamState.lastError) && <p className="form-error">{error ?? streamState.lastError}</p>}

      <footer className="companion-footer">
        <div className="chat-input-row">
          <textarea
            rows={2}
            value={draft}
            placeholder={c.composerPlaceholder}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && !e.shiftKey) {
                e.preventDefault()
                void ensureThreadAndSend(draft)
              }
            }}
          />
          <button
            type="button"
            className="btn primary"
            disabled={sending || creatingThread || !draft.trim()}
            onClick={() => void ensureThreadAndSend(draft)}
          >
            {sending || creatingThread ? c.sendingTurnBtn : c.sendTurnBtn}
          </button>
        </div>
      </footer>
    </aside>
  )
}
