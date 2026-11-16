import { useEffect, useMemo, useRef, useState } from 'react'
import { AgentExecutionTrace } from './agent-kit/AgentExecutionTrace.tsx'
import { AgentMarkdown } from './agent-kit/AgentMarkdown.tsx'
import { AgentSelect, type ProviderStatusDot } from './agent-kit/AgentSelect.tsx'
import { AgentSelectionToolbar } from './agent-kit/AgentSelectionToolbar.tsx'
import { AgentComposerLead, AgentSkillPicker } from './agent-kit/AgentSkillPicker.tsx'
import { ToolInvocationPill } from './AgentsWorkspace.tsx'
import {
  AGENT_COPY,
  type AgentMessage,
  type AppAgentConfig,
  type SelectionAnchorV2,
  type SideChatResponderV2,
  type SkillOptionV2,
} from './agentTypes.ts'
import {
  appAgentsApi,
  boundActiveViewContext,
  deriveAgentStreamPhaseV2,
  extractMentions,
  mapToolToTraceStepV2,
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
  const {
    bridgeStatus,
    manifest,
    agents,
    threads,
    workspaces,
    activeWorkspaceId,
    skills,
    customSkills,
    toggleSkillPin,
    deleteCustomSkill,
    loadThreads,
    createThread,
  } = useAppAgents()
  const [selectedAgentSlug, setSelectedAgentSlug] = useState<string>('macro-argus')
  const [attachedSkills, setAttachedSkills] = useState<string[]>([])
  const [applyEveryTurn, setApplyEveryTurn] = useState(true)
  const [threadId, setThreadId] = useState<number | string | null>(null)
  const [draft, setDraft] = useState('')
  const [creatingThread, setCreatingThread] = useState(false)
  const [selectionBar, setSelectionBar] = useState<{
    anchor: SelectionAnchorV2
    coords: { x: number; y: number }
  } | null>(null)
  const listRef = useRef<HTMLDivElement | null>(null)
  const draftRef = useRef<HTMLTextAreaElement | null>(null)

  const activeWorkspace = useMemo(
    () => workspaces.find((w) => w.id === activeWorkspaceId) ?? workspaces[0] ?? null,
    [activeWorkspaceId, workspaces],
  )
  const grantedSkills = useMemo(
    () => skills.filter((s) => s.granted && s.enabled),
    [skills],
  )
  const providerDotStatus: ProviderStatusDot =
    bridgeStatus.mode === 'disconnected' ? 'error' : 'connected'
  const caps = manifest?.capabilities ?? manifest?.features

  const allSkillOptionsV2 = useMemo<SkillOptionV2[]>(() => {
    const map = new Map<string, SkillOptionV2>()
    for (const sk of grantedSkills) {
      map.set(sk.slug, {
        slug: sk.slug,
        name: skillDisplayName(sk),
        description: sk.description,
        source:
          sk.source === 'custom'
            ? 'custom'
            : sk.source === 'app' || sk.source === 'builtin'
              ? 'app'
              : 'hub',
        slash_command: `/${sk.slug}`,
        skill_md: sk.body_md || sk.body_preview || sk.description,
        tags: sk.group_slugs,
        pinned: Boolean(sk.pinned),
      })
    }
    for (const cs of customSkills) {
      map.set(cs.slug, {
        slug: cs.slug,
        name: cs.name,
        description: cs.description,
        source: 'custom',
        slash_command: cs.slash_command || `/${cs.slug}`,
        skill_md: cs.skill_md,
        tags: cs.tags,
        pinned: Boolean(cs.pinned),
      })
    }
    return Array.from(map.values())
  }, [customSkills, grantedSkills])

  const selectedSkillOptionsV2 = useMemo<SkillOptionV2[]>(
    () =>
      attachedSkills.map(
        (slug) =>
          allSkillOptionsV2.find((s) => s.slug === slug) ?? {
            slug,
            name: slug,
            description: '',
            source: 'app' as const,
            slash_command: `/${slug}`,
          },
      ),
    [allSkillOptionsV2, attachedSkills],
  )

  const pinnedSlugs = useMemo(
    () => allSkillOptionsV2.filter((s) => s.pinned).map((s) => s.slug),
    [allSkillOptionsV2],
  )

  const responderOptions = useMemo<SideChatResponderV2[]>(
    () =>
      agents.map((ag) => ({
        slug: ag.slug,
        displayName: ag.display_name,
        emoji: ag.emoji,
        accent: ag.color,
        roleHint: ag.description,
      })),
    [agents],
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

  const { messages, streamState, reconnectNotice, sending, error, sendTurn } = useThreadStream(
    open ? threadId : null,
  )

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

    const skillsSnapshot = [...attachedSkills]
    if (!applyEveryTurn && skillsSnapshot.length > 0) {
      setAttachedSkills([])
    }
    setDraft('')
    if (targetThreadId !== null && String(targetThreadId) === String(threadId)) {
      await sendTurn(trimmed, {
        mentions: extractMentions(trimmed, agents.map((a) => a.slug)),
        activeViewContext,
        skillSlugs: skillsSnapshot,
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
          ...(skillsSnapshot.length > 0 ? { skill_slugs: skillsSnapshot } : {}),
          ...(bounded ? { active_view_context: bounded } : {}),
        }),
      })
      setThreadId(targetThreadId)
    }
  }

  const handleAssistantMouseUp = (msg: AgentMessage) => {
    if (msg.role === 'user') return
    const sel = window.getSelection()
    if (!sel || sel.isCollapsed) return
    const quote = sel.toString().trim()
    if (quote.length < 3) return
    const range = sel.rangeCount > 0 ? sel.getRangeAt(0) : null
    if (!range) return
    const rect = range.getBoundingClientRect()
    if (rect.width === 0 && rect.height === 0) return
    setSelectionBar({
      anchor: {
        id: `comp-msg-${msg.id}`,
        quote: quote.slice(0, 600),
        sourceTitle: msg.sender_slug ? `@${msg.sender_slug}` : 'analyst',
      },
      coords: {
        x: rect.left + rect.width / 2,
        y: rect.top,
      },
    })
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

  return (
    <aside className="companion-drawer ak-scope" role="dialog" aria-label={c.companionHeaderTitle}>
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
        <div className="chip-row" style={{ alignItems: 'center' }}>
          <AgentSelect
            size="sm"
            ariaLabel={c.studioRouteAgentLabel}
            value={selectedAgentSlug}
            onChange={setSelectedAgentSlug}
            options={agents.map((ag) => ({
              value: ag.slug,
              label: `${ag.emoji} ${ag.display_name}`,
              badge: `@${ag.slug}`,
              dots: [providerDotStatus],
            }))}
          />
          <AgentSkillPicker
            size="sm"
            label={c.v2SkillsBtn}
            searchPlaceholder={c.skillsSearchPlaceholder}
            skills={allSkillOptionsV2}
            selectedSlugs={attachedSkills}
            onToggleSkill={(slug) =>
              setAttachedSkills((prev) =>
                prev.includes(slug) ? prev.filter((s) => s !== slug) : [...prev, slug],
              )
            }
            applyEveryTurn={applyEveryTurn}
            onToggleApplyEveryTurn={setApplyEveryTurn}
            pinnedSlugs={pinnedSlugs}
            onTogglePin={(slug) => void toggleSkillPin(slug, !pinnedSlugs.includes(slug))}
            onDeleteCustomSkill={(slug) => void deleteCustomSkill(slug)}
          />
        </div>

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
          const toolCalls = m.tool_calls ?? []
          const traceSteps = toolCalls.map((tc) => mapToolToTraceStepV2(tc))
          const approvalPills = toolCalls.filter((tc) => Boolean(tc.approval_required))
          const hasTrace = Boolean(m.reasoning) || traceSteps.length > 0
          return (
            <div
              key={m.id}
              className={
                isUser ? 'salon-turn salon-turn-user ak-msg--user' : 'salon-turn salon-turn-agent'
              }
            >
              <div className="salon-turn-card">
                <header className="salon-turn-meta">
                  <b>
                    {isUser
                      ? lang === 'vi'
                        ? 'Bạn đọc'
                        : 'You'
                      : `${ag?.emoji ?? '📈'} ${ag?.display_name ?? m.sender_slug ?? 'Analyst'}`}
                  </b>
                  {!isUser && (
                    <button
                      type="button"
                      className="btn secondary btn-sm"
                      onClick={() => void navigator.clipboard?.writeText(m.content)}
                    >
                      {c.v2CopyBtn}
                    </button>
                  )}
                </header>
                {!isUser && hasTrace && (
                  <AgentExecutionTrace
                    phase={deriveAgentStreamPhaseV2({
                      isLive: false,
                      hasReasoning: Boolean(m.reasoning),
                      hasText: Boolean(m.content),
                      doneToolCount: traceSteps.length,
                    })}
                    thinkingText={m.reasoning ?? undefined}
                    steps={traceSteps}
                    responderLabel={ag?.display_name ?? m.sender_slug ?? undefined}
                    responderAvatar={ag?.emoji}
                    onCitationClick={onOpenArticle}
                  />
                )}
                {approvalPills.length > 0 && (
                  <div className="salon-tool-list">
                    {approvalPills.map((tc) => (
                      <ToolInvocationPill key={tc.merge_id} pill={tc} lang={lang} />
                    ))}
                  </div>
                )}
                <div
                  className="salon-turn-prose"
                  onMouseUp={!isUser ? () => handleAssistantMouseUp(m) : undefined}
                >
                  <AgentMarkdown content={m.content} onCitationClick={onOpenArticle} />
                </div>
              </div>
            </div>
          )
        })}

        {(sending ||
          streamState.toolPills.length > 0 ||
          Object.keys(streamState.drafts).length > 0 ||
          Boolean(reconnectNotice)) &&
          (() => {
            const draftsList = Object.values(streamState.drafts)
            if (draftsList.length === 0) {
              const liveSteps = streamState.toolPills.map((p) => mapToolToTraceStepV2(p))
              const runningCount = liveSteps.filter((s) => s.state === 'running').length
              const doneCount = liveSteps.length - runningCount
              const selectedAg = agentMap.get(selectedAgentSlug)
              return (
                <AgentExecutionTrace
                  phase={deriveAgentStreamPhaseV2({
                    isLive: sending,
                    hasError: Boolean(streamState.lastError),
                    runningToolCount: runningCount,
                    doneToolCount: doneCount,
                  })}
                  steps={liveSteps}
                  responderLabel={selectedAg?.display_name}
                  responderAvatar={selectedAg?.emoji}
                  reconnectNotice={reconnectNotice}
                  onCitationClick={onOpenArticle}
                />
              )
            }
            return draftsList.map((d) => {
              const ag = agentMap.get(d.agent_slug)
              const seatPills = streamState.toolPills.filter(
                (p) => !p.agent_slug || p.agent_slug === d.agent_slug,
              )
              const liveSteps = seatPills.map((p) => mapToolToTraceStepV2(p))
              const runningCount = liveSteps.filter((s) => s.state === 'running').length
              const doneCount = liveSteps.length - runningCount
              return (
                <div key={d.agent_slug} className="salon-turn salon-turn-agent streaming">
                  <div className="salon-turn-card">
                    <header className="salon-turn-meta">
                      <b>
                        {ag?.emoji ?? '📈'} {ag?.display_name ?? `@${d.agent_slug}`}
                      </b>
                      <em>{c.seatSpeaking}</em>
                    </header>
                    <AgentExecutionTrace
                      phase={deriveAgentStreamPhaseV2({
                        isLive: true,
                        hasError: Boolean(streamState.lastError),
                        hasText: Boolean(d.text),
                        hasReasoning: Boolean(d.reasoning),
                        runningToolCount: runningCount,
                        doneToolCount: doneCount,
                      })}
                      thinkingText={d.reasoning || undefined}
                      steps={liveSteps}
                      responderLabel={ag?.display_name ?? d.agent_slug}
                      responderAvatar={ag?.emoji}
                      reconnectNotice={reconnectNotice}
                      onCitationClick={onOpenArticle}
                    />
                    <div className="salon-turn-prose">
                      <AgentMarkdown content={d.text} onCitationClick={onOpenArticle} />
                    </div>
                  </div>
                </div>
              )
            })
          })()}
      </div>

      <AgentSelectionToolbar
        anchor={selectionBar?.anchor ?? null}
        coords={selectionBar?.coords ?? null}
        showListen={Boolean(caps?.voice_tts)}
        responders={responderOptions}
        selectedResponderSlug={selectedAgentSlug}
        onSelectResponder={setSelectedAgentSlug}
        onAddToChat={(anchor) => {
          const quoted = anchor.quote
            .split('\n')
            .map((line) => `> ${line}`)
            .join('\n')
          setDraft((prev) => (prev.trim() ? `${quoted}\n\n${prev}` : `${quoted}\n\n`))
          setSelectionBar(null)
          window.getSelection()?.removeAllRanges()
          draftRef.current?.focus()
        }}
        onAsk={(anchor, slug) => {
          const target = slug || selectedAgentSlug || 'macro-argus'
          setSelectedAgentSlug(target)
          setSelectionBar(null)
          window.getSelection()?.removeAllRanges()
          void ensureThreadAndSend(
            `@${target} ${lang === 'vi' ? 'Phân tích đoạn trích này:' : 'Analyze this excerpt:'}\n> ${anchor.quote.replace(/\n+/g, ' ')}`,
          )
        }}
        onDismiss={() => setSelectionBar(null)}
      />

      {(error || streamState.lastError) && <p className="form-error">{error ?? streamState.lastError}</p>}

      <footer className="companion-footer">
        <AgentComposerLead
          selectedSkills={selectedSkillOptionsV2}
          onRemoveSkill={(slug) =>
            setAttachedSkills((prev) => prev.filter((item) => item !== slug))
          }
          applyEveryTurn={applyEveryTurn}
        />
        <textarea
          ref={draftRef}
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
        <div className="companion-footer-bar">
          <span className="muted-sub" style={{ fontSize: 12 }}>
            {c.companionComposerHint}
          </span>
          <button
            type="button"
            className="pplx-submit-btn"
            aria-label={sending || creatingThread ? c.sendingTurnBtn : c.sendTurnBtn}
            title={sending || creatingThread ? c.sendingTurnBtn : c.sendTurnBtn}
            disabled={sending || creatingThread || !draft.trim()}
            onClick={() => void ensureThreadAndSend(draft)}
          >
            <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
              <line x1="12" y1="19" x2="12" y2="5" />
              <polyline points="5 12 12 5 19 12" />
            </svg>
          </button>
        </div>
      </footer>
    </aside>
  )
}
