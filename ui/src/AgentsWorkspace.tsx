import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { AgentStudio } from './AgentStudio.tsx'
import { AgentSelect } from './agent-kit/AgentSelect.tsx'
import { AgentSplitLayout } from './agent-kit/AgentSplitPane.tsx'
import {
  AGENT_COPY,
  formatBridgeModeLabel,
  formatDiscussionModeShort,
  type AccessRequestTargetKind,
  type AgentMessage,
  type AgentThread,
  type AgentWorkspace,
  type AppAgentConfig,
  type CoWriterDraftRow,
  type DiscussionMode,
  type ExecutionTraceStep,
  type MemoryGraphNode,
  type MergedToolInvocation,
  type WorkspaceSurface,
} from './agentTypes.ts'
import {
  computeEffectiveScope,
  extractMentions,
  buildSeatTraceId,
  filterSurfacesByManifest,
  parseComposerTrigger,
  resolveModelRoles,
  skillDisplayName,
  skillGroupDisplayName,
  stripSkillFrontmatter,
  useAppAgents,
  useThreadStream,
} from './useAppAgents.ts'

export interface AgentsWorkspaceProps {
  lang: 'vi' | 'en'
  onOpenArticle?: (articleId: number) => void
  onOpenRegions?: () => void
}

type NewsmanSettingsTab = 'connections' | 'models' | 'mcp' | 'memory' | 'general' | 'telegram'

const DISCUSSION_MODES: readonly {
  id: DiscussionMode
  labelKey: keyof typeof AGENT_COPY.vi
  descKey: keyof typeof AGENT_COPY.vi
}[] = [
  { id: 'parallel_panel', labelKey: 'modeParallelPanel', descKey: 'modeParallelPanelDesc' },
  { id: 'sequential_build', labelKey: 'modeSequentialBuild', descKey: 'modeSequentialBuildDesc' },
  { id: 'cross_debate', labelKey: 'modeCrossDebate', descKey: 'modeCrossDebateDesc' },
  { id: 'mention_or_lead', labelKey: 'modeMentionOrLead', descKey: 'modeMentionOrLeadDesc' },
] as const

export function ToolInvocationPill({
  pill,
  lang,
}: {
  pill: MergedToolInvocation
  lang: 'vi' | 'en'
}) {
  const c = AGENT_COPY[lang]
  const statusLabel =
    pill.status === 'approval_required'
      ? c.toolApprovalRequired
      : pill.status === 'running'
        ? c.toolCallRunning
        : pill.status === 'error'
          ? c.toolCallError
          : c.toolCallDone

  const badgeClass =
    pill.status === 'approval_required'
      ? 'tier-badge gated'
      : pill.status === 'running'
        ? 'tier-badge running'
        : pill.status === 'error'
          ? 'tier-badge error'
          : 'tier-badge'

  const hasScope =
    Boolean(pill.resource_ctx?.regions?.length) || Boolean(pill.resource_ctx?.topics?.length)

  return (
    <details className={`agent-tool-pill status-${pill.status}`}>
      <summary>
        <span className={badgeClass}>{statusLabel}</span>
        <code className="agent-tool-name">{pill.tool}</code>
        {pill.agent_slug && <span className="agent-tool-caller">@{pill.agent_slug}</span>}
        {hasScope && (
          <span className="agent-v4s-chip" title={c.salonScopeEffectiveLabel}>
            ⛨ {c.v4sScopedBadge}
            {pill.resource_ctx?.regions?.length ? ` · ${pill.resource_ctx.regions.join(', ')}` : ''}
            {pill.resource_ctx?.topics?.length ? ` · #${pill.resource_ctx.topics.join(', ')}` : ''}
          </span>
        )}
      </summary>
      <div className="agent-tool-details">
        {pill.arguments !== undefined && (
          <pre className="agent-tool-code">
            {typeof pill.arguments === 'string' ? pill.arguments : JSON.stringify(pill.arguments, null, 2)}
          </pre>
        )}
        {pill.approval_required && (
          <div className="agent-approval-banner">
            <strong>⚠ {pill.approval_required.action_type}</strong>
            <p>{pill.approval_required.summary}</p>
            {pill.approval_required.request_ref && <code>ref: {pill.approval_required.request_ref}</code>}
          </div>
        )}
        {pill.result_text && <pre className="agent-tool-code result">{pill.result_text}</pre>}
      </div>
    </details>
  )
}

function emptyWorkspaceDraft(): AgentWorkspace {
  return {
    id: '',
    name: '',
    description: '',
    emoji: '🗂️',
    custom_instructions_md: '',
    resource_ctx: {},
    default_skill_slugs: [],
    default_agent_slugs: ['macro-argus', 'editor-minh'],
    default_discussion_mode: 'mention_or_lead',
    is_default: false,
  }
}

export function AgentsWorkspace({ lang, onOpenArticle, onOpenRegions }: AgentsWorkspaceProps) {
  const {
    bridgeStatus,
    manifest,
    modelCatalog,
    mcpNamespaces,
    customProviders,
    memoryGraph,
    agents,
    models,
    threads,
    workspaces,
    activeWorkspaceId,
    setActiveWorkspaceId,
    skills,
    skillGroups,
    capabilityBundles,
    mcpGroups,
    allowedMcpTools,
    allowedModelAliases,
    channelGrants,
    accessRequests,
    probeReport,
    probingConnections,
    syncingSkills,
    schedules,
    telegramConfig,
    telegramRoutes,
    saveBridgeConfig,
    probeConnections,
    fetchModelCatalog,
    toggleCatalogModels,
    saveModelRoles,
    saveMcpNamespaces,
    createCustomProvider,
    deleteCustomProvider,
    saveAgent,
    deleteAgent,
    pullAgentFromHub,
    pushAgentDraft,
    createThread,
    updateThread,
    deleteThread,
    branchThread,
    saveWorkspace,
    deleteWorkspace,
    syncSkills,
    toggleSkillEnabled,
    requestSkillAccess,
    suggestCowriterDiff,
    decideCowriterDraft,
    saveMemoryNode,
    updateMemoryNode,
    deleteMemoryNode,
    auditMemory,
    dedupMemory,
    createSchedule,
    updateSchedule,
    deleteSchedule,
    runScheduleNow,
    saveTelegramConfig,
    createTelegramRoute,
    deleteTelegramRoute,
    generateTelegramLinkCode,
  } = useAppAgents()

  const uiLang: 'vi' | 'en' = bridgeStatus.interface_language === 'vi' ? 'vi' : 'en'
  void lang
  const c = AGENT_COPY[uiLang]
  const surfaceGate = useMemo(() => filterSurfacesByManifest(manifest), [manifest])
  const safeMemoryGraph = useMemo(
    () =>
      memoryGraph ?? {
        nodes: [],
        edges: [],
        l1_scratchpad: '',
        stats: { total_nodes: 0, total_edges: 0, l1_count: 0, l2_count: 0, l3_count: 0 },
      },
    [memoryGraph],
  )

  const [initialRequestedSurface] = useState<string | null>(() => {
    try {
      const requested = sessionStorage.getItem('lnc-newsman-agents-surface')
      if (requested) sessionStorage.removeItem('lnc-newsman-agents-surface')
      return requested
    } catch {
      return null
    }
  })

  const [surface, setSurface] = useState<'chat' | 'cowriter' | 'studio' | 'memory' | 'settings'>(() => {
    if (
      initialRequestedSurface === 'models' ||
      initialRequestedSurface === 'settings' ||
      initialRequestedSurface === 'connections'
    ) {
      return 'settings'
    }
    if (
      initialRequestedSurface === 'agents' ||
      initialRequestedSurface === 'workspaces' ||
      initialRequestedSurface === 'skills' ||
      initialRequestedSurface === 'studio'
    ) {
      return 'studio'
    }
    if (initialRequestedSurface === 'cowriter') return 'cowriter'
    if (initialRequestedSurface === 'memory') return 'memory'
    return 'chat'
  })
  const [studioSubTab, setStudioSubTab] = useState<'agents' | 'workspaces' | 'skills'>(() => {
    if (initialRequestedSurface === 'workspaces') return 'workspaces'
    if (initialRequestedSurface === 'skills') return 'skills'
    return 'agents'
  })
  const caps = manifest?.capabilities ?? manifest?.features
  const visibleSettingsTabs = useMemo<NewsmanSettingsTab[]>(() => {
    const tabs: NewsmanSettingsTab[] = ['connections', 'models', 'mcp', 'memory', 'general']
    if (caps?.telegram !== false) tabs.push('telegram')
    return tabs
  }, [caps?.telegram])
  const [settingsTab, setSettingsTab] = useState<NewsmanSettingsTab>(() =>
    initialRequestedSurface === 'models' ? 'models' : 'connections',
  )
  useEffect(() => {
    if (!visibleSettingsTabs.includes(settingsTab)) {
      setSettingsTab('connections')
    }
  }, [settingsTab, visibleSettingsTabs])
  const [selectedThreadId, setSelectedThreadId] = useState<number | string | null>(null)
  const [railOpen, setRailOpen] = useState(true)
  const [sidePanelOpen, setSidePanelOpen] = useState(false)
  const [sidePanelTab, setSidePanelTab] = useState<'trace' | 'context'>('trace')
  const [selectedTraceId, setSelectedTraceId] = useState<string | null>(null)
  const [composerText, setComposerText] = useState('')
  const [caretPos, setCaretPos] = useState(0)
  const [turnSkillSlugs, setTurnSkillSlugs] = useState<string[]>([])
  const [turnModelOverride, setTurnModelOverride] = useState('')
  const [copiedMsgId, setCopiedMsgId] = useState<string | null>(null)
  const [composerPopover, setComposerPopover] = useState<
    null | 'scope' | 'mode' | 'skills' | 'mcp' | 'style' | 'customize'
  >(null)
  const [chatMode, setChatMode] = useState<'chat' | 'co_writer' | 'research' | 'brief'>('chat')
  const [personaStyle, setPersonaStyle] = useState<string>('analytical')
  const [turnScopeFilter, setTurnScopeFilter] = useState({ region: '', series: '', topic: '' })
  const [ghostHint, setGhostHint] = useState('')
  const [notesDraft, setNotesDraft] = useState('')
  const [notesSaving, setNotesSaving] = useState(false)
  const [askCustomAnswers, setAskCustomAnswers] = useState<Record<string, string>>({})
  const [paletteOpen, setPaletteOpen] = useState(false)
  const [paletteQuery, setPaletteQuery] = useState('')

  const [newRoomOpen, setNewRoomOpen] = useState(false)
  const [newRoomKind, setNewRoomKind] = useState<'direct' | 'group'>('group')
  const [newRoomTitle, setNewRoomTitle] = useState('')
  const [newRoomMembers, setNewRoomMembers] = useState<string[]>(['macro-argus', 'tax-linh', 'editor-minh'])
  const [newRoomLead, setNewRoomLead] = useState<string>('editor-minh')
  const [newRoomMode, setNewRoomMode] = useState<DiscussionMode>('parallel_panel')
  const [wbInput, setWbInput] = useState('')
  const [summarizing, setSummarizing] = useState(false)

  // Workspaces Surface state
  const [wsDraft, setWsDraft] = useState<AgentWorkspace>(() => emptyWorkspaceDraft())
  const [wsRegionsInput, setWsRegionsInput] = useState('')
  const [wsTopicsInput, setWsTopicsInput] = useState('')
  const [wsStatus, setWsStatus] = useState('')
  const [wsError, setWsError] = useState('')
  const [wsBusy, setWsBusy] = useState(false)

  // Skill Hub Surface state
  const [skillGroupFilter, setSkillGroupFilter] = useState<string>('all')
  const [skillSearch, setSkillSearch] = useState('')
  const [selectedSkillSlug, setSelectedSkillSlug] = useState<string>('macro-cross-impact-brief')
  const [accessModalOpen, setAccessModalOpen] = useState(false)
  const [accessKind, setAccessKind] = useState<AccessRequestTargetKind>('skill')
  const [accessSlug, setAccessSlug] = useState('global-commodities-pack-sync')
  const [accessReason, setAccessReason] = useState('')
  const [accessNotice, setAccessNotice] = useState('')

  // Co-Writer Surface state
  const [cwTitle, setCwTitle] = useState('')
  const [cwArticleId, setCwArticleId] = useState('')
  const [cwAgentSlug, setCwAgentSlug] = useState('editor-minh')
  const [cwBefore, setCwBefore] = useState('')
  const [cwInstruction, setCwInstruction] = useState('')
  const [cwActiveDraft, setCwActiveDraft] = useState<CoWriterDraftRow | null>(null)
  const [cwHistory, setCwHistory] = useState<CoWriterDraftRow[]>([])
  const [cwBusy, setCwBusy] = useState(false)
  const [cwNotice, setCwNotice] = useState('')

  // Memory Graph Surface state
  const [memLayer, setMemLayer] = useState<'L2' | 'L3'>('L2')
  const [memKind, setMemKind] = useState('canon_rule')
  const [memTitle, setMemTitle] = useState('')
  const [memContent, setMemContent] = useState('')
  const [memSourceRef, setMemSourceRef] = useState('')
  const [memBusy, setMemBusy] = useState(false)
  const [memNotice, setMemNotice] = useState('')
  const [memEditingId, setMemEditingId] = useState<string | null>(null)
  const [memEditContent, setMemEditContent] = useState('')

  // Settings / v2 Custom Provider & Roles state
  const [provName, setProvName] = useState('')
  const [provBaseUrl, setProvBaseUrl] = useState('')
  const [provApiKey, setProvApiKey] = useState('')
  const [provBusy, setProvBusy] = useState(false)
  const [mcpDraft, setMcpDraft] = useState<Record<string, boolean>>({})

  const openAccessModalFor = (kind: AccessRequestTargetKind, slug: string, label?: string) => {
    setAccessKind(kind)
    setAccessSlug(slug)
    if (label) {
      setAccessReason(
        uiLang === 'vi'
          ? `Yêu cầu cấp quyền ${label} từ LNC Newsman.`
          : `Requesting access to ${label} from LNC Newsman.`,
      )
    }
    setAccessModalOpen(true)
  }

  const formatTargetKindTag = (kind: string): string => {
    switch (kind) {
      case 'capability_bundle':
        return c.targetKindBundleTag
      case 'mcp_group':
        return c.targetKindMcpGroupTag
      case 'skill_group':
        return c.targetKindGroupTag
      case 'model_alias':
        return c.targetKindModelTag
      case 'channel':
        return c.targetKindChannelTag
      default:
        return c.targetKindSkillTag
    }
  }

  const formatRouteLabel = (val: string | undefined): string => {
    switch (val) {
      case 'hub':
        return c.routeViaHub
      case 'direct':
        return c.routeViaDirect
      case 'hub_cygnus':
        return c.routeViaHubCygnus
      case 'direct_cygnus':
        return c.routeViaDirectCygnus
      case 'direct_bot_api':
        return c.routeViaDirectBot
      default:
        return c.routeDisabled
    }
  }

  // Settings Surface state
  const [connForm, setConnForm] = useState({
    astraea_url: bridgeStatus.astraea_url ?? '',
    hub_url: bridgeStatus.hub_url ?? '',
    llm_base_url: bridgeStatus.llm_base_url ?? '',
    default_model: bridgeStatus.default_model ?? 'gpt-5.4-mini',
    default_smart_model: bridgeStatus.default_smart_model ?? bridgeStatus.default_model ?? 'gpt-5.4-mini',
    fast_background_model: bridgeStatus.fast_background_model ?? '',
    interface_language: (bridgeStatus.interface_language ?? 'en') as 'en' | 'vi',
    model_output_language: (bridgeStatus.model_output_language ?? 'vi') as 'vi' | 'en' | 'auto',
    send_key_mode: (bridgeStatus.send_key_mode ?? 'enter') as 'enter' | 'ctrl_enter',
    max_tool_rounds: bridgeStatus.max_tool_rounds ?? 8,
    max_group_hops: bridgeStatus.max_group_hops ?? 3,
    default_temperature: bridgeStatus.default_temperature ?? 0.4,
    allowed_hosts: (bridgeStatus.allowed_hosts ?? []).join(', '),
    astraea_token: '',
    hub_token: '',
    llm_api_key: '',
    thread_title: bridgeStatus.task_models?.thread_title ?? '',
    round_summary: bridgeStatus.task_models?.round_summary ?? '',
    companion_brief: bridgeStatus.task_models?.companion_brief ?? '',
    tax_doc_digest: bridgeStatus.task_models?.tax_doc_digest ?? '',
  })
  const [connNotice, setConnNotice] = useState('')
  const [connError, setConnError] = useState('')

  const listEndRef = useRef<HTMLDivElement | null>(null)
  const composerRef = useRef<HTMLTextAreaElement | null>(null)

  const activeWorkspace = useMemo(
    () => workspaces.find((w) => w.id === activeWorkspaceId) ?? null,
    [activeWorkspaceId, workspaces],
  )

  const grantedSkills = useMemo(
    () => skills.filter((s) => s.granted && s.enabled),
    [skills],
  )

  const enabledCatalogModels = useMemo(
    () => modelCatalog.filter((m) => m.enabled).map((m) => m.model_id),
    [modelCatalog],
  )
  const selectableModels = useMemo(
    () => (enabledCatalogModels.length > 0 ? enabledCatalogModels : models),
    [enabledCatalogModels, models],
  )
  const roleSummary = useMemo(() => resolveModelRoles(bridgeStatus), [bridgeStatus])

  useEffect(() => {
    const next: Record<string, boolean> = {}
    for (const ns of mcpNamespaces) next[ns.namespace] = ns.enabled
    setMcpDraft(next)
  }, [mcpNamespaces])

  useEffect(() => {
    setConnForm((prev) => ({
      ...prev,
      astraea_url: bridgeStatus.astraea_url ?? prev.astraea_url,
      hub_url: bridgeStatus.hub_url ?? prev.hub_url,
      llm_base_url: bridgeStatus.llm_base_url ?? prev.llm_base_url,
      default_model: bridgeStatus.default_model ?? prev.default_model,
      default_smart_model:
        bridgeStatus.default_smart_model ?? bridgeStatus.default_model ?? prev.default_smart_model,
      fast_background_model: bridgeStatus.fast_background_model ?? '',
      interface_language: (bridgeStatus.interface_language ?? prev.interface_language) as 'en' | 'vi',
      model_output_language: (bridgeStatus.model_output_language ??
        prev.model_output_language) as 'vi' | 'en' | 'auto',
      send_key_mode: (bridgeStatus.send_key_mode ?? prev.send_key_mode) as 'enter' | 'ctrl_enter',
      max_tool_rounds: bridgeStatus.max_tool_rounds ?? prev.max_tool_rounds,
      max_group_hops: bridgeStatus.max_group_hops ?? prev.max_group_hops,
      default_temperature: bridgeStatus.default_temperature ?? prev.default_temperature,
      allowed_hosts: (bridgeStatus.allowed_hosts ?? []).join(', '),
      thread_title: bridgeStatus.task_models?.thread_title ?? '',
      round_summary: bridgeStatus.task_models?.round_summary ?? '',
      companion_brief: bridgeStatus.task_models?.companion_brief ?? '',
      tax_doc_digest: bridgeStatus.task_models?.tax_doc_digest ?? '',
    }))
  }, [bridgeStatus])

  useEffect(() => {
    if (workspaces.length > 0 && !wsDraft.id && !wsDraft.name) {
      const target = activeWorkspace ?? workspaces[0]
      setWsDraft(target)
      setWsRegionsInput((target.resource_ctx.regions ?? []).join(', '))
      setWsTopicsInput((target.resource_ctx.topics ?? []).join(', '))
    }
  }, [activeWorkspace, workspaces, wsDraft.id, wsDraft.name])

  useEffect(() => {
    if (skills.length > 0 && !skills.some((s) => s.slug === selectedSkillSlug)) {
      setSelectedSkillSlug(skills[0].slug)
    }
  }, [selectedSkillSlug, skills])

  const filteredThreads = useMemo(() => {
    if (!activeWorkspaceId) return threads
    const matching = threads.filter((t) => !t.workspace_id || t.workspace_id === activeWorkspaceId)
    return matching.length > 0 ? matching : threads
  }, [activeWorkspaceId, threads])

  const pinnedThreads = useMemo(() => filteredThreads.filter((t) => t.pinned), [filteredThreads])
  const recentThreads = useMemo(() => filteredThreads.filter((t) => !t.pinned), [filteredThreads])

  useEffect(() => {
    if (filteredThreads.length > 0 && selectedThreadId === null) {
      setSelectedThreadId(filteredThreads[0].id)
    }
  }, [filteredThreads, selectedThreadId])

  const activeThread = useMemo(
    () => threads.find((t) => String(t.id) === String(selectedThreadId)) ?? null,
    [selectedThreadId, threads],
  )

  useEffect(() => {
    setNotesDraft(activeThread?.scratchpad_md ?? '')
    const cm = activeThread?.chat_mode
    if (cm === 'co_writer' || cm === 'research' || cm === 'brief' || cm === 'chat') {
      setChatMode(cm)
    }
    if (activeThread?.persona_style) {
      setPersonaStyle(activeThread.persona_style)
    }
  }, [activeThread?.id, activeThread?.scratchpad_md, activeThread?.chat_mode, activeThread?.persona_style])

  const {
    messages,
    whiteboard,
    askUserPrompts,
    streamState,
    streamConnected,
    sending,
    error,
    sendTurn,
    answerAskUser,
    retrySeat,
    requestGhostHint,
    addWhiteboardNote,
    deleteWhiteboardNote,
    summarizeRound,
  } = useThreadStream(surface === 'chat' ? selectedThreadId : null)

  useEffect(() => {
    listEndRef.current?.scrollIntoView({ behavior: 'smooth', block: 'end' })
  }, [messages.length, streamState.drafts])

  const agentMap = useMemo(() => {
    const map = new Map<string, AppAgentConfig>()
    for (const a of agents) map.set(a.slug, a)
    return map
  }, [agents])

  const seatOrder = useMemo(() => {
    if (activeThread?.member_slugs?.length) {
      return activeThread.member_slugs.map((slug) => agentMap.get(slug)).filter((a): a is AppAgentConfig => Boolean(a))
    }
    return agents
  }, [activeThread, agentMap, agents])

  const threadWorkspace = useMemo(
    () =>
      (activeThread?.workspace_id
        ? workspaces.find((w) => w.id === activeThread.workspace_id)
        : null) ??
      activeWorkspace ??
      null,
    [activeThread?.workspace_id, activeWorkspace, workspaces],
  )

  const effectiveThreadScope = useMemo(
    () => computeEffectiveScope(threadWorkspace, seatOrder),
    [seatOrder, threadWorkspace],
  )

  const traceEntries = useMemo(() => {
    return messages
      .filter((m) => m.role !== 'user' && (m.reasoning || (m.tool_calls && m.tool_calls.length > 0)))
      .map((m) => ({
        traceId: buildSeatTraceId(m),
        message: m,
        agent: m.sender_slug ? agentMap.get(m.sender_slug) : undefined,
      }))
  }, [agentMap, messages])

  const activeTrigger = useMemo(() => parseComposerTrigger(composerText, caretPos), [caretPos, composerText])

  const autocompleteItems = useMemo(() => {
    if (!activeTrigger) return []
    const q = activeTrigger.query.toLowerCase()
    if (activeTrigger.trigger === '@') {
      return agents
        .filter((a) => a.slug.toLowerCase().includes(q) || a.display_name.toLowerCase().includes(q))
        .slice(0, 6)
        .map((a) => ({ kind: '@' as const, slug: a.slug, label: `${a.emoji} ${a.display_name}`, sub: `@${a.slug}` }))
    }
    return grantedSkills
      .filter((s) => s.slug.toLowerCase().includes(q) || s.name.toLowerCase().includes(q))
      .slice(0, 6)
      .map((s) => ({ kind: '/' as const, slug: s.slug, label: `🧩 ${s.name}`, sub: `/${s.slug}` }))
  }, [activeTrigger, agents, grantedSkills])

  const directThreads = useMemo(() => filteredThreads.filter((t) => t.kind === 'direct'), [filteredThreads])
  const groupThreads = useMemo(() => filteredThreads.filter((t) => t.kind === 'group'), [filteredThreads])

  const activeGroupObj = useMemo(
    () => (skillGroupFilter === 'all' ? null : skillGroups.find((g) => g.slug === skillGroupFilter) ?? null),
    [skillGroupFilter, skillGroups],
  )

  const filteredSkills = useMemo(() => {
    const q = skillSearch.trim().toLowerCase()
    return skills.filter((sk) => {
      if (skillGroupFilter !== 'all') {
        const inSkillGroups = sk.group_slugs.includes(skillGroupFilter)
        const inGroupList = activeGroupObj?.skill_slugs.includes(sk.slug) ?? false
        if (!inSkillGroups && !inGroupList) return false
      }
      if (!q) return true
      return (
        sk.slug.toLowerCase().includes(q) ||
        sk.name.toLowerCase().includes(q) ||
        (sk.display_name ?? '').toLowerCase().includes(q) ||
        sk.description.toLowerCase().includes(q) ||
        sk.category.toLowerCase().includes(q)
      )
    })
  }, [activeGroupObj, skillGroupFilter, skillSearch, skills])

  const selectedSkill = useMemo(
    () =>
      filteredSkills.find((s) => s.slug === selectedSkillSlug) ??
      filteredSkills[0] ??
      skills.find((s) => s.slug === selectedSkillSlug) ??
      null,
    [filteredSkills, selectedSkillSlug, skills],
  )

  const handleAttachSkillToSalon = (slug: string) => {
    const targetSkill = skills.find((s) => s.slug === slug)
    if (targetSkill && targetSkill.granted && !targetSkill.enabled) {
      void toggleSkillEnabled(slug, true)
    }
    setTurnSkillSlugs((prev) => (prev.includes(slug) ? prev : [...prev, slug]))
    if (selectedThreadId === null && threads.length > 0) {
      setSelectedThreadId(threads[0].id)
    }
    setAccessNotice(c.skillsAttachedNotice)
    setSurface('chat')
  }

  const handleRequestGroupPack = async (groupSlug: string, displayLabel: string) => {
    const created = await requestSkillAccess({
      target_kind: 'skill_group',
      target_slug: groupSlug,
      reason:
        uiLang === 'vi'
          ? `Yêu cầu cấp quyền gói kỹ năng ${displayLabel} từ Newsman Desk.`
          : `Requesting skill group pack ${displayLabel} from Newsman Desk.`,
    })
    if (created) {
      setAccessNotice(`${displayLabel}: ${created.status}`)
    }
  }

  const handleLaunchSalonWithWorkspace = async (ws: AgentWorkspace) => {
    setActiveWorkspaceId(ws.id)
    const existing = threads.find((t) => t.workspace_id === ws.id)
    if (existing) {
      setSelectedThreadId(existing.id)
      setSurface('chat')
      return
    }
    setWsBusy(true)
    setWsError('')
    try {
      const members =
        ws.default_agent_slugs.length > 0 ? ws.default_agent_slugs : ['macro-argus', 'editor-minh']
      const created = await createThread({
        kind: members.length > 1 ? 'group' : 'direct',
        title: ws.name,
        workspace_id: ws.id,
        member_slugs: members,
        lead_slug: members[0],
        discussion_mode: ws.default_discussion_mode ?? 'mention_or_lead',
        pinned_skills: ws.default_skill_slugs ?? [],
      })
      setSelectedThreadId(created.id)
      setSurface('chat')
    } catch (err) {
      setWsError(err instanceof Error ? err.message : 'Failed to launch thread')
    } finally {
      setWsBusy(false)
    }
  }

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault()
        setPaletteOpen((v) => !v)
        setPaletteQuery('')
      } else if (e.key === 'Escape' && paletteOpen) {
        setPaletteOpen(false)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [paletteOpen])

  useEffect(() => {
    const trimmed = composerText.trim()
    if (surface !== 'chat' || trimmed.length < 10 || sending) {
      setGhostHint('')
      return
    }
    let alive = true
    const timer = window.setTimeout(() => {
      void requestGhostHint(trimmed).then((hint) => {
        if (alive && hint && hint !== trimmed) setGhostHint(hint)
      })
    }, 650)
    return () => {
      alive = false
      window.clearTimeout(timer)
    }
  }, [composerText, requestGhostHint, sending, surface])

  useLayoutEffect(() => {
    const el = composerRef.current
    if (!el) return
    el.style.height = 'auto'
    const nextHeight = Math.max(44, Math.min(180, el.scrollHeight))
    el.style.height = `${nextHeight}px`
  }, [composerText, surface])

  const handleOpenTrace = (traceId: string) => {
    setSelectedTraceId(traceId)
    setSidePanelTab('trace')
    setSidePanelOpen(true)
  }

  const handleInsertTrigger = (trigger: '@' | '/') => {
    const el = composerRef.current
    const pos = el?.selectionStart ?? composerText.length
    const before = composerText.slice(0, pos)
    const after = composerText.slice(pos)
    const needsLeadingSpace = before.length > 0 && !/\s$/.test(before)
    const prefix = needsLeadingSpace ? ` ${trigger}` : trigger
    const next = `${before}${prefix}${after}`
    const nextCaret = before.length + prefix.length
    setComposerText(next)
    setCaretPos(nextCaret)
    requestAnimationFrame(() => {
      if (!composerRef.current) return
      composerRef.current.focus()
      composerRef.current.setSelectionRange(nextCaret, nextCaret)
    })
  }

  const handleDeleteThread = (th: AgentThread) => {
    if (!window.confirm(`${c.deleteThreadConfirm}\n\n“${th.title}”`)) return
    void deleteThread(th.id)
    if (String(selectedThreadId) === String(th.id)) setSelectedThreadId(null)
  }

  const handleBranchFromMessage = async (msgId: string | number) => {
    if (!activeThread) return
    const branched = await branchThread(activeThread.id, { from_message_id: String(msgId) })
    if (branched) setSelectedThreadId(branched.id)
  }

  const handleApplyAutocomplete = (item: { kind: '@' | '/'; slug: string }) => {
    if (!activeTrigger) return
    const before = composerText.slice(0, activeTrigger.start)
    const after = composerText.slice(activeTrigger.end)
    if (item.kind === '@') {
      const inserted = `@${item.slug} `
      const next = `${before}${inserted}${after}`
      setComposerText(next)
      setCaretPos(before.length + inserted.length)
    } else {
      setTurnSkillSlugs((prev) => (prev.includes(item.slug) ? prev : [...prev, item.slug]))
      const next = `${before}${after}`.replace(/\s{2,}/g, ' ')
      setComposerText(next)
      setCaretPos(before.length)
    }
    composerRef.current?.focus()
  }

  const handleCreateRoom = async () => {
    const title = newRoomTitle.trim() || (newRoomKind === 'group' ? 'Editorial Panel' : 'Direct Analyst Brief')
    const members = newRoomMembers.length > 0 ? newRoomMembers : [agents[0]?.slug ?? 'macro-argus']
    const created = await createThread({
      kind: newRoomKind,
      title,
      member_slugs: newRoomKind === 'direct' ? [members[0]] : members,
      discussion_mode: newRoomKind === 'direct' ? 'mention_or_lead' : newRoomMode,
      lead_slug: newRoomLead || members[0],
      workspace_id: activeWorkspace?.id ?? null,
      pinned_skills: activeWorkspace?.default_skill_slugs ?? [],
    })
    setSelectedThreadId(created.id)
    setNewRoomTitle('')
    setNewRoomOpen(false)
  }

  const handleSend = async (overrideText?: string) => {
    const baseText = (overrideText ?? composerText).trim()
    if (!baseText) return
    const scopeParts = [
      turnScopeFilter.region.trim() ? `region:${turnScopeFilter.region.trim()}` : '',
      turnScopeFilter.series.trim() ? `series:${turnScopeFilter.series.trim()}` : '',
      turnScopeFilter.topic.trim() ? `topic:${turnScopeFilter.topic.trim()}` : '',
    ].filter(Boolean)
    const raw =
      scopeParts.length > 0 && !overrideText
        ? `${baseText}\n[Scope: ${scopeParts.join(' · ')}]`
        : baseText
    if (selectedThreadId === null) {
      const defAgents = activeWorkspace?.default_agent_slugs?.length
        ? activeWorkspace.default_agent_slugs
        : [agents[0]?.slug ?? 'macro-argus']
      const created = await createThread({
        kind: defAgents.length > 1 ? 'group' : 'direct',
        title: baseText.slice(0, 48),
        member_slugs: defAgents,
        discussion_mode: activeWorkspace?.default_discussion_mode ?? 'mention_or_lead',
        lead_slug: defAgents[0],
        workspace_id: activeWorkspace?.id ?? null,
        pinned_skills: activeWorkspace?.default_skill_slugs ?? [],
      })
      setSelectedThreadId(created.id)
    }
    const knownSlugs = agents.map((a) => a.slug)
    const mentions = extractMentions(raw, knownSlugs)
    if (!overrideText) {
      setComposerText('')
      setGhostHint('')
    }
    await sendTurn(raw, {
      mentions,
      skillSlugs: turnSkillSlugs,
      modelOverride: turnModelOverride || null,
      chatMode,
      personaStyle,
    })
  }

  const handleExportMemory = (format: 'json' | 'md') => {
    const content =
      format === 'json'
        ? JSON.stringify(safeMemoryGraph, null, 2)
        : [
            `# LNC Newsman Memory (${safeMemoryGraph.nodes.length} nodes)`,
            safeMemoryGraph.l1_scratchpad ? `## L1 Scratchpad\n${safeMemoryGraph.l1_scratchpad}` : '',
            ...safeMemoryGraph.nodes.map(
              (n) => `### [${n.layer}] ${n.title} (${n.node_kind})\n${n.content}\n_source: ${n.source_ref}_`,
            ),
          ]
            .filter(Boolean)
            .join('\n\n')
    const blob = new Blob([content], { type: format === 'json' ? 'application/json' : 'text/markdown' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `newsman-memory.${format}`
    a.click()
    URL.revokeObjectURL(url)
  }

  const renderMessageContent = (content: string) => {
    const parts = content.split(/(#article-\d+)/gi)
    return parts.map((part, idx) => {
      const m = /^#article-(\d+)$/i.exec(part)
      if (m && onOpenArticle) {
        const id = Number(m[1])
        return (
          <button key={idx} type="button" className="agent-citation-link" onClick={() => onOpenArticle(id)}>
            📰 #{id}
          </button>
        )
      }
      return <span key={idx}>{part}</span>
    })
  }

  const selectWorkspaceForEdit = (ws: AgentWorkspace) => {
    setWsDraft({ ...ws })
    setWsRegionsInput((ws.resource_ctx.regions ?? []).join(', '))
    setWsTopicsInput((ws.resource_ctx.topics ?? []).join(', '))
    setWsStatus('')
    setWsError('')
  }

  const handleSaveWorkspaceForm = async () => {
    if (!wsDraft.name.trim()) {
      setWsError(lang === 'vi' ? 'Tên không gian không được để trống.' : 'Workspace name is required.')
      return
    }
    setWsBusy(true)
    setWsError('')
    setWsStatus('')
    const parseCsv = (raw: string) =>
      raw
        .split(',')
        .map((s) => s.trim().toLowerCase())
        .filter(Boolean)
    const regions = parseCsv(wsRegionsInput)
    const topics = parseCsv(wsTopicsInput)
    try {
      const saved = await saveWorkspace({
        ...wsDraft,
        resource_ctx: {
          ...(regions.length > 0 ? { regions } : {}),
          ...(topics.length > 0 ? { topics } : {}),
        },
      })
      setWsDraft(saved)
      setActiveWorkspaceId(saved.id)
      setWsStatus(lang === 'vi' ? 'Đã lưu không gian.' : 'Workspace saved.')
    } catch (err) {
      setWsError(err instanceof Error ? err.message : 'Save failed')
    } finally {
      setWsBusy(false)
    }
  }

  const statusLabel = formatBridgeModeLabel(bridgeStatus, c)

  const composerBytes = new TextEncoder().encode(composerText).byteLength

  const workspaceLeadingIcon = (
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <path
        d="M2 4.2C2 3.54 2.54 3 3.2 3h3.1l1.4 1.6h5.1c.66 0 1.2.54 1.2 1.2v6c0 .66-.54 1.2-1.2 1.2H3.2C2.54 13 2 12.46 2 11.8V4.2Z"
        stroke="currentColor"
        strokeWidth="1.4"
        strokeLinejoin="round"
      />
    </svg>
  )

  const surfaceLabelMap: Record<'chat' | 'cowriter' | 'studio' | 'memory' | 'settings', string> = {
    chat: c.v2SurfaceChat,
    cowriter: c.v2SurfaceCowriter,
    studio: c.v2SurfaceStudio,
    memory: c.v2SurfaceMemory,
    settings: c.v2SurfaceSettings,
  }

  const settingsTabMeta: Record<NewsmanSettingsTab, { label: string; icon: string }> = {
    connections: {
      label: uiLang === 'vi' ? 'Kết nối Hub' : 'Connections',
      icon: '🔌',
    },
    models: {
      label: uiLang === 'vi' ? 'Mô hình ngôn ngữ' : 'Language Models',
      icon: '🧠',
    },
    mcp: {
      label: uiLang === 'vi' ? 'Công cụ & MCP' : 'Tools & MCP',
      icon: '🧰',
    },
    memory: {
      label: uiLang === 'vi' ? 'Trí nhớ 3 tầng' : 'Memory',
      icon: '🧬',
    },
    general: {
      label: uiLang === 'vi' ? 'Cài đặt chung' : 'General',
      icon: '⚙️',
    },
    telegram: {
      label: 'Telegram',
      icon: '✈️',
    },
  }

  const saveBridgeConfigPayload = (successMsg: string) => {
    setConnError('')
    setConnNotice('')
    const allowed = connForm.allowed_hosts
      .split(',')
      .map((h) => h.trim())
      .filter(Boolean)
    return void saveBridgeConfig({
      astraea_url: connForm.astraea_url.trim(),
      hub_url: connForm.hub_url.trim(),
      llm_base_url: connForm.llm_base_url.trim(),
      default_model: connForm.default_model.trim() || connForm.default_smart_model.trim() || 'gpt-5.4-mini',
      default_smart_model:
        connForm.default_smart_model.trim() || connForm.default_model.trim() || 'gpt-5.4-mini',
      fast_background_model: connForm.fast_background_model.trim(),
      interface_language: connForm.interface_language,
      model_output_language: connForm.model_output_language,
      send_key_mode: connForm.send_key_mode,
      max_tool_rounds: connForm.max_tool_rounds,
      max_group_hops: connForm.max_group_hops,
      default_temperature: connForm.default_temperature,
      allowed_hosts: allowed,
      task_models: {
        thread_title: connForm.thread_title || null,
        round_summary: connForm.round_summary || null,
        companion_brief: connForm.companion_brief || null,
        tax_doc_digest: connForm.tax_doc_digest || null,
      },
      ...(connForm.hub_token ? { hub_token: connForm.hub_token } : {}),
      ...(connForm.astraea_token ? { astraea_token: connForm.astraea_token } : {}),
      ...(connForm.llm_api_key ? { llm_api_key: connForm.llm_api_key } : {}),
    })
      .then(() => {
        setConnForm((p) => ({
          ...p,
          hub_token: '',
          astraea_token: '',
          llm_api_key: '',
        }))
        setConnNotice(successMsg)
      })
      .catch((err) => setConnError(err instanceof Error ? err.message : 'Save failed'))
  }

  const paletteEntries = useMemo(() => {
    const q = paletteQuery.trim().toLowerCase()
    const items: Array<{ id: string; label: string; hint: string; onPick: () => void }> = []
    for (const sId of surfaceGate.surfaces) {
      items.push({
        id: `surf:${sId}`,
        label: surfaceLabelMap[sId],
        hint: 'Surface',
        onPick: () => setSurface(sId),
      })
    }
    for (const qa of manifest?.quick_actions ?? []) {
      const label = (uiLang === 'vi' ? qa.label_vi : qa.label_en) ?? qa.label ?? qa.id
      items.push({
        id: `qa:${qa.id}`,
        label: `✦ ${label}`,
        hint: c.v2QuickActionsLabel,
        onPick: () => {
          setSurface('chat')
          setComposerText(qa.prompt ?? qa.prompt_template ?? '')
        },
      })
    }
    for (const th of threads.slice(0, 12)) {
      items.push({
        id: `thread:${th.id}`,
        label: th.title,
        hint: th.kind,
        onPick: () => {
          setSelectedThreadId(th.id)
          setSurface('chat')
        },
      })
    }
    for (const ag of agents) {
      items.push({
        id: `agent:${ag.slug}`,
        label: `${ag.emoji} ${ag.display_name}`,
        hint: `@${ag.slug}`,
        onPick: () => {
          setSurface('chat')
          setComposerText((prev) => (prev ? `${prev} @${ag.slug} ` : `@${ag.slug} `))
        },
      })
    }
    if (!q) return items.slice(0, 14)
    return items
      .filter((it) => it.label.toLowerCase().includes(q) || it.hint.toLowerCase().includes(q))
      .slice(0, 14)
  }, [agents, c.v2QuickActionsLabel, manifest?.quick_actions, paletteQuery, surfaceGate.surfaces, threads, uiLang])

  return (
    <div className="agents-workspace ak-scope">
      <div className="ak-surface-bar">
        <nav className="ak-tab-strip" role="tablist" aria-label={c.agentsWorkspaceTitle}>
          {surfaceGate.surfaces.map((id) => (
            <button
              key={id}
              type="button"
              role="tab"
              aria-selected={surface === id}
              className={surface === id ? 'ak-tab-btn active' : 'ak-tab-btn'}
              onClick={() => setSurface(id)}
            >
              {surfaceLabelMap[id]}
            </button>
          ))}
        </nav>
        <div className="ak-btn-group">
          <button
            type="button"
            className="btn secondary btn-sm"
            onClick={() => {
              setPaletteOpen(true)
              setPaletteQuery('')
            }}
            title="Ctrl+K / Cmd+K"
          >
            ⌘K
          </button>
          <AgentSelect
            size="sm"
            ariaLabel={c.workspaceActiveLabel}
            value={activeWorkspaceId ?? ''}
            onChange={(v) => setActiveWorkspaceId(v || null)}
            options={[
              { value: '', label: c.workspaceAll },
              ...workspaces.map((ws) => ({ value: ws.id, label: `${ws.emoji} ${ws.name}` })),
            ]}
          />
          <span className={`bridge-status-pill mode-${bridgeStatus.mode}`}>
            <span className="dot" />
            {statusLabel}
          </span>
        </div>
      </div>

      {paletteOpen && (
        <div
          className="ak-palette-backdrop"
          role="dialog"
          aria-label="Command Palette"
          onClick={() => setPaletteOpen(false)}
        >
          <div className="ak-palette-card" onClick={(e) => e.stopPropagation()}>
            <input
              autoFocus
              className="ak-palette-input"
              value={paletteQuery}
              placeholder={c.v2PalettePlaceholder}
              onChange={(e) => setPaletteQuery(e.target.value)}
            />
            <div className="ak-palette-list">
              {paletteEntries.map((item) => (
                <button
                  key={item.id}
                  type="button"
                  className="ak-palette-item"
                  onClick={() => {
                    item.onPick()
                    setPaletteOpen(false)
                  }}
                >
                  <span>{item.label}</span>
                  <span className="ak-select-badge">{item.hint}</span>
                </button>
              ))}
            </div>
          </div>
        </div>
      )}

      {surface === 'studio' && (
        <>
          <div className="ak-subtabs">
            {(
              [
                ['agents', c.surfaceAgents],
                ['workspaces', c.surfaceWorkspaces],
                ['skills', c.surfaceSkills],
              ] as const
            ).map(([subId, subLabel]) => (
              <button
                key={subId}
                type="button"
                className={studioSubTab === subId ? 'ak-tab-btn active' : 'ak-tab-btn'}
                onClick={() => setStudioSubTab(subId)}
              >
                {subLabel}
              </button>
            ))}
          </div>

          {studioSubTab === 'agents' && (
            <AgentStudio
              lang={uiLang}
              agents={agents}
              models={selectableModels}
              threads={threads}
              schedules={schedules}
              telegramConfig={telegramConfig}
              telegramRoutes={telegramRoutes}
              bridgeStatus={bridgeStatus}
              skills={skills}
              skillGroups={skillGroups}
              allowedMcpTools={allowedMcpTools}
              workspaces={workspaces}
              onSelectWorkspace={(id) => {
                setActiveWorkspaceId(id)
                const target = workspaces.find((w) => w.id === id)
                if (target) selectWorkspaceForEdit(target)
                setStudioSubTab('workspaces')
                setSurface('studio')
              }}
              onSaveAgent={saveAgent}
              onDeleteAgent={deleteAgent}
              onCreateSchedule={createSchedule}
              onUpdateSchedule={updateSchedule}
              onDeleteSchedule={deleteSchedule}
              onRunScheduleNow={runScheduleNow}
              onSaveTelegramConfig={saveTelegramConfig}
              onCreateTelegramRoute={createTelegramRoute}
              onDeleteTelegramRoute={deleteTelegramRoute}
              onSaveBridgeConfig={saveBridgeConfig}
              onPullAgentFromHub={pullAgentFromHub}
              onPushAgentDraft={pushAgentDraft}
              onGenerateTelegramLinkCode={generateTelegramLinkCode}
            />
          )}

          {studioSubTab === 'workspaces' && (
        <div className="workspaces-shell">
          <aside className="workspaces-list-pane">
            <div className="pane-head-row">
              <strong>{c.surfaceWorkspaces}</strong>
              <button
                type="button"
                className="btn small secondary"
                onClick={() => {
                  setWsDraft(emptyWorkspaceDraft())
                  setWsRegionsInput('')
                  setWsTopicsInput('')
                  setWsStatus('')
                  setWsError('')
                }}
              >
                {c.workspaceNewBtn}
              </button>
            </div>
            {workspaces.map((ws) => (
              <button
                key={ws.id}
                type="button"
                className={wsDraft.id === ws.id ? 'workspace-card active' : 'workspace-card'}
                onClick={() => selectWorkspaceForEdit(ws)}
              >
                <div className="workspace-card-head">
                  <span>
                    {ws.emoji} <strong>{ws.name}</strong>
                  </span>
                  {ws.is_default && <span className="tier-badge">{c.workspaceDefaultBadge}</span>}
                </div>
                <p>{ws.description}</p>
              </button>
            ))}
          </aside>

          <section className="workspace-editor-pane">
            <div className="tax-guardrail-callout">⚖️ {c.workspaceTaxReminder}</div>
            <div className="catalog-form-row">
              <label className="field">
                <span>{c.workspaceEmojiLabel}</span>
                <input
                  value={wsDraft.emoji}
                  onChange={(e) => setWsDraft((p) => ({ ...p, emoji: e.target.value }))}
                />
              </label>
              <label className="field">
                <span>{c.workspaceNameLabel}</span>
                <input
                  value={wsDraft.name}
                  onChange={(e) => setWsDraft((p) => ({ ...p, name: e.target.value }))}
                />
              </label>
              <div className="field">
                <span>{c.workspaceDefaultModeLabel}</span>
                <AgentSelect
                  ariaLabel={c.workspaceDefaultModeLabel}
                  value={wsDraft.default_discussion_mode}
                  onChange={(v) =>
                    setWsDraft((p) => ({
                      ...p,
                      default_discussion_mode: v as DiscussionMode,
                    }))
                  }
                  options={DISCUSSION_MODES.map((m) => ({ value: m.id, label: c[m.labelKey] }))}
                />
              </div>
            </div>

            <label className="field">
              <span>{c.workspaceDescLabel}</span>
              <input
                value={wsDraft.description}
                onChange={(e) => setWsDraft((p) => ({ ...p, description: e.target.value }))}
              />
            </label>

            <div className="catalog-form-row">
              <label className="field">
                <span>{c.workspaceRegionsLabel}</span>
                <input
                  value={wsRegionsInput}
                  placeholder="vn, global"
                  onChange={(e) => setWsRegionsInput(e.target.value)}
                />
              </label>
              <label className="field">
                <span>{c.workspaceTopicsLabel}</span>
                <input
                  value={wsTopicsInput}
                  placeholder="macro, tax, policy"
                  onChange={(e) => setWsTopicsInput(e.target.value)}
                />
              </label>
            </div>

            <label className="field">
              <span>{c.workspaceInstructionsLabel}</span>
              <textarea
                rows={4}
                value={wsDraft.custom_instructions_md}
                onChange={(e) =>
                  setWsDraft((p) => ({ ...p, custom_instructions_md: e.target.value }))
                }
              />
            </label>

            <div className="studio-field-group">
              <h4>{c.workspaceDefaultSkillsLabel}</h4>
              <div className="chip-row">
                {grantedSkills.map((sk) => {
                  const active = wsDraft.default_skill_slugs.includes(sk.slug)
                  return (
                    <button
                      key={sk.slug}
                      type="button"
                      className={active ? 'chip active' : 'chip'}
                      onClick={() =>
                        setWsDraft((p) => ({
                          ...p,
                          default_skill_slugs: active
                            ? p.default_skill_slugs.filter((s) => s !== sk.slug)
                            : [...p.default_skill_slugs, sk.slug],
                        }))
                      }
                    >
                      🧩 {sk.name}
                    </button>
                  )
                })}
              </div>
            </div>

            <div className="studio-field-group">
              <h4>{c.workspaceDefaultAgentsLabel}</h4>
              <div className="chip-row">
                {agents.map((ag) => {
                  const active = wsDraft.default_agent_slugs.includes(ag.slug)
                  return (
                    <button
                      key={ag.slug}
                      type="button"
                      className={active ? 'chip active' : 'chip'}
                      onClick={() =>
                        setWsDraft((p) => ({
                          ...p,
                          default_agent_slugs: active
                            ? p.default_agent_slugs.filter((s) => s !== ag.slug)
                            : [...p.default_agent_slugs, ag.slug],
                        }))
                      }
                    >
                      {ag.emoji} {ag.display_name}
                    </button>
                  )
                })}
              </div>
            </div>

            <label className="toggle-line">
              <input
                type="checkbox"
                checked={wsDraft.is_default}
                onChange={(e) => setWsDraft((p) => ({ ...p, is_default: e.target.checked }))}
              />
              <span>{c.workspaceIsDefaultLabel}</span>
            </label>

            {wsError && <p className="form-error">{wsError}</p>}
            {wsStatus && <p className="status-ok">{wsStatus}</p>}

            <div className="btn-row">
              <button
                type="button"
                className="btn primary"
                disabled={wsBusy}
                onClick={() => void handleSaveWorkspaceForm()}
              >
                {c.workspaceSaveBtn}
              </button>
              {wsDraft.id && (
                <button
                  type="button"
                  className="btn secondary"
                  disabled={wsBusy}
                  onClick={() => void handleLaunchSalonWithWorkspace(wsDraft)}
                >
                  💬 {c.workspaceLaunchSalonBtn}
                </button>
              )}
              {wsDraft.id && (
                <button
                  type="button"
                  className="btn ghost danger"
                  disabled={wsBusy}
                  onClick={() => {
                    if (!window.confirm(c.deleteWorkspaceConfirm)) return
                    void deleteWorkspace(wsDraft.id).then(() => setWsDraft(emptyWorkspaceDraft()))
                  }}
                >
                  {c.workspaceDeleteBtn}
                </button>
              )}
            </div>
          </section>
        </div>
      )}

      {studioSubTab === 'skills' && (
        <div className="skills-hub-shell">
          <div className="skills-hub-toolbar">
            <input
              type="search"
              value={skillSearch}
              placeholder={c.skillsSearchPlaceholder}
              onChange={(e) => setSkillSearch(e.target.value)}
            />
            <div className="chip-row">
              <button
                type="button"
                className={skillGroupFilter === 'all' ? 'chip active' : 'chip'}
                onClick={() => setSkillGroupFilter('all')}
              >
                {c.skillsAllGroups} ({skills.length})
              </button>
              {skillGroups.map((grp) => (
                <button
                  key={grp.slug}
                  type="button"
                  className={skillGroupFilter === grp.slug ? 'chip active' : 'chip'}
                  onClick={() =>
                    setSkillGroupFilter((prev) => (prev === grp.slug ? 'all' : grp.slug))
                  }
                >
                  {grp.icon || '📦'} {skillGroupDisplayName(grp)} ({grp.granted_count}/{grp.total_count})
                </button>
              ))}
            </div>
            <div className="btn-row">
              <button
                type="button"
                className="btn secondary"
                disabled={syncingSkills}
                onClick={() => void syncSkills()}
              >
                {syncingSkills ? c.skillsSyncing : c.skillsSyncBtn}
              </button>
              <button
                type="button"
                className="btn primary"
                onClick={() => setAccessModalOpen(true)}
              >
                🔐 {c.skillsRequestAccessBtn}
              </button>
            </div>
          </div>

          {accessNotice && <p className="status-ok">{accessNotice}</p>}

          {(capabilityBundles.length > 0 ||
            mcpGroups.length > 0 ||
            allowedModelAliases.length > 0 ||
            channelGrants.length > 0) && (
            <section className="skill-group-packs-section" aria-label={c.skillsBundlesTitle}>
              {capabilityBundles.length > 0 && (
                <>
                  <h4 className="skill-packs-heading">{c.skillsBundlesTitle}</h4>
                  <div className="skill-group-packs-grid">
                    {capabilityBundles.map((bundle) => {
                      const bLabel = bundle.display_name || bundle.name || bundle.slug
                      return (
                        <article key={bundle.slug} className="skill-pack-card">
                          <div className="skill-pack-head">
                            <div className="skill-pack-title">
                              <span className="skill-pack-icon" aria-hidden="true">
                                {bundle.icon || '🎁'}
                              </span>
                              <div>
                                <strong>{bLabel}</strong>
                                <span className="skill-pack-cat">
                                  {bundle.skill_group_slugs.length} skills · {bundle.mcp_group_slugs.length} MCP
                                </span>
                              </div>
                            </div>
                            <span className="scope-badge">{bundle.allowed_model_aliases.length} models</span>
                          </div>
                          {bundle.description && <p className="skill-pack-desc">{bundle.description}</p>}
                          <div className="skill-pack-foot">
                            <code>{bundle.slug}</code>
                            {bundle.granted ? (
                              <span className="tier-badge tier-read status-approved">
                                {c.skillsBundleGrantedBadge}
                              </span>
                            ) : (
                              <button
                                type="button"
                                className="btn small secondary"
                                onClick={() => openAccessModalFor('capability_bundle', bundle.slug, bLabel)}
                              >
                                🔐 {c.skillsBundleRequestBtn}
                              </button>
                            )}
                          </div>
                        </article>
                      )
                    })}
                  </div>
                </>
              )}

              {mcpGroups.length > 0 && (
                <>
                  <h4 className="skill-packs-heading">{c.skillsMcpGroupsTitle}</h4>
                  <div className="skill-group-packs-grid">
                    {mcpGroups.map((mg) => {
                      const mLabel = mg.display_name || mg.name || mg.slug
                      return (
                        <article key={mg.slug} className="skill-pack-card">
                          <div className="skill-pack-head">
                            <div className="skill-pack-title">
                              <span className="skill-pack-icon" aria-hidden="true">
                                {mg.icon || '🧩'}
                              </span>
                              <div>
                                <strong>{mLabel}</strong>
                                {mg.category && <span className="skill-pack-cat">{mg.category}</span>}
                              </div>
                            </div>
                            <span className="scope-badge">
                              {mg.item_count || mg.patterns.length} tools
                            </span>
                          </div>
                          {mg.description && <p className="skill-pack-desc">{mg.description}</p>}
                          <div className="skill-pack-foot">
                            <code>{mg.patterns.slice(0, 2).join(', ') || mg.slug}</code>
                            {mg.granted ? (
                              <span className="tier-badge tier-read status-approved">
                                {c.skillsMcpGrantedBadge}
                              </span>
                            ) : (
                              <button
                                type="button"
                                className="btn small secondary"
                                onClick={() => openAccessModalFor('mcp_group', mg.slug, mLabel)}
                              >
                                🔐 {c.skillsMcpGroupRequestBtn}
                              </button>
                            )}
                          </div>
                        </article>
                      )
                    })}
                  </div>
                </>
              )}

              {(allowedModelAliases.length > 0 || channelGrants.length > 0) && (
                <div className="chip-row" style={{ marginTop: 10 }}>
                  {allowedModelAliases.length > 0 && (
                    <>
                      <span className="muted-sub">{c.skillsModelsGrantedTitle}:</span>
                      {allowedModelAliases.map((m) => (
                        <span key={m} className="chip active">
                          ✨ {m}
                        </span>
                      ))}
                    </>
                  )}
                  {channelGrants.length > 0 && (
                    <>
                      <span className="muted-sub">{c.skillsChannelsGrantedTitle}:</span>
                      {channelGrants.map((ch) => (
                        <span key={ch} className="chip active">
                          📡 {ch}
                        </span>
                      ))}
                    </>
                  )}
                </div>
              )}
            </section>
          )}

          {skillGroups.length > 0 && (
            <section className="skill-group-packs-section" aria-label={c.skillsGroupPacksTitle}>
              <h4 className="skill-packs-heading">{c.skillsGroupPacksTitle}</h4>
              <div className="skill-group-packs-grid">
                {skillGroups.map((grp) => {
                  const pct =
                    grp.total_count > 0
                      ? Math.min(100, Math.round((grp.granted_count / grp.total_count) * 100))
                      : 0
                  const grpLabel = skillGroupDisplayName(grp)
                  const isFiltered = skillGroupFilter === grp.slug
                  return (
                    <article
                      key={grp.slug}
                      className={isFiltered ? 'skill-pack-card active' : 'skill-pack-card'}
                      onClick={() =>
                        setSkillGroupFilter((prev) => (prev === grp.slug ? 'all' : grp.slug))
                      }
                    >
                      <div className="skill-pack-head">
                        <div className="skill-pack-title">
                          <span className="skill-pack-icon" aria-hidden="true">
                            {grp.icon || '📦'}
                          </span>
                          <div>
                            <strong>{grpLabel}</strong>
                            {grp.category && <span className="skill-pack-cat">{grp.category}</span>}
                          </div>
                        </div>
                        <span className="scope-badge">
                          {grp.granted_count}/{grp.total_count}
                        </span>
                      </div>
                      {grp.description && <p className="skill-pack-desc">{grp.description}</p>}
                      <div className="skill-progress-track" aria-hidden="true">
                        <div className="skill-progress-fill" style={{ width: `${pct}%` }} />
                      </div>
                      <div className="skill-pack-foot">
                        <code>{grp.slug}</code>
                        {grp.granted ? (
                          <span className="tier-badge tier-read status-approved">
                            {c.skillsGroupGrantedBadge}
                          </span>
                        ) : (
                          <button
                            type="button"
                            className="btn small secondary"
                            onClick={(e) => {
                              e.stopPropagation()
                              void handleRequestGroupPack(grp.slug, grpLabel)
                            }}
                          >
                            📦 {c.skillsGroupRequestBtn}
                          </button>
                        )}
                      </div>
                    </article>
                  )
                })}
              </div>
            </section>
          )}

          <div className="skills-hub-grid">
            <div className="skills-card-list">
              {filteredSkills.length === 0 && <p className="empty-hint">{c.skillsEmpty}</p>}
              {filteredSkills.map((sk) => (
                <article
                  key={sk.slug}
                  className={
                    selectedSkill?.slug === sk.slug
                      ? 'skill-card active'
                      : sk.granted
                        ? 'skill-card'
                        : 'skill-card locked'
                  }
                  onClick={() => setSelectedSkillSlug(sk.slug)}
                >
                  <div className="skill-card-head">
                    <strong>
                      {sk.granted ? '🧩' : '🔒'} {skillDisplayName(sk)}
                    </strong>
                    <span className={sk.granted ? 'tier-badge tier-read' : 'tier-badge tier-write_gated'}>
                      {sk.granted ? c.skillsGrantedBadge : c.skillsLockedBadge}
                    </span>
                  </div>
                  <p>{sk.description || (!sk.granted ? sk.body_preview : '')}</p>
                  <div className="skill-card-foot">
                    <code>{sk.slug}</code>
                    {sk.granted ? (
                      <button
                        type="button"
                        className={sk.enabled ? 'btn small primary' : 'btn small secondary'}
                        onClick={(e) => {
                          e.stopPropagation()
                          void toggleSkillEnabled(sk.slug, !sk.enabled)
                        }}
                      >
                        {sk.enabled ? c.skillsEnableToggle : c.skillsDisableToggle}
                      </button>
                    ) : (
                      <button
                        type="button"
                        className="btn small secondary"
                        onClick={(e) => {
                          e.stopPropagation()
                          setAccessKind('skill')
                          setAccessSlug(sk.slug)
                          setAccessModalOpen(true)
                        }}
                      >
                        {c.skillsRequestAccessBtn}
                      </button>
                    )}
                  </div>
                </article>
              ))}
            </div>

            <aside className="skill-playbook-viewer">
              {selectedSkill ? (
                <>
                  <div className="skill-playbook-head">
                    <div>
                      <h3>
                        {selectedSkill.granted ? '🧩' : '🔒'} {skillDisplayName(selectedSkill)}
                      </h3>
                      <span className="muted-sub">
                        <code>/{selectedSkill.slug}</code> · v{selectedSkill.version} ·{' '}
                        {selectedSkill.source} · {selectedSkill.category}
                      </span>
                    </div>
                    {selectedSkill.granted && (
                      <div className="btn-row">
                        <button
                          type="button"
                          className={selectedSkill.enabled ? 'btn small primary' : 'btn small secondary'}
                          onClick={() =>
                            void toggleSkillEnabled(selectedSkill.slug, !selectedSkill.enabled)
                          }
                        >
                          {selectedSkill.enabled ? c.skillsEnableToggle : c.skillsDisableToggle}
                        </button>
                        <button
                          type="button"
                          className="btn small primary"
                          onClick={() => handleAttachSkillToSalon(selectedSkill.slug)}
                        >
                          💬 {c.skillsAttachToSalonBtn}
                        </button>
                      </div>
                    )}
                  </div>
                  {selectedSkill.granted ? (
                    <pre className="skill-playbook-body">
                      {stripSkillFrontmatter(selectedSkill.body_md) || selectedSkill.description}
                    </pre>
                  ) : (
                    <div className="skill-locked-preview">
                      <strong>🔒 {c.skillsLockedPreviewTitle}</strong>
                      <p>
                        {selectedSkill.body_preview?.trim() ||
                          selectedSkill.description ||
                          c.skillsLockedPreviewEmpty}
                      </p>
                      <div className="btn-row">
                        <button
                          type="button"
                          className="btn small primary"
                          onClick={() => {
                            setAccessKind('skill')
                            setAccessSlug(selectedSkill.slug)
                            setAccessModalOpen(true)
                          }}
                        >
                          🔓 {c.skillsRequestSkillBtn}
                        </button>
                        {selectedSkill.group_slugs[0] && (
                          <button
                            type="button"
                            className="btn small secondary"
                            onClick={() => {
                              setAccessKind('skill_group')
                              setAccessSlug(selectedSkill.group_slugs[0])
                              setAccessModalOpen(true)
                            }}
                          >
                            📦 {c.skillsRequestByGroupBtn}
                          </button>
                        )}
                        <button
                          type="button"
                          className="btn small ghost"
                          onClick={() => {
                            if (onOpenRegions) {
                              onOpenRegions()
                            } else {
                              window.location.hash = 'regions'
                            }
                          }}
                        >
                          ⌖ {c.skillsInspectTaxonomyBtn}
                        </button>
                      </div>
                    </div>
                  )}
                </>
              ) : (
                <p className="empty-hint">{c.skillsPlaybookPlaceholder}</p>
              )}

              <div className="access-requests-box">
                <h4>{c.skillsRequestsHistoryTitle}</h4>
                {accessRequests.length === 0 ? (
                  <p className="empty-hint">{c.skillsNoRequests}</p>
                ) : (
                  <div className="access-request-list">
                    {accessRequests.map((req) => {
                      const normStatus = req.status.toLowerCase()
                      const isApproved = normStatus === 'approved'
                      const isRejected = normStatus === 'rejected'
                      const isPending = normStatus === 'pending' || normStatus === 'submitted'
                      const badgeCls = isApproved
                        ? 'tier-badge status-approved'
                        : isRejected
                          ? 'tier-badge status-rejected'
                          : isPending
                            ? 'tier-badge status-pending'
                            : 'tier-badge status-local'
                      const badgeText = isApproved
                        ? `✓ ${c.skillsReqStatusApproved}`
                        : isRejected
                          ? `✕ ${c.skillsReqStatusRejected}`
                          : isPending
                            ? `⏳ ${c.skillsReqStatusPending}`
                            : `💾 ${c.skillsReqStatusLocal}`
                      return (
                        <div key={req.id} className="access-request-row">
                          <div className="access-request-info">
                            <strong>
                              {formatTargetKindTag(req.target_kind)}:{' '}
                              {req.target_slug}
                            </strong>
                            <p>{req.reason || '—'}</p>
                            {isApproved && (req.reviewed_by || req.review_note) && (
                              <small className="access-request-meta">
                                {req.reviewed_by ? `${c.skillsReqReviewedBy}: ${req.reviewed_by}` : ''}
                                {req.reviewed_by && req.review_note ? ' · ' : ''}
                                {req.review_note || ''}
                              </small>
                            )}
                            {isPending && req.approval_request_id && (
                              <small className="access-request-meta">
                                {c.skillsReqApprovalId}: <code>{req.approval_request_id.slice(0, 12)}</code>
                              </small>
                            )}
                            {isRejected && (req.review_note || req.reviewed_by) && (
                              <small className="access-request-meta error-note">
                                {req.reviewed_by ? `${req.reviewed_by}: ` : ''}
                                {req.review_note || ''}
                              </small>
                            )}
                          </div>
                          <span className={badgeCls} title={req.status}>
                            {badgeText}
                          </span>
                        </div>
                      )
                    })}
                  </div>
                )}
              </div>
            </aside>
          </div>

          {accessModalOpen && (
            <div className="modal-backdrop" role="dialog" aria-label={c.skillsRequestModalTitle}>
              <div className="modal-card">
                <h3>{c.skillsRequestModalTitle}</h3>
                <div className="field">
                  <span>{c.skillsRequestKindLabel}</span>
                  <AgentSelect
                    ariaLabel={c.skillsRequestKindLabel}
                    value={accessKind}
                    onChange={(v) => setAccessKind(v as AccessRequestTargetKind)}
                    options={[
                      { value: 'capability_bundle', label: c.skillsRequestKindBundle },
                      { value: 'mcp_group', label: c.skillsRequestKindMcpGroup },
                      { value: 'skill_group', label: c.skillsRequestKindGroup },
                      { value: 'skill', label: c.skillsRequestKindSkill },
                      { value: 'model_alias', label: c.skillsRequestKindModel },
                      { value: 'channel', label: c.skillsRequestKindChannel },
                    ]}
                  />
                </div>
                <label className="field">
                  <span>{c.skillsRequestTargetLabel}</span>
                  <input value={accessSlug} onChange={(e) => setAccessSlug(e.target.value)} />
                </label>
                <label className="field">
                  <span>{c.skillsRequestReasonLabel}</span>
                  <textarea
                    rows={3}
                    value={accessReason}
                    onChange={(e) => setAccessReason(e.target.value)}
                  />
                </label>
                <div className="btn-row">
                  <button
                    type="button"
                    className="btn primary"
                    disabled={!accessSlug.trim()}
                    onClick={() => {
                      void requestSkillAccess({
                        target_kind: accessKind,
                        target_slug: accessSlug.trim(),
                        reason: accessReason.trim(),
                      }).then((created) => {
                        setAccessModalOpen(false)
                        setAccessReason('')
                        if (created) {
                          setAccessNotice(`${created.target_slug}: ${created.status}`)
                        }
                      })
                    }}
                  >
                    {c.skillsRequestSubmitBtn}
                  </button>
                  <button
                    type="button"
                    className="btn secondary"
                    onClick={() => setAccessModalOpen(false)}
                  >
                    {c.cancelBtn}
                  </button>
                </div>
              </div>
            </div>
          )}
        </div>
      )}
        </>
      )}

      {surface === 'cowriter' && (
        <div className="ak-cowriter-grid">
          <section className="studio-section">
            <div className="ak-toolbar">
              <h3>{c.v2CowriterTitle}</h3>
              <button
                type="button"
                className="btn secondary btn-sm"
                onClick={() => {
                  if (!cwBefore.trim()) {
                    setCwTitle(uiLang === 'vi' ? 'Điểm tin Chính sách Tiền tệ & Thuế' : 'Macro & Tax Policy Brief')
                    setCwBefore(
                      uiLang === 'vi'
                        ? 'Lãi suất điều hành giữ ổn định trong quý. Cần kiểm tra mốc kê khai thuế TNDN quý và nguồn trích dẫn chính thức.'
                        : 'Policy rates held steady this quarter. Verify quarterly CIT filing dates and attach primary source links.',
                    )
                    setCwInstruction(
                      uiLang === 'vi'
                        ? 'Chỉnh văn phong sắc nét hơn, giữ nguồn trích dẫn và thêm lưu ý miễn trừ thuế.'
                        : 'Tighten editorial clarity, preserve source attribution, and keep tax disclaimer.',
                    )
                  }
                }}
              >
                ✦ Sample
              </button>
            </div>
            <div className="catalog-form-row">
              <label className="field">
                <span>{c.threadTitleInput}</span>
                <input
                  value={cwTitle}
                  placeholder={uiLang === 'vi' ? 'Tiêu đề bản thảo…' : 'Draft title…'}
                  onChange={(e) => setCwTitle(e.target.value)}
                />
              </label>
              <label className="field">
                <span>Article ID</span>
                <input
                  value={cwArticleId}
                  placeholder="123"
                  onChange={(e) => setCwArticleId(e.target.value.replace(/[^0-9]/g, ''))}
                />
              </label>
              <div className="field">
                <span>{c.studioRouteAgentLabel}</span>
                <AgentSelect
                  ariaLabel={c.studioRouteAgentLabel}
                  value={cwAgentSlug}
                  onChange={setCwAgentSlug}
                  options={agents.map((a) => ({
                    value: a.slug,
                    label: `${a.emoji} ${a.display_name}`,
                    badge: `@${a.slug}`,
                  }))}
                />
              </div>
            </div>
            <label className="field">
              <span>{uiLang === 'vi' ? 'Nội dung gốc' : 'Draft Content'}</span>
              <textarea
                rows={9}
                value={cwBefore}
                placeholder={
                  uiLang === 'vi'
                    ? 'Dán đoạn bài báo hoặc báo cáo cần biên tập…'
                    : 'Paste article excerpt or newsroom brief to revise…'
                }
                onChange={(e) => setCwBefore(e.target.value)}
              />
            </label>
            <label className="field">
              <span>{c.v2CowriterInstructionPlaceholder}</span>
              <input
                value={cwInstruction}
                placeholder={c.v2CowriterInstructionPlaceholder}
                onChange={(e) => setCwInstruction(e.target.value)}
              />
            </label>
            {cwNotice && <p className="status-ok">{cwNotice}</p>}
            <div className="btn-row">
              <button
                type="button"
                className="btn primary"
                disabled={cwBusy || !cwBefore.trim() || !cwInstruction.trim()}
                onClick={() => {
                  setCwBusy(true)
                  setCwNotice('')
                  void suggestCowriterDiff({
                    thread_id: activeThread ? String(activeThread.id) : null,
                    article_id: cwArticleId ? Number(cwArticleId) : null,
                    title: cwTitle.trim() || 'Newsroom Draft',
                    before_text: cwBefore,
                    instruction: cwInstruction,
                    agent_slug: cwAgentSlug,
                  })
                    .then((draft) => {
                      if (draft) {
                        setCwActiveDraft(draft)
                        setCwHistory((prev) => [draft, ...prev.filter((d) => d.id !== draft.id)])
                      }
                    })
                    .catch((err) => setCwNotice(err instanceof Error ? err.message : 'Suggest failed'))
                    .finally(() => setCwBusy(false))
                }}
              >
                {c.v2CowriterSuggestBtn}
              </button>
            </div>
          </section>

          <section className="studio-section">
            <div className="ak-toolbar">
              <h4>{cwActiveDraft ? cwActiveDraft.title : c.v2SurfaceCowriter}</h4>
              {cwActiveDraft && <span className="tier-badge">{cwActiveDraft.status}</span>}
            </div>
            {!cwActiveDraft ? (
              <p className="empty-hint">
                {uiLang === 'vi'
                  ? 'Nhập văn bản và chỉ dẫn biên tập rồi bấm Đề xuất sửa.'
                  : 'Enter source text and instruction, then click Suggest Diff.'}
              </p>
            ) : (
              <>
                {cwActiveDraft.rationale && <p className="muted-sub">{cwActiveDraft.rationale}</p>}
                <div className="ak-diff-grid">
                  <div>
                    <small className="muted-sub">Before</small>
                    <div className="ak-diff-pane">{cwActiveDraft.before_text}</div>
                  </div>
                  <div>
                    <small className="muted-sub">After</small>
                    <div className="ak-diff-pane after">{cwActiveDraft.after_text}</div>
                  </div>
                </div>
                {cwActiveDraft.status === 'pending' && (
                  <div className="btn-row" style={{ marginTop: 10 }}>
                    <button
                      type="button"
                      className="btn primary"
                      onClick={() => {
                        void decideCowriterDraft(cwActiveDraft.id, 'accept').then((updated) => {
                          if (updated) {
                            setCwBefore(updated.after_text)
                            setCwActiveDraft(updated)
                            setCwHistory((prev) =>
                              prev.map((d) => (d.id === updated.id ? updated : d)),
                            )
                            setCwNotice(uiLang === 'vi' ? 'Đã áp dụng bản sửa.' : 'Diff accepted.')
                          }
                        })
                      }}
                    >
                      ✓ {c.v2CowriterAcceptBtn}
                    </button>
                    <button
                      type="button"
                      className="btn secondary"
                      onClick={() => {
                        void decideCowriterDraft(cwActiveDraft.id, 'reject').then((updated) => {
                          if (updated) {
                            setCwActiveDraft(updated)
                            setCwHistory((prev) =>
                              prev.map((d) => (d.id === updated.id ? updated : d)),
                            )
                            setCwNotice(uiLang === 'vi' ? 'Đã từ chối bản sửa.' : 'Diff rejected.')
                          }
                        })
                      }}
                    >
                      ✕ {c.v2CowriterRejectBtn}
                    </button>
                  </div>
                )}
              </>
            )}

            {cwHistory.length > 0 && (
              <div style={{ marginTop: 14 }}>
                <small className="muted-sub">History ({cwHistory.length})</small>
                <div className="chip-row" style={{ marginTop: 6 }}>
                  {cwHistory.slice(0, 6).map((item) => (
                    <button
                      key={item.id}
                      type="button"
                      className={cwActiveDraft?.id === item.id ? 'chip active' : 'chip'}
                      onClick={() => setCwActiveDraft(item)}
                    >
                      {item.title} · {item.status}
                    </button>
                  ))}
                </div>
              </div>
            )}
          </section>
        </div>
      )}

      {surface === 'memory' && (
        <div className="models-connections-shell">
          <div className="ak-toolbar">
            <div className="ak-btn-group">
              <span className="chip">
                {safeMemoryGraph.stats.total_nodes} nodes · {safeMemoryGraph.stats.total_edges} edges
              </span>
              <span className="chip">
                L1: {safeMemoryGraph.stats.l1_count} · L2: {safeMemoryGraph.stats.l2_count} · L3:{' '}
                {safeMemoryGraph.stats.l3_count}
              </span>
            </div>
            <div className="ak-btn-group">
              <button
                type="button"
                className="btn secondary btn-sm"
                disabled={memBusy}
                onClick={() => {
                  setMemBusy(true)
                  void auditMemory()
                    .then((res) =>
                      setMemNotice(
                        res.ai_summary ||
                          `Checked ${res.total_checked ?? 0} nodes (${res.findings?.length ?? 0} findings).`,
                      ),
                    )
                    .finally(() => setMemBusy(false))
                }}
              >
                {c.v2MemoryAuditBtn}
              </button>
              <button
                type="button"
                className="btn secondary btn-sm"
                disabled={memBusy}
                onClick={() => {
                  setMemBusy(true)
                  void dedupMemory()
                    .then((res) =>
                      setMemNotice(
                        `Removed ${res.removed_count ?? 0} duplicates (${res.remaining_count ?? 0} remaining).`,
                      ),
                    )
                    .finally(() => setMemBusy(false))
                }}
              >
                {c.v2MemoryDedupBtn}
              </button>
              <button
                type="button"
                className="btn secondary btn-sm"
                onClick={() => handleExportMemory('json')}
              >
                {c.v2MemoryExportJson}
              </button>
              <button
                type="button"
                className="btn secondary btn-sm"
                onClick={() => handleExportMemory('md')}
              >
                {c.v2MemoryExportMd}
              </button>
            </div>
          </div>

          {memNotice && <p className="status-ok">{memNotice}</p>}

          <div className="ak-concentric-rings">
            <section className="ak-ring-col">
              <strong>{c.v2MemoryLayerL1}</strong>
              <p className="muted-sub">
                {activeThread
                  ? activeThread.scratchpad_md || safeMemoryGraph.l1_scratchpad || '—'
                  : safeMemoryGraph.l1_scratchpad || '—'}
              </p>
              {safeMemoryGraph.nodes
                .filter((n: MemoryGraphNode) => n.layer === 'L1')
                .map((node: MemoryGraphNode) => (
                  <div key={node.id} className="ak-memory-card">
                    <strong>{node.title}</strong>
                    <p>{node.content}</p>
                  </div>
                ))}
            </section>

            {(['L2', 'L3'] as const).map((layer) => (
              <section key={layer} className="ak-ring-col">
                <strong>{layer === 'L2' ? c.v2MemoryLayerL2 : c.v2MemoryLayerL3}</strong>
                {safeMemoryGraph.nodes
                  .filter((n: MemoryGraphNode) => n.layer === layer)
                  .map((node: MemoryGraphNode) => (
                    <div key={node.id} className="ak-memory-card">
                      <div className="ak-toolbar">
                        <strong>{node.title}</strong>
                        <span className="ak-origin-pill">{node.node_kind}</span>
                      </div>
                      {memEditingId === node.id ? (
                        <>
                          <textarea
                            rows={3}
                            value={memEditContent}
                            onChange={(e) => setMemEditContent(e.target.value)}
                          />
                          <div className="ak-btn-group">
                            <button
                              type="button"
                              className="btn primary btn-sm"
                              onClick={() => {
                                void updateMemoryNode({ id: node.id, content: memEditContent }).then(
                                  () => setMemEditingId(null),
                                )
                              }}
                            >
                              ✓
                            </button>
                            <button
                              type="button"
                              className="btn secondary btn-sm"
                              onClick={() => setMemEditingId(null)}
                            >
                              ✕
                            </button>
                          </div>
                        </>
                      ) : (
                        <>
                          <p>{node.content}</p>
                          <div className="ak-toolbar">
                            <small className="muted-sub">{node.source_ref || 'local'}</small>
                            <div className="ak-btn-group">
                              <button
                                type="button"
                                className="btn secondary btn-sm"
                                onClick={() => {
                                  setMemEditingId(node.id)
                                  setMemEditContent(node.content)
                                }}
                              >
                                Edit
                              </button>
                              <button
                                type="button"
                                className="icon-del"
                                aria-label="Delete node"
                                onClick={() => void deleteMemoryNode(node.id)}
                              >
                                🗑
                              </button>
                            </div>
                          </div>
                        </>
                      )}
                    </div>
                  ))}
              </section>
            ))}
          </div>

          <section className="studio-section">
            <h4>{c.v2MemoryAddNodeBtn}</h4>
            <div className="catalog-form-row">
              <div className="field">
                <span>Layer</span>
                <AgentSelect
                  ariaLabel="Memory Layer"
                  value={memLayer}
                  onChange={(v) => setMemLayer(v as 'L2' | 'L3')}
                  options={[
                    { value: 'L2', label: c.v2MemoryLayerL2 },
                    { value: 'L3', label: c.v2MemoryLayerL3 },
                  ]}
                />
              </div>
              <label className="field">
                <span>Kind</span>
                <input value={memKind} onChange={(e) => setMemKind(e.target.value)} />
              </label>
              <label className="field">
                <span>Title</span>
                <input value={memTitle} onChange={(e) => setMemTitle(e.target.value)} />
              </label>
              <label className="field">
                <span>Source Ref</span>
                <input value={memSourceRef} onChange={(e) => setMemSourceRef(e.target.value)} />
              </label>
            </div>
            <label className="field">
              <span>Content</span>
              <textarea
                rows={2}
                value={memContent}
                onChange={(e) => setMemContent(e.target.value)}
              />
            </label>
            <div className="btn-row">
              <button
                type="button"
                className="btn primary"
                disabled={memBusy || !memTitle.trim() || !memContent.trim()}
                onClick={() => {
                  setMemBusy(true)
                  void saveMemoryNode({
                    layer: memLayer,
                    node_kind: memKind.trim() || 'canon_rule',
                    title: memTitle.trim(),
                    content: memContent.trim(),
                    source_ref: memSourceRef.trim(),
                  })
                    .then(() => {
                      setMemTitle('')
                      setMemContent('')
                      setMemSourceRef('')
                    })
                    .finally(() => setMemBusy(false))
                }}
              >
                + {c.v2MemoryAddNodeBtn}
              </button>
            </div>
          </section>
        </div>
      )}

      {surface === 'settings' && (
        <div className="ak-split-wrap">
          <AgentSplitLayout
            storageKey="newsman:settings"
            leftDefaultWidth={220}
            leftMinWidth={185}
            leftMaxWidth={320}
            rightCollapsed
            left={
              <nav className="ak-settings-nav" aria-label={c.v2SurfaceSettings}>
                {visibleSettingsTabs.map((tabId) => {
                  const meta = settingsTabMeta[tabId]
                  return (
                    <button
                      key={tabId}
                      type="button"
                      className={`ak-settings-nav-btn ${settingsTab === tabId ? 'active' : ''}`}
                      onClick={() => setSettingsTab(tabId)}
                    >
                      <span>
                        {meta.icon} {meta.label}
                      </span>
                    </button>
                  )
                })}
              </nav>
            }
            center={
              <div className="ak-settings-body">
                {connError && <p className="form-error">{connError}</p>}
                {connNotice && <p className="status-ok">{connNotice}</p>}

                {settingsTab === 'connections' && (
                  <>
                    <section className="ak-hero-card">
                      <div className="ak-hero-head">
                        <div>
                          <h3>⚡ LNC Agents Hub</h3>
                          <p className="muted-sub" style={{ marginTop: 4 }}>
                            {c.connHubFirstSub}
                          </p>
                        </div>
                        <span className="tier-badge">{statusLabel}</span>
                      </div>

                      <div className="catalog-form-row">
                        <label className="field">
                          <span>{c.bridgeHubUrl}</span>
                          <input
                            value={connForm.hub_url}
                            placeholder="https://agents-hub.lengoc.me"
                            onChange={(e) => setConnForm((p) => ({ ...p, hub_url: e.target.value }))}
                          />
                        </label>
                        <label className="field">
                          <span>{c.bridgeHubToken}</span>
                          <input
                            type="password"
                            value={connForm.hub_token}
                            placeholder={bridgeStatus.has_hub_token ? '••••••••' : ''}
                            onChange={(e) => setConnForm((p) => ({ ...p, hub_token: e.target.value }))}
                          />
                        </label>
                      </div>

                      <div className="ak-unlock-grid">
                        <span className="muted-sub">
                          {uiLang === 'vi' ? 'Mở khóa qua Hub:' : 'Unlocked via Hub:'}
                        </span>
                        <span
                          className={
                            probeReport?.astraea?.ok || Boolean(bridgeStatus.hub_url)
                              ? 'chip active'
                              : 'chip'
                          }
                        >
                          Astraea
                        </span>
                        {(['search', 'cortex', 'obsidian', 'library', 'notebooklm', 'cygnus'] as const).map(
                          (ns) => {
                            const entry = mcpNamespaces.find((item) => item.namespace === ns)
                            const enabled = entry ? entry.enabled : Boolean(bridgeStatus.hub_url)
                            return (
                              <span key={ns} className={enabled ? 'chip active' : 'chip'}>
                                {ns}
                              </span>
                            )
                          },
                        )}
                      </div>

                      <div className="probe-bar">
                        <button
                          type="button"
                          className="btn primary"
                          onClick={() => saveBridgeConfigPayload(c.bridgeTokenSaved)}
                        >
                          {c.taskModelsSaveBtn}
                        </button>
                        <button
                          type="button"
                          className="btn secondary"
                          disabled={probingConnections}
                          onClick={() => void probeConnections()}
                        >
                          {probingConnections ? c.modelsProbing : `🔌 ${c.modelsProbeBtn}`}
                        </button>
                        <button
                          type="button"
                          className="btn secondary"
                          disabled={syncingSkills || probingConnections}
                          onClick={() => void Promise.all([syncSkills(), probeConnections()])}
                        >
                          {syncingSkills || probingConnections ? c.modelsProbing : c.connSyncAllBtn}
                        </button>
                        {probeReport && (
                          <span className="muted-sub">
                            {c.modelsProbeLastRun}:{' '}
                            {new Date(probeReport.probed_at).toLocaleTimeString()}
                          </span>
                        )}
                      </div>
                    </section>

                    <div className="probe-cards-grid">
                      {(
                        [
                          [c.modelsServiceHub, probeReport?.hub, Boolean(bridgeStatus.hub_url)],
                          [
                            c.modelsServiceAstraea,
                            probeReport?.astraea,
                            Boolean(bridgeStatus.astraea_url || bridgeStatus.hub_url),
                          ],
                          [
                            c.modelsServiceLlm,
                            probeReport?.llm,
                            Boolean(bridgeStatus.llm_base_url || bridgeStatus.hub_url),
                          ],
                        ] as const
                      ).map(([title, res, fallbackConfigured]) => {
                        const configured = res ? res.configured : fallbackConfigured
                        const statusText = !configured
                          ? c.modelsNotConfigured
                          : res?.ok
                            ? c.modelsConnectedOk
                            : res
                              ? c.modelsDisconnected
                              : statusLabel
                        return (
                          <div key={title} className={`probe-card ${res?.ok ? 'ok' : ''}`}>
                            <div className="probe-card-head">
                              <strong>{title}</strong>
                              <span className={res?.ok ? 'tier-badge tier-read' : 'tier-badge'}>
                                {statusText}
                              </span>
                            </div>
                            <small>{res?.url_host ?? '—'}</small>
                            {res?.detail && <p className="muted-sub">{res.detail}</p>}
                          </div>
                        )
                      })}
                    </div>

                    <section className="studio-section conn-diag-section">
                      <div className="probe-card-head">
                        <strong>{c.routeMatrixTitle}</strong>
                        <span className="tier-badge">
                          {formatRouteLabel(probeReport?.relay_route ?? bridgeStatus.relay_route)}
                        </span>
                      </div>
                      <div className="chip-row conn-diag-chips">
                        {(
                          [
                            [
                              c.routeLegAstraeaHttp,
                              probeReport?.routing?.astraea_http ?? bridgeStatus.routing?.astraea_http,
                            ],
                            [
                              c.routeLegAstraeaWs,
                              probeReport?.routing?.astraea_ws ?? bridgeStatus.routing?.astraea_ws,
                            ],
                            [
                              c.routeLegModels,
                              probeReport?.routing?.models ?? bridgeStatus.routing?.models,
                            ],
                            [
                              c.routeLegSkillsMcp,
                              probeReport?.routing?.skills_and_mcp ??
                                bridgeStatus.routing?.skills_and_mcp,
                            ],
                            [
                              c.routeLegTelegram,
                              telegramConfig.telegram_route ??
                                probeReport?.routing?.telegram ??
                                bridgeStatus.routing?.telegram,
                            ],
                          ] as const
                        ).map(([label, routeVal]) => (
                          <span
                            key={label}
                            className={
                              routeVal && routeVal !== 'none' && routeVal !== 'disabled'
                                ? 'chip active'
                                : 'chip'
                            }
                          >
                            {label}: <strong>{formatRouteLabel(routeVal)}</strong>
                          </span>
                        ))}
                      </div>
                    </section>

                    <details
                      className="studio-scope-box"
                      open={Boolean(
                        connForm.astraea_url || connForm.llm_base_url || customProviders.length > 0,
                      )}
                    >
                      <summary style={{ cursor: 'pointer', fontWeight: 600 }}>
                        {c.connDirectOverridesTitle}
                      </summary>
                      <p className="muted-sub" style={{ marginTop: 6 }}>
                        {c.connDirectOverridesHint}
                      </p>

                      <div className="catalog-form-row">
                        <label className="field">
                          <span>{c.bridgeAstraeaUrl}</span>
                          <input
                            value={connForm.astraea_url}
                            placeholder=""
                            onChange={(e) =>
                              setConnForm((p) => ({ ...p, astraea_url: e.target.value }))
                            }
                          />
                        </label>
                        <label className="field">
                          <span>{c.bridgeAstraeaToken}</span>
                          <input
                            type="password"
                            value={connForm.astraea_token}
                            placeholder={bridgeStatus.has_astraea_token ? '••••••••' : ''}
                            onChange={(e) =>
                              setConnForm((p) => ({ ...p, astraea_token: e.target.value }))
                            }
                          />
                        </label>
                      </div>
                      <div className="catalog-form-row">
                        <label className="field">
                          <span>{c.bridgeLlmBaseUrl}</span>
                          <input
                            value={connForm.llm_base_url}
                            placeholder=""
                            onChange={(e) =>
                              setConnForm((p) => ({ ...p, llm_base_url: e.target.value }))
                            }
                          />
                        </label>
                        <label className="field">
                          <span>{c.bridgeLlmApiKey}</span>
                          <input
                            type="password"
                            value={connForm.llm_api_key}
                            placeholder={bridgeStatus.has_llm_api_key ? '••••••••' : ''}
                            onChange={(e) =>
                              setConnForm((p) => ({ ...p, llm_api_key: e.target.value }))
                            }
                          />
                        </label>
                      </div>
                      <label className="field">
                        <span>{c.bridgeAllowedHosts}</span>
                        <input
                          value={connForm.allowed_hosts}
                          placeholder="agents-hub.lengoc.me, 127.0.0.1"
                          onChange={(e) =>
                            setConnForm((p) => ({ ...p, allowed_hosts: e.target.value }))
                          }
                        />
                      </label>
                      <div className="btn-row" style={{ marginTop: 8 }}>
                        <button
                          type="button"
                          className="btn secondary btn-sm"
                          onClick={() => saveBridgeConfigPayload(c.bridgeTokenSaved)}
                        >
                          {c.taskModelsSaveBtn}
                        </button>
                      </div>

                      <div className="studio-field-group" style={{ marginTop: 14 }}>
                        <h4>
                          {uiLang === 'vi'
                            ? 'Nhà cung cấp OpenAI tương thích'
                            : 'Custom OpenAI-Compatible Providers'}
                        </h4>
                        {customProviders.length > 0 && (
                          <div className="chat-sched-table" style={{ marginBottom: 10 }}>
                            {customProviders.map((prov) => (
                              <div key={prov.id} className="chat-sched-row">
                                <div className="chat-sched-main">
                                  <b>{prov.name}</b>
                                  <small className="muted">
                                    {prov.base_url} · {prov.has_secret ? '••••••••' : 'no key'}
                                  </small>
                                </div>
                                <button
                                  type="button"
                                  className="icon-del"
                                  aria-label={`Delete ${prov.name}`}
                                  onClick={() => void deleteCustomProvider(prov.id)}
                                >
                                  🗑
                                </button>
                              </div>
                            ))}
                          </div>
                        )}
                        <div className="catalog-form-row">
                          <label className="field">
                            <span>Provider Name</span>
                            <input
                              value={provName}
                              placeholder="Ollama / OpenRouter / vLLM"
                              onChange={(e) => setProvName(e.target.value)}
                            />
                          </label>
                          <label className="field">
                            <span>Base URL</span>
                            <input
                              value={provBaseUrl}
                              placeholder="https://api.example.com/v1"
                              onChange={(e) => setProvBaseUrl(e.target.value)}
                            />
                          </label>
                          <label className="field">
                            <span>API Key (write-only)</span>
                            <input
                              type="password"
                              value={provApiKey}
                              placeholder="sk-..."
                              onChange={(e) => setProvApiKey(e.target.value)}
                            />
                          </label>
                        </div>
                        <div className="btn-row">
                          <button
                            type="button"
                            className="btn secondary btn-sm"
                            disabled={provBusy || !provName.trim() || !provBaseUrl.trim()}
                            onClick={() => {
                              setProvBusy(true)
                              setConnError('')
                              void createCustomProvider({
                                name: provName.trim(),
                                base_url: provBaseUrl.trim(),
                                api_key: provApiKey.trim() || undefined,
                              })
                                .then(() => {
                                  setProvName('')
                                  setProvBaseUrl('')
                                  setProvApiKey('')
                                  setConnNotice('Custom provider saved.')
                                })
                                .catch((err) =>
                                  setConnError(err instanceof Error ? err.message : 'Save failed'),
                                )
                                .finally(() => setProvBusy(false))
                            }}
                          >
                            + {c.v2AddProviderBtn}
                          </button>
                        </div>
                      </div>
                    </details>
                  </>
                )}

                {settingsTab === 'models' && (
                  <>
                    <section className="studio-section">
                      <div className="ak-toolbar">
                        <div>
                          <h4>
                            {uiLang === 'vi'
                              ? '2 Vai trò Mô hình Toàn cục'
                              : '2 Global Model Roles'}
                          </h4>
                          <small className="muted-sub">
                            Smart: <code>{roleSummary.smartModel}</code> · Fast:{' '}
                            <code>{roleSummary.fastModel}</code>
                          </small>
                        </div>
                        <button
                          type="button"
                          className="btn secondary btn-sm"
                          onClick={() => {
                            setConnError('')
                            void fetchModelCatalog()
                              .then((res) =>
                                setConnNotice(
                                  `Fetched ${res.fetched_count ?? 0} models${res.auto_enabled ? ' (auto-enabled)' : ''}.`,
                                ),
                              )
                              .catch((err) =>
                                setConnError(
                                  err instanceof Error ? err.message : 'Fetch catalog failed',
                                ),
                              )
                          }}
                        >
                          ⇣ {c.v2FetchCatalogBtn}
                        </button>
                      </div>

                      <div className="catalog-form-row" style={{ marginTop: 10 }}>
                        <div className="field">
                          <span>{c.v2SmartModelLabel}</span>
                          <AgentSelect
                            ariaLabel={c.v2SmartModelLabel}
                            value={connForm.default_smart_model}
                            onChange={(v) =>
                              setConnForm((p) => ({
                                ...p,
                                default_smart_model: v,
                                default_model: v,
                              }))
                            }
                            options={selectableModels.map((m) => ({ value: m, label: m }))}
                          />
                        </div>
                        <div className="field">
                          <span>{c.v2FastModelLabel}</span>
                          <AgentSelect
                            ariaLabel={c.v2FastModelLabel}
                            value={connForm.fast_background_model}
                            onChange={(v) =>
                              setConnForm((p) => ({ ...p, fast_background_model: v }))
                            }
                            options={[
                              { value: '', label: c.v2FastFollowSmart },
                              ...selectableModels.map((m) => ({ value: m, label: m })),
                            ]}
                          />
                        </div>
                      </div>
                      <div className="btn-row" style={{ marginTop: 10 }}>
                        <button
                          type="button"
                          className="btn primary btn-sm"
                          onClick={() => {
                            setConnError('')
                            void saveModelRoles({
                              default_smart_model: connForm.default_smart_model,
                              fast_background_model: connForm.fast_background_model,
                            })
                              .then(() => setConnNotice('Model roles saved.'))
                              .catch((err) =>
                                setConnError(err instanceof Error ? err.message : 'Save failed'),
                              )
                          }}
                        >
                          {c.v2SaveRolesBtn}
                        </button>
                      </div>
                    </section>

                    <section className="studio-section">
                      <div className="ak-toolbar">
                        <h4>
                          {c.modelsCatalogTitle} ({enabledCatalogModels.length}/
                          {modelCatalog.length > 0 ? modelCatalog.length : selectableModels.length})
                        </h4>
                      </div>
                      {modelCatalog.length > 0 ? (
                        <div className="chip-row" style={{ marginTop: 10 }}>
                          {modelCatalog.map((item) => (
                            <button
                              key={item.model_id}
                              type="button"
                              className={item.enabled ? 'chip active' : 'chip'}
                              onClick={() =>
                                void toggleCatalogModels([
                                  { model_id: item.model_id, enabled: !item.enabled },
                                ])
                              }
                            >
                              {item.enabled ? '✓ ' : ''}
                              {item.model_id}
                            </button>
                          ))}
                        </div>
                      ) : (
                        <div className="chip-row" style={{ marginTop: 10 }}>
                          {selectableModels.map((m) => (
                            <span key={m} className="chip">
                              {m}
                            </span>
                          ))}
                        </div>
                      )}
                    </section>
                  </>
                )}

                {settingsTab === 'mcp' && (
                  <>
                    <section className="studio-section">
                      <div className="ak-toolbar">
                        <h4>Business MCP Namespaces</h4>
                        <button
                          type="button"
                          className="btn primary btn-sm"
                          onClick={() => {
                            const updates = mcpNamespaces.map((ns) => ({
                              namespace: ns.namespace,
                              enabled: Boolean(mcpDraft[ns.namespace]),
                            }))
                            void saveMcpNamespaces(updates).then(() =>
                              setConnNotice('MCP namespaces updated.'),
                            )
                          }}
                        >
                          {c.v2SavePrefsBtn}
                        </button>
                      </div>
                      <div className="chip-row" style={{ marginTop: 10 }}>
                        {mcpNamespaces.map((ns) => (
                          <button
                            key={ns.namespace}
                            type="button"
                            className={mcpDraft[ns.namespace] ? 'chip active' : 'chip'}
                            onClick={() =>
                              setMcpDraft((prev) => ({
                                ...prev,
                                [ns.namespace]: !prev[ns.namespace],
                              }))
                            }
                          >
                            {mcpDraft[ns.namespace] ? '✓ ' : ''}
                            {ns.namespace}
                          </button>
                        ))}
                      </div>
                    </section>

                    <section className="studio-section">
                      <h4>
                        {uiLang === 'vi' ? 'Giới hạn an toàn thực thi' : 'Runtime Guardrails'}
                      </h4>
                      <div className="catalog-form-row" style={{ marginTop: 8 }}>
                        <label className="field">
                          <span>Max Tool Rounds</span>
                          <input
                            type="number"
                            min={1}
                            max={20}
                            value={connForm.max_tool_rounds}
                            onChange={(e) =>
                              setConnForm((p) => ({
                                ...p,
                                max_tool_rounds: Number(e.target.value) || 8,
                              }))
                            }
                          />
                        </label>
                        <label className="field">
                          <span>Max Group Hops</span>
                          <input
                            type="number"
                            min={1}
                            max={10}
                            value={connForm.max_group_hops}
                            onChange={(e) =>
                              setConnForm((p) => ({
                                ...p,
                                max_group_hops: Number(e.target.value) || 3,
                              }))
                            }
                          />
                        </label>
                        <label className="field">
                          <span>Default Temperature</span>
                          <input
                            type="number"
                            step={0.1}
                            min={0}
                            max={2}
                            value={connForm.default_temperature}
                            onChange={(e) =>
                              setConnForm((p) => ({
                                ...p,
                                default_temperature: Number(e.target.value) || 0.4,
                              }))
                            }
                          />
                        </label>
                      </div>
                      <div className="btn-row" style={{ marginTop: 10 }}>
                        <button
                          type="button"
                          className="btn secondary btn-sm"
                          onClick={() => saveBridgeConfigPayload(c.bridgeTokenSaved)}
                        >
                          {c.taskModelsSaveBtn}
                        </button>
                      </div>
                    </section>
                  </>
                )}

                {settingsTab === 'memory' && (
                  <section className="studio-section">
                    <div className="ak-toolbar">
                      <h4>{c.v2SurfaceMemory}</h4>
                      <div className="ak-btn-group">
                        <button
                          type="button"
                          className="btn secondary btn-sm"
                          onClick={() => handleExportMemory('json')}
                        >
                          {c.v2MemoryExportJson}
                        </button>
                        <button
                          type="button"
                          className="btn secondary btn-sm"
                          onClick={() => handleExportMemory('md')}
                        >
                          {c.v2MemoryExportMd}
                        </button>
                        <button
                          type="button"
                          className="btn primary btn-sm"
                          onClick={() => setSurface('memory')}
                        >
                          {uiLang === 'vi' ? 'Mở đồ thị trí nhớ' : 'Open Memory Graph'}
                        </button>
                      </div>
                    </div>
                    <div className="chip-row" style={{ marginTop: 10 }}>
                      <span className="chip active">
                        Total Nodes: <strong>{safeMemoryGraph.stats.total_nodes}</strong>
                      </span>
                      <span className="chip">
                        Edges: <strong>{safeMemoryGraph.stats.total_edges}</strong>
                      </span>
                      <span className="chip">
                        {c.v2MemoryLayerL1}: <strong>{safeMemoryGraph.stats.l1_count}</strong>
                      </span>
                      <span className="chip">
                        {c.v2MemoryLayerL2}: <strong>{safeMemoryGraph.stats.l2_count}</strong>
                      </span>
                      <span className="chip">
                        {c.v2MemoryLayerL3}: <strong>{safeMemoryGraph.stats.l3_count}</strong>
                      </span>
                    </div>
                  </section>
                )}

                {settingsTab === 'general' && (
                  <>
                    <section className="studio-section">
                      <h4>
                        {uiLang === 'vi'
                          ? 'Ngôn ngữ & Phím thao tác'
                          : 'Language & Composer Behavior'}
                      </h4>
                      <div className="catalog-form-row" style={{ marginTop: 8 }}>
                        <div className="field">
                          <span>{c.v2LangUiLabel}</span>
                          <AgentSelect
                            ariaLabel={c.v2LangUiLabel}
                            value={connForm.interface_language}
                            onChange={(v) =>
                              setConnForm((p) => ({
                                ...p,
                                interface_language: v as 'en' | 'vi',
                              }))
                            }
                            options={[
                              { value: 'en', label: 'English' },
                              { value: 'vi', label: 'Tiếng Việt' },
                            ]}
                          />
                        </div>
                        <div className="field">
                          <span>{c.v2LangModelLabel}</span>
                          <AgentSelect
                            ariaLabel={c.v2LangModelLabel}
                            value={connForm.model_output_language}
                            onChange={(v) =>
                              setConnForm((p) => ({
                                ...p,
                                model_output_language: v as 'vi' | 'en' | 'auto',
                              }))
                            }
                            options={[
                              { value: 'vi', label: 'Tiếng Việt' },
                              { value: 'en', label: 'English' },
                              { value: 'auto', label: 'Auto' },
                            ]}
                          />
                        </div>
                        <div className="field">
                          <span>{c.v2SendKeyLabel}</span>
                          <AgentSelect
                            ariaLabel={c.v2SendKeyLabel}
                            value={connForm.send_key_mode}
                            onChange={(v) =>
                              setConnForm((p) => ({
                                ...p,
                                send_key_mode: v as 'enter' | 'ctrl_enter',
                              }))
                            }
                            options={[
                              { value: 'enter', label: 'Enter' },
                              { value: 'ctrl_enter', label: 'Ctrl/Cmd + Enter' },
                            ]}
                          />
                        </div>
                      </div>
                      <div className="btn-row" style={{ marginTop: 10 }}>
                        <button
                          type="button"
                          className="btn primary btn-sm"
                          onClick={() => saveBridgeConfigPayload(c.bridgeTokenSaved)}
                        >
                          {c.taskModelsSaveBtn}
                        </button>
                      </div>
                    </section>

                    <section className="studio-section conn-diag-section">
                      <div className="probe-card-head">
                        <strong>{c.connDiagTitle}</strong>
                        <span className="tier-badge">{statusLabel}</span>
                      </div>
                      <p className="muted-sub">{c.connDiagSub}</p>
                      <div className="chip-row conn-diag-chips">
                        <span className="chip">
                          {c.diagModePrefix}: {statusLabel}
                        </span>
                        <span className="chip">
                          {bridgeStatus.secret_store_open ? c.diagSecretReady : c.diagSecretSeed}
                        </span>
                        <span className="chip">
                          {c.diagAgentsCount}: {agents.length}
                        </span>
                        <span className="chip">
                          {c.diagWorkspacesCount}: {workspaces.length}
                        </span>
                        <span className="chip">
                          {c.diagSkillsCount}: {skills.filter((s) => s.granted).length}/{skills.length}
                        </span>
                        <span className="chip">
                          {c.skillsMcpGroupsTitle}: {mcpGroups.filter((g) => g.granted).length}/
                          {mcpGroups.length}
                        </span>
                        <span className="chip">
                          {c.diagHostsCount}: {(bridgeStatus.allowed_hosts ?? []).length}
                        </span>
                      </div>
                    </section>
                  </>
                )}

                {settingsTab === 'telegram' && (
                  <section className="studio-section">
                    <div className="ak-toolbar">
                      <h4>Telegram Bridge</h4>
                      <button
                        type="button"
                        className="btn primary btn-sm"
                        onClick={() => {
                          setSurface('studio')
                          setStudioSubTab('agents')
                        }}
                      >
                        {uiLang === 'vi'
                          ? 'Mở cấu hình Telegram trong Studio'
                          : 'Open Telegram in Studio'}
                      </button>
                    </div>
                    <div className="chip-row" style={{ marginTop: 10 }}>
                      <span className="chip active">
                        Route: <strong>{formatRouteLabel(telegramConfig.telegram_route)}</strong>
                      </span>
                      <span className="chip">
                        Mode: <strong>{telegramConfig.transport_mode}</strong>
                      </span>
                      <span className="chip">
                        Bot: <strong>{telegramConfig.bot_name || '—'}</strong>
                      </span>
                      <span className="chip">
                        Policy: <strong>{telegramConfig.chat_policy}</strong>
                      </span>
                      <span className="chip">
                        Routes: <strong>{telegramRoutes.length}</strong>
                      </span>
                    </div>
                    <p className="muted-sub" style={{ marginTop: 10 }}>
                      {uiLang === 'vi'
                        ? 'Bot token, Cygnus relay, quy tắc định tuyến và mã liên kết /link NM-XXXX được quản lý tập trung tại tab Telegram trong Xưởng trợ lý.'
                        : 'Bot credentials, Cygnus relay, routing rules, and /link NM-XXXX codes are managed in Agent Studio -> Telegram.'}
                    </p>
                  </section>
                )}
              </div>
            }
          />
        </div>
      )}

      {surface === 'chat' && (
        <div className="ak-split-wrap">
          <AgentSplitLayout
            storageKey="newsman:chat"
            leftCollapsed={!railOpen}
            rightCollapsed={!sidePanelOpen}
            left={
              <aside className="agents-rail" style={{ width: '100%', border: 0 }}>
                <div className="rail-actions">
                  <button
                    type="button"
                    className="btn small secondary"
                    onClick={() => {
                      setNewRoomKind('direct')
                      setNewRoomMembers([
                        activeWorkspace?.default_agent_slugs[0] ?? agents[0]?.slug ?? 'macro-argus',
                      ])
                      setNewRoomOpen(true)
                    }}
                  >
                    {c.newDirectBtn}
                  </button>
                  <button
                    type="button"
                    className="btn small primary"
                    onClick={() => {
                      setNewRoomKind('group')
                      setNewRoomMembers(
                        activeWorkspace?.default_agent_slugs?.length
                          ? activeWorkspace.default_agent_slugs
                          : agents.map((a) => a.slug),
                      )
                      setNewRoomMode(activeWorkspace?.default_discussion_mode ?? 'parallel_panel')
                      setNewRoomOpen(true)
                    }}
                  >
                    {c.newGroupBtn}
                  </button>
                </div>

                {newRoomOpen && (
                  <div className="new-room-panel">
                    <input
                      value={newRoomTitle}
                      placeholder={c.threadTitleInput}
                      onChange={(e) => setNewRoomTitle(e.target.value)}
                    />
                    {newRoomKind === 'group' && (
                      <>
                        <div className="field">
                          <span>{c.discussionModeLabel}</span>
                          <AgentSelect
                            size="sm"
                            ariaLabel={c.discussionModeLabel}
                            value={newRoomMode}
                            onChange={(v) => setNewRoomMode(v as DiscussionMode)}
                            options={DISCUSSION_MODES.map((m) => ({
                              value: m.id,
                              label: c[m.labelKey],
                            }))}
                          />
                        </div>
                        <div className="field">
                          <span>{c.pickLeadLabel}</span>
                          <AgentSelect
                            size="sm"
                            ariaLabel={c.pickLeadLabel}
                            value={newRoomLead}
                            onChange={setNewRoomLead}
                            options={newRoomMembers.map((slug) => ({
                              value: slug,
                              label: `@${slug}`,
                            }))}
                          />
                        </div>
                      </>
                    )}
                    <div className="chip-row">
                      {agents.map((ag) => {
                        const selected = newRoomMembers.includes(ag.slug)
                        return (
                          <button
                            key={ag.slug}
                            type="button"
                            className={selected ? 'chip active' : 'chip'}
                            onClick={() => {
                              if (newRoomKind === 'direct') {
                                setNewRoomMembers([ag.slug])
                                setNewRoomLead(ag.slug)
                                return
                              }
                              setNewRoomMembers((prev) =>
                                prev.includes(ag.slug)
                                  ? prev.filter((s) => s !== ag.slug)
                                  : [...prev, ag.slug],
                              )
                            }}
                          >
                            {ag.emoji} {ag.display_name}
                          </button>
                        )
                      })}
                    </div>
                    <div className="btn-row">
                      <button
                        type="button"
                        className="btn small primary"
                        onClick={() => void handleCreateRoom()}
                      >
                        {c.createRoomAction}
                      </button>
                      <button
                        type="button"
                        className="btn small secondary"
                        onClick={() => setNewRoomOpen(false)}
                      >
                        {c.cancelBtn}
                      </button>
                    </div>
                  </div>
                )}

                <div className="rail-scroll-body">
                  {pinnedThreads.length > 0 && (
                    <div className="rail-section">
                      <h4>★ {c.v2ThreadsPinned}</h4>
                      {pinnedThreads.map((th) => (
                        <div
                          key={th.id}
                          className={
                            String(selectedThreadId) === String(th.id)
                              ? 'thread-item active'
                              : 'thread-item'
                          }
                        >
                          <button
                            type="button"
                            className="thread-item-btn"
                            onClick={() => setSelectedThreadId(th.id)}
                          >
                            <strong>
                              {th.parent_thread_id ? '↳ ' : ''}
                              {th.title}
                            </strong>
                            <small className="thread-item-meta">
                              <span className="thread-mode-tag">
                                {th.kind === 'group'
                                  ? formatDiscussionModeShort(th.discussion_mode, c)
                                  : 'DM'}
                              </span>
                              <span>{th.member_slugs.map((s) => `@${s}`).join(', ')}</span>
                            </small>
                          </button>
                          <button
                            type="button"
                            className="icon-del"
                            aria-label={c.v2UnpinThreadBtn}
                            title={c.v2UnpinThreadBtn}
                            onClick={() => void updateThread(th.id, { pinned: false })}
                          >
                            ★
                          </button>
                          <button
                            type="button"
                            className="icon-del"
                            aria-label={`${c.deleteThreadLabel}: ${th.title}`}
                            title={c.deleteThreadLabel}
                            onClick={() => handleDeleteThread(th)}
                          >
                            🗑
                          </button>
                        </div>
                      ))}
                    </div>
                  )}

                  <div className="rail-section">
                    <h4>{c.v2ThreadsRecent}</h4>
                    {recentThreads.length === 0 && pinnedThreads.length === 0 && (
                      <p className="empty-hint">{c.noThreadsYet}</p>
                    )}
                    {recentThreads.map((th) => (
                      <div
                        key={th.id}
                        className={
                          String(selectedThreadId) === String(th.id)
                            ? 'thread-item active'
                            : 'thread-item'
                        }
                      >
                        <button
                          type="button"
                          className="thread-item-btn"
                          onClick={() => setSelectedThreadId(th.id)}
                        >
                          <strong>
                            {th.parent_thread_id ? '↳ ' : ''}
                            {th.title}
                          </strong>
                          <small className="thread-item-meta">
                            <span className="thread-mode-tag">
                              {th.kind === 'group'
                                ? formatDiscussionModeShort(th.discussion_mode, c)
                                : 'DM'}
                            </span>
                            <span>{th.member_slugs.map((s) => `@${s}`).join(', ')}</span>
                          </small>
                        </button>
                        <button
                          type="button"
                          className="icon-del"
                          aria-label={c.v2PinThreadBtn}
                          title={c.v2PinThreadBtn}
                          onClick={() => void updateThread(th.id, { pinned: true })}
                        >
                          ☆
                        </button>
                        <button
                          type="button"
                          className="icon-del"
                          aria-label={`${c.deleteThreadLabel}: ${th.title}`}
                          title={c.deleteThreadLabel}
                          onClick={() => handleDeleteThread(th)}
                        >
                          🗑
                        </button>
                      </div>
                    ))}
                  </div>
                </div>
              </aside>
            }
            center={
              <section className="agents-stage" style={{ height: '100%', minWidth: 0 }}>
                <header className="stage-head">
                  <div className="stage-head-main">
                    <button
                      type="button"
                      className={railOpen ? 'rail-toggle-btn active' : 'rail-toggle-btn'}
                      aria-label={railOpen ? c.railHideBtn : c.railShowBtn}
                      aria-expanded={railOpen}
                      title={railOpen ? c.railHideBtn : c.railShowBtn}
                      onClick={() => setRailOpen((v) => !v)}
                    >
                      <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
                        <rect
                          x="1.75"
                          y="2.5"
                          width="12.5"
                          height="11"
                          rx="2.2"
                          stroke="currentColor"
                          strokeWidth="1.4"
                        />
                        <path d="M5.75 2.5v11" stroke="currentColor" strokeWidth="1.4" />
                      </svg>
                    </button>
                    <div className="stage-head-title-wrap">
                      <strong>
                        {activeThread
                          ? `${activeThread.parent_thread_id ? '↳ ' : ''}${activeThread.title}`
                          : c.selectThreadPrompt}
                      </strong>
                      {activeThread && (
                        <div className="stage-seats-bar">
                          {seatOrder.map((ag) => {
                            const state = streamState.seatStates[ag.slug] ?? 'idle'
                            return (
                              <span
                                key={ag.slug}
                                className={`seat-pill state-${state}`}
                                style={{ borderColor: ag.color }}
                              >
                                {ag.emoji} {ag.display_name}
                                {state !== 'idle' && (
                                  <em>
                                    {' '}
                                    ·{' '}
                                    {state === 'thinking'
                                      ? c.seatThinking
                                      : state === 'speaking'
                                        ? c.seatSpeaking
                                        : state === 'tool_calling'
                                          ? c.seatToolCalling
                                          : state === 'error'
                                            ? c.seatError
                                            : c.seatDone}
                                  </em>
                                )}
                              </span>
                            )
                          })}
                          {streamConnected && <span className="sse-badge">● {c.liveStreamBadge}</span>}
                        </div>
                      )}
                    </div>
                  </div>

                  <div className="stage-head-controls">
                    {activeThread?.kind === 'group' && (
                      <div className="mode-pill-row">
                        {DISCUSSION_MODES.map((m) => (
                          <button
                            key={m.id}
                            type="button"
                            className={activeThread.discussion_mode === m.id ? 'chip active' : 'chip'}
                            title={`${c[m.labelKey]} — ${c[m.descKey]}`}
                            onClick={() => void updateThread(activeThread.id, { discussion_mode: m.id })}
                          >
                            {formatDiscussionModeShort(m.id, c)}
                          </button>
                        ))}
                      </div>
                    )}
                    <button
                      type="button"
                      className={
                        sidePanelOpen && sidePanelTab === 'trace'
                          ? 'btn small primary'
                          : 'btn small secondary'
                      }
                      onClick={() => {
                        if (sidePanelOpen && sidePanelTab === 'trace') {
                          setSidePanelOpen(false)
                        } else {
                          setSidePanelTab('trace')
                          setSidePanelOpen(true)
                        }
                      }}
                    >
                      {c.v2DrawerActivity} ({traceEntries.length})
                    </button>
                    <button
                      type="button"
                      className={
                        sidePanelOpen && sidePanelTab === 'context'
                          ? 'btn small primary'
                          : 'btn small secondary'
                      }
                      onClick={() => {
                        if (sidePanelOpen && sidePanelTab === 'context') {
                          setSidePanelOpen(false)
                        } else {
                          setSidePanelTab('context')
                          setSidePanelOpen(true)
                        }
                      }}
                    >
                      📌 {c.v2DrawerNotes}
                      {whiteboard.length > 0 ? ` (${whiteboard.length})` : ''}
                    </button>
                  </div>
                </header>

                {activeThread && (
                  <div className="stage-scope-bar">
                    <AgentSelect
                      size="sm"
                      ariaLabel={c.salonScopeWorkspaceLabel}
                      value={activeThread.workspace_id ?? ''}
                      onChange={(v) => {
                        const nextWs = v || null
                        void updateThread(activeThread.id, { workspace_id: nextWs })
                      }}
                      options={[
                        { value: '', label: c.salonScopeNoWorkspace },
                        ...workspaces.map((ws) => ({ value: ws.id, label: `${ws.emoji} ${ws.name}` })),
                      ]}
                    />

                    <span
                      className={
                        effectiveThreadScope.disjointRegions ? 'scope-chip warning' : 'scope-chip'
                      }
                      title={c.salonScopeEffectiveLabel}
                    >
                      ⌖{' '}
                      {turnScopeFilter.region.trim()
                        ? turnScopeFilter.region.trim()
                        : effectiveThreadScope.disjointRegions
                          ? c.salonScopeDisjoint
                          : effectiveThreadScope.regions?.length
                            ? effectiveThreadScope.regions.join(', ')
                            : c.salonScopeRegionsAll}
                    </span>

                    {turnScopeFilter.series.trim() && (
                      <span className="scope-chip">📊 series:{turnScopeFilter.series.trim()}</span>
                    )}

                    <span
                      className={
                        effectiveThreadScope.disjointTopics ? 'scope-chip warning' : 'scope-chip'
                      }
                      title={c.salonScopeEffectiveLabel}
                    >
                      #{' '}
                      {turnScopeFilter.topic.trim()
                        ? turnScopeFilter.topic.trim()
                        : effectiveThreadScope.disjointTopics
                          ? c.salonScopeDisjoint
                          : effectiveThreadScope.topics?.length
                            ? effectiveThreadScope.topics.join(', ')
                            : c.salonScopeTopicsAll}
                    </span>

                    {((activeThread.pinned_skills?.length ?? 0) > 0 || turnSkillSlugs.length > 0) && (
                      <div className="scope-skills-inline" title={c.salonScopePinnedSkills}>
                        {(activeThread.pinned_skills ?? []).map((slug) => (
                          <code key={`pin-${slug}`} className="scope-skill-chip">
                            /{slug}
                          </code>
                        ))}
                        {turnSkillSlugs
                          .filter((s) => !(activeThread.pinned_skills ?? []).includes(s))
                          .map((slug) => (
                            <button
                              key={`turn-${slug}`}
                              type="button"
                              className="scope-skill-chip attached"
                              onClick={() =>
                                setTurnSkillSlugs((prev) => prev.filter((item) => item !== slug))
                              }
                            >
                              /{slug} ×
                            </button>
                          ))}
                      </div>
                    )}
                  </div>
                )}

                <div className="stage-messages" style={{ flex: 1, minHeight: 0, overflowY: 'auto' }}>
                  {messages.length === 0 && (
                    <div className="starter-prompts-box">
                      <p className="muted-sub">{c.selectThreadPrompt}</p>
                      <div className="chip-row">
                        {(manifest?.quick_actions && manifest.quick_actions.length > 0
                          ? manifest.quick_actions.map((qa) => ({
                              label: (uiLang === 'vi' ? qa.label_vi : qa.label_en) ?? qa.label ?? qa.id,
                              prompt: qa.prompt ?? qa.prompt_template ?? '',
                            }))
                          : [c.starterMacroToday, c.starterTaxReview, c.starterPolicyWatch].map(
                              (p) => ({ label: p, prompt: p }),
                            )
                        ).map((item) => (
                          <button
                            key={item.label}
                            type="button"
                            className="chip"
                            onClick={() => void handleSend(item.prompt)}
                          >
                            ✦ {item.label}
                          </button>
                        ))}
                      </div>
                    </div>
                  )}

                  {messages.map((m: AgentMessage) => {
                    const isUser = m.role === 'user'
                    const ag = m.sender_slug ? agentMap.get(m.sender_slug) : undefined
                    const traceId = buildSeatTraceId(m)
                    const steps: ExecutionTraceStep[] =
                      m.trace_steps && m.trace_steps.length > 0
                        ? m.trace_steps.map((raw) => ({
                            merge_id: typeof raw.merge_id === 'string' ? raw.merge_id : undefined,
                            tool: typeof raw.tool === 'string' ? raw.tool : 'tool',
                            origin:
                              typeof raw.origin === 'string'
                                ? (raw.origin as ExecutionTraceStep['origin'])
                                : undefined,
                            status: typeof raw.status === 'string' ? raw.status : undefined,
                            duration_ms:
                              typeof raw.duration_ms === 'number' ? raw.duration_ms : undefined,
                          }))
                        : (m.tool_calls ?? []).map((tc) => ({
                            merge_id: tc.merge_id,
                            tool: tc.tool,
                            origin: (tc.origin as ExecutionTraceStep['origin']) ?? undefined,
                            status: tc.status,
                            duration_ms: tc.duration_ms ?? undefined,
                          }))
                    const hasTrace = Boolean(m.reasoning) || steps.length > 0
                    return (
                      <div
                        key={m.id}
                        className={
                          isUser
                            ? 'salon-turn salon-turn-user'
                            : 'salon-turn salon-turn-agent partner-seat'
                        }
                      >
                        <div
                          className="salon-turn-card"
                          style={!isUser && ag ? { borderLeft: `3px solid ${ag.color}` } : undefined}
                        >
                          <header className="salon-turn-meta">
                            <b>
                              {isUser
                                ? uiLang === 'vi'
                                  ? 'Biên tập viên'
                                  : 'Editor'
                                : `${ag?.emoji ?? '📈'} ${ag?.display_name ?? m.sender_slug ?? 'Agent'}`}
                            </b>
                            <div className="seat-meta-right">
                              {m.round_idx && <span className="round-tag">R{m.round_idx}</span>}
                              {!isUser && hasTrace && (
                                <button
                                  type="button"
                                  className={
                                    selectedTraceId === traceId ? 'trace-badge-btn active' : 'trace-badge-btn'
                                  }
                                  onClick={() => handleOpenTrace(traceId)}
                                >
                                  {c.v2ExecutionTrace} · {steps.length} {c.traceToolsUnit} ▸
                                </button>
                              )}
                            </div>
                          </header>

                          {!isUser && hasTrace && (
                            <details className="ak-trace-box">
                              <summary style={{ cursor: 'pointer', fontWeight: 600 }}>
                                {c.v2ExecutionTrace} ({steps.length})
                              </summary>
                              {m.reasoning && <pre className="trace-reasoning-pre">{m.reasoning}</pre>}
                              {steps.map((st, idx) => {
                                const originTag =
                                  st.origin === 'builtin'
                                    ? c.v2TraceBuiltin
                                    : st.origin === 'app_domain' || st.origin === 'app_v4s'
                                      ? c.v2TraceApp
                                      : c.v2TraceMcp
                                return (
                                  <div key={st.merge_id ?? `${st.tool}-${idx}`} className="ak-trace-row">
                                    <span>
                                      <span className="ak-origin-pill">{originTag}</span>{' '}
                                      <code>{st.tool}</code>
                                    </span>
                                    <span className="muted-sub">
                                      {typeof st.duration_ms === 'number' ? `${st.duration_ms}ms · ` : ''}
                                      {st.status ?? 'ok'}
                                    </span>
                                  </div>
                                )
                              })}
                            </details>
                          )}

                          <div className="salon-turn-prose">{renderMessageContent(m.content)}</div>

                          <footer className="seat-hover-actions">
                            <button
                              type="button"
                              className="seat-action-btn"
                              onClick={() => {
                                void navigator.clipboard?.writeText(m.content)
                                setCopiedMsgId(String(m.id))
                                setTimeout(() => setCopiedMsgId(null), 1500)
                              }}
                            >
                              {copiedMsgId === String(m.id) ? '✓' : c.v2CopyBtn}
                            </button>
                            <button
                              type="button"
                              className="seat-action-btn"
                              onClick={() => void handleBranchFromMessage(m.id)}
                            >
                              ⑂ {c.v2BranchBtn}
                            </button>
                            {!isUser && m.sender_slug && (
                              <button
                                type="button"
                                className="seat-action-btn"
                                disabled={sending}
                                onClick={() => void retrySeat(m.sender_slug!, turnModelOverride || null)}
                              >
                                ↻ {c.v2RetrySeatBtn}
                              </button>
                            )}
                            {!isUser && (
                              <button
                                type="button"
                                className="seat-action-btn"
                                onClick={() => {
                                  const quoted = `> ${m.content.slice(0, 180)}\n\n`
                                  setComposerText((prev) => `${quoted}${prev}`)
                                  composerRef.current?.focus()
                                }}
                              >
                                {c.actionQuote}
                              </button>
                            )}
                            {!isUser && m.sender_slug && (
                              <button
                                type="button"
                                className="seat-action-btn"
                                onClick={() => {
                                  setComposerText((prev) => `@${m.sender_slug} ${prev}`)
                                  composerRef.current?.focus()
                                }}
                              >
                                {c.actionAskPeer}
                                {m.sender_slug}
                              </button>
                            )}
                          </footer>
                        </div>
                      </div>
                    )
                  })}

                  {askUserPrompts.map((prompt) => (
                    <div key={prompt.prompt_id} className="ak-ask-card">
                      <div className="ak-toolbar">
                        <strong>
                          @{prompt.agent_slug} — {prompt.question}
                        </strong>
                        <span className="tier-badge">{prompt.status}</span>
                      </div>
                      {prompt.status === 'pending' ? (
                        <>
                          {prompt.options.length > 0 && (
                            <div className="chip-row">
                              {prompt.options.map((opt) => (
                                <button
                                  key={opt}
                                  type="button"
                                  className="chip"
                                  onClick={() => void answerAskUser(prompt.prompt_id, opt, true)}
                                >
                                  {opt}
                                </button>
                              ))}
                            </div>
                          )}
                          {prompt.allow_custom && (
                            <div className="chat-input-row">
                              <input
                                value={askCustomAnswers[prompt.prompt_id] ?? ''}
                                placeholder={c.v2AskUserCustomPlaceholder}
                                onChange={(e) =>
                                  setAskCustomAnswers((prev) => ({
                                    ...prev,
                                    [prompt.prompt_id]: e.target.value,
                                  }))
                                }
                              />
                              <button
                                type="button"
                                className="btn primary btn-sm"
                                disabled={!(askCustomAnswers[prompt.prompt_id] ?? '').trim()}
                                onClick={() => {
                                  const val = (askCustomAnswers[prompt.prompt_id] ?? '').trim()
                                  if (!val) return
                                  void answerAskUser(prompt.prompt_id, val, true)
                                }}
                              >
                                {c.v2AskUserSubmit}
                              </button>
                            </div>
                          )}
                        </>
                      ) : (
                        <small className="muted-sub">→ {prompt.answer}</small>
                      )}
                    </div>
                  ))}

                  {streamState.toolPills.length > 0 && (
                    <div className="ak-trace-box">
                      <strong>{c.v2ExecutionTrace} (live)</strong>
                      {streamState.toolPills.map((pill) => (
                        <div key={pill.merge_id} className="ak-trace-row">
                          <span>
                            <span className="ak-origin-pill">{pill.origin ?? c.v2TraceMcp}</span>{' '}
                            <code>{pill.tool}</code>
                          </span>
                          <span className="muted-sub">
                            {typeof pill.duration_ms === 'number' ? `${pill.duration_ms}ms` : pill.status}
                          </span>
                        </div>
                      ))}
                    </div>
                  )}

                  {Object.values(streamState.drafts).map((draft) => {
                    const ag = agentMap.get(draft.agent_slug)
                    return (
                      <div key={draft.agent_slug} className="salon-turn salon-turn-agent partner-seat streaming">
                        <div className="salon-turn-card">
                          <header className="salon-turn-meta">
                            <b>
                              {ag?.emoji ?? '📈'} {ag?.display_name ?? draft.agent_slug}
                            </b>
                            <span className="tier-badge running">{c.seatSpeaking}</span>
                          </header>
                          <div className="salon-turn-prose">{draft.text}</div>
                        </div>
                      </div>
                    )
                  })}
                  <div ref={listEndRef} />
                </div>

                {(error || streamState.lastError) && (
                  <p className="form-error">{error ?? streamState.lastError}</p>
                )}

                <footer className="composer-tray">
                  <div className="ak-composer-t1">
                    {(
                      [
                        ['scope', c.v2ScopeBtn],
                        ['mode', `${c.v2ModeBtn}: ${chatMode}`],
                        ['skills', `${c.v2SkillsBtn}${turnSkillSlugs.length ? ` (${turnSkillSlugs.length})` : ''}`],
                        ['mcp', c.v2McpBtn],
                        ['style', `${c.v2StyleBtn}: ${personaStyle}`],
                        ['customize', c.v2CustomizeBtn],
                      ] as const
                    ).map(([popId, popLabel]) => (
                      <div key={popId} className="ak-popover-anchor">
                        <button
                          type="button"
                          className={composerPopover === popId ? 'chip active' : 'chip'}
                          onClick={() =>
                            setComposerPopover((prev) => (prev === popId ? null : popId))
                          }
                        >
                          {popLabel}
                        </button>
                        {composerPopover === popId && (
                          <div className="ak-popover" role="dialog">
                            {popId === 'scope' && (
                              <>
                                <input
                                  value={turnScopeFilter.region}
                                  placeholder="region (e.g. vn, us, eu)"
                                  onChange={(e) =>
                                    setTurnScopeFilter((p) => ({ ...p, region: e.target.value }))
                                  }
                                />
                                <input
                                  value={turnScopeFilter.series}
                                  placeholder="series (e.g. cpi, policy_rate)"
                                  onChange={(e) =>
                                    setTurnScopeFilter((p) => ({ ...p, series: e.target.value }))
                                  }
                                />
                                <input
                                  value={turnScopeFilter.topic}
                                  placeholder="topic (e.g. macro, tax)"
                                  onChange={(e) =>
                                    setTurnScopeFilter((p) => ({ ...p, topic: e.target.value }))
                                  }
                                />
                              </>
                            )}
                            {popId === 'mode' && (
                              <div className="chip-row">
                                {(['chat', 'co_writer', 'research', 'brief'] as const).map((m) => (
                                  <button
                                    key={m}
                                    type="button"
                                    className={chatMode === m ? 'chip active' : 'chip'}
                                    onClick={() => {
                                      setChatMode(m)
                                      if (activeThread) {
                                        void updateThread(activeThread.id, { chat_mode: m })
                                      }
                                      setComposerPopover(null)
                                    }}
                                  >
                                    {m}
                                  </button>
                                ))}
                              </div>
                            )}
                            {popId === 'skills' && (
                              <div className="chip-row">
                                {grantedSkills.map((sk) => {
                                  const on = turnSkillSlugs.includes(sk.slug)
                                  return (
                                    <button
                                      key={sk.slug}
                                      type="button"
                                      className={on ? 'chip active' : 'chip'}
                                      onClick={() =>
                                        setTurnSkillSlugs((prev) =>
                                          on ? prev.filter((x) => x !== sk.slug) : [...prev, sk.slug],
                                        )
                                      }
                                    >
                                      {on ? '✓ ' : ''}
                                      {skillDisplayName(sk)}
                                    </button>
                                  )
                                })}
                              </div>
                            )}
                            {popId === 'mcp' && (
                              <div className="chip-row">
                                {mcpNamespaces.map((ns) => (
                                  <span key={ns.namespace} className={ns.enabled ? 'chip active' : 'chip'}>
                                    {ns.enabled ? '✓ ' : ''}
                                    {ns.namespace}
                                  </span>
                                ))}
                              </div>
                            )}
                            {popId === 'style' && (
                              <div className="chip-row">
                                {(['analytical', 'concise', 'executive', 'neutral'] as const).map(
                                  (st) => (
                                    <button
                                      key={st}
                                      type="button"
                                      className={personaStyle === st ? 'chip active' : 'chip'}
                                      onClick={() => {
                                        setPersonaStyle(st)
                                        if (activeThread) {
                                          void updateThread(activeThread.id, { persona_style: st })
                                        }
                                        setComposerPopover(null)
                                      }}
                                    >
                                      {st}
                                    </button>
                                  ),
                                )}
                              </div>
                            )}
                            {popId === 'customize' && (
                              <>
                                <small className="muted-sub">
                                  {c.v2SendKeyLabel}:{' '}
                                  {bridgeStatus.send_key_mode === 'ctrl_enter'
                                    ? 'Ctrl/Cmd+Enter'
                                    : 'Enter'}
                                </small>
                                <div className="chip-row">
                                  <button
                                    type="button"
                                    className="chip"
                                    onClick={() =>
                                      void saveBridgeConfig({
                                        send_key_mode:
                                          bridgeStatus.send_key_mode === 'ctrl_enter'
                                            ? 'enter'
                                            : 'ctrl_enter',
                                      })
                                    }
                                  >
                                    Toggle Send Key
                                  </button>
                                  <button
                                    type="button"
                                    className="chip"
                                    onClick={() =>
                                      void saveBridgeConfig({
                                        interface_language: uiLang === 'vi' ? 'en' : 'vi',
                                      })
                                    }
                                  >
                                    UI: {uiLang.toUpperCase()}
                                  </button>
                                </div>
                              </>
                            )}
                          </div>
                        )}
                      </div>
                    ))}
                  </div>

                  {autocompleteItems.length > 0 && (
                    <div className="composer-autocomplete" role="listbox">
                      {autocompleteItems.map((item) => (
                        <button
                          key={`${item.kind}-${item.slug}`}
                          type="button"
                          className="autocomplete-item"
                          onClick={() => handleApplyAutocomplete(item)}
                        >
                          <span>{item.label}</span>
                          <code>{item.sub}</code>
                        </button>
                      ))}
                    </div>
                  )}

                  {turnSkillSlugs.length > 0 && (
                    <div className="chip-row">
                      {turnSkillSlugs.map((slug) => (
                        <button
                          key={slug}
                          type="button"
                          className="chip active"
                          onClick={() => setTurnSkillSlugs((prev) => prev.filter((s) => s !== slug))}
                        >
                          🧩 /{slug} ×
                        </button>
                      ))}
                    </div>
                  )}

                  <textarea
                    ref={composerRef}
                    rows={1}
                    value={composerText}
                    placeholder={c.composerPlaceholder}
                    onChange={(e) => {
                      setComposerText(e.target.value)
                      setCaretPos(e.target.selectionStart ?? e.target.value.length)
                    }}
                    onClick={(e) => setCaretPos(e.currentTarget.selectionStart ?? composerText.length)}
                    onKeyUp={(e) => setCaretPos(e.currentTarget.selectionStart ?? composerText.length)}
                    onKeyDown={(e) => {
                      if (e.key === 'Tab' && ghostHint) {
                        e.preventDefault()
                        setComposerText((prev) =>
                          prev.trimEnd().length > 0 ? `${prev.trimEnd()} ${ghostHint}` : ghostHint,
                        )
                        setGhostHint('')
                        return
                      }
                      const useCtrl = bridgeStatus.send_key_mode === 'ctrl_enter'
                      const isSend = useCtrl
                        ? (e.ctrlKey || e.metaKey) && e.key === 'Enter'
                        : e.key === 'Enter' && !e.shiftKey
                      if (isSend) {
                        e.preventDefault()
                        void handleSend()
                      }
                    }}
                  />

                  {ghostHint && (
                    <div className="ak-ghost-hint">
                      <span>{ghostHint}</span>
                      <button
                        type="button"
                        className="btn secondary btn-sm"
                        onClick={() => {
                          setComposerText((prev) => `${prev.trimEnd()} ${ghostHint}`)
                          setGhostHint('')
                        }}
                      >
                        Tab · {c.v2TabHintPrompt}
                      </button>
                    </div>
                  )}

                  <div className="composer-tray-bar">
                    <div className="composer-tray-left">
                      <div className="composer-quick-triggers">
                        <button
                          type="button"
                          className="composer-trigger-btn"
                          title={c.composerMentionTitle}
                          onClick={() => handleInsertTrigger('@')}
                        >
                          {c.composerMentionBtn}
                        </button>
                        <button
                          type="button"
                          className="composer-trigger-btn"
                          title={c.composerSkillTitle}
                          onClick={() => handleInsertTrigger('/')}
                        >
                          {c.composerSkillBtn}
                        </button>
                      </div>
                      <AgentSelect
                        size="sm"
                        ariaLabel={c.composerModelLabel}
                        value={turnModelOverride}
                        onChange={setTurnModelOverride}
                        options={[
                          { value: '', label: `${c.composerModelDefault} (${roleSummary.smartModel})` },
                          ...selectableModels.map((m) => ({ value: m, label: m })),
                        ]}
                      />
                      {composerBytes > 0 && (
                        <span className="context-budget-chip" title={c.composerBudgetLabel}>
                          {composerBytes < 1024 ? '<1' : (composerBytes / 1024).toFixed(1)} / 16 KB
                        </span>
                      )}
                    </div>
                    <button
                      type="button"
                      className="btn primary"
                      disabled={sending || !composerText.trim()}
                      onClick={() => void handleSend()}
                    >
                      {sending ? c.sendingTurnBtn : c.sendTurnBtn}
                    </button>
                  </div>
                </footer>
              </section>
            }
            right={
              <aside className="group-side-panel" style={{ width: '100%', border: 0 }}>
                <div className="side-panel-tabs">
                  <button
                    type="button"
                    className={sidePanelTab === 'trace' ? 'tab active' : 'tab'}
                    onClick={() => setSidePanelTab('trace')}
                  >
                    {c.v2DrawerActivity}
                  </button>
                  <button
                    type="button"
                    className={sidePanelTab === 'context' ? 'tab active' : 'tab'}
                    onClick={() => setSidePanelTab('context')}
                  >
                    {c.v2DrawerNotes}
                  </button>
                  <button
                    type="button"
                    className="side-panel-close"
                    aria-label={c.cancelBtn}
                    title={c.cancelBtn}
                    onClick={() => setSidePanelOpen(false)}
                  >
                    ✕
                  </button>
                </div>

                {sidePanelTab === 'trace' && (
                  <div className="side-panel-trace-list">
                    {traceEntries.length === 0 ? (
                      <p className="empty-hint">{c.panelNoTraceYet}</p>
                    ) : (
                      traceEntries.map(({ traceId, message, agent }) => (
                        <article
                          key={traceId}
                          data-trace-id={traceId}
                          className={selectedTraceId === traceId ? 'trace-card active' : 'trace-card'}
                        >
                          <header className="trace-card-head">
                            <strong>
                              {agent?.emoji ?? '📈'} {agent?.display_name ?? message.sender_slug ?? 'Agent'}
                            </strong>
                            {message.round_idx && <span className="round-tag">R{message.round_idx}</span>}
                          </header>
                          {message.reasoning && <pre className="trace-reasoning-pre">{message.reasoning}</pre>}
                          {message.tool_calls && message.tool_calls.length > 0 && (
                            <div className="salon-tool-list">
                              {message.tool_calls.map((tc) => (
                                <ToolInvocationPill key={tc.merge_id} pill={tc} lang={uiLang} />
                              ))}
                            </div>
                          )}
                        </article>
                      ))
                    )}
                  </div>
                )}

                {sidePanelTab === 'context' && (
                  <div className="side-panel-context-pane">
                    {activeWorkspace && (
                      <div className="workspace-ctx-summary">
                        <strong>
                          {activeWorkspace.emoji} {activeWorkspace.name}
                        </strong>
                        <p className="muted-sub">{activeWorkspace.description}</p>
                        <div className="chip-row">
                          <span className="scope-badge">
                            ⌖{' '}
                            {activeWorkspace.resource_ctx.regions?.length
                              ? activeWorkspace.resource_ctx.regions.join(', ')
                              : c.salonScopeRegionsAll}
                          </span>
                          <span className="scope-badge">
                            #{' '}
                            {activeWorkspace.resource_ctx.topics?.length
                              ? activeWorkspace.resource_ctx.topics.join(', ')
                              : c.salonScopeTopicsAll}
                          </span>
                        </div>
                      </div>
                    )}

                    {activeThread && (
                      <div className="studio-scope-box" style={{ marginBottom: 10 }}>
                        <div className="ak-toolbar">
                          <strong>{c.v2L1ScratchpadLabel}</strong>
                          <button
                            type="button"
                            className="btn secondary btn-sm"
                            disabled={notesSaving}
                            onClick={() => {
                              setNotesSaving(true)
                              void updateThread(activeThread.id, { scratchpad_md: notesDraft }).finally(
                                () => setNotesSaving(false),
                              )
                            }}
                          >
                            {c.v2SaveNotesBtn}
                          </button>
                        </div>
                        <textarea
                          className="ak-notes-editor"
                          value={notesDraft}
                          onChange={(e) => setNotesDraft(e.target.value)}
                        />
                      </div>
                    )}

                    <div className="wb-head">
                      <div>
                        <strong>{c.whiteboardTitle}</strong>
                        <p>{c.whiteboardSub}</p>
                      </div>
                      <button
                        type="button"
                        className="btn small secondary"
                        disabled={summarizing || !selectedThreadId}
                        onClick={() => {
                          setSummarizing(true)
                          void summarizeRound().finally(() => setSummarizing(false))
                        }}
                      >
                        {summarizing ? c.whiteboardSummarizing : c.whiteboardSummarizeBtn}
                      </button>
                    </div>
                    <div className="wb-list">
                      {whiteboard.length === 0 && <p className="empty-hint">{c.whiteboardEmpty}</p>}
                      {whiteboard.map((note) => (
                        <div key={note.id} className="wb-note">
                          <div className="wb-note-head">
                            <span>
                              {note.pinned && <span className="tier-badge">{c.whiteboardPinnedBadge}</span>}{' '}
                              <b>@{note.author_slug ?? 'owner'}</b>
                            </span>
                            <button
                              type="button"
                              className="icon-del"
                              aria-label={c.cancelBtn}
                              title={c.cancelBtn}
                              onClick={() => void deleteWhiteboardNote(note.id)}
                            >
                              🗑
                            </button>
                          </div>
                          <p>{note.content}</p>
                        </div>
                      ))}
                    </div>
                    <div className="wb-add">
                      <textarea
                        rows={2}
                        value={wbInput}
                        placeholder={c.whiteboardPlaceholder}
                        onChange={(e) => setWbInput(e.target.value)}
                      />
                      <button
                        type="button"
                        className="btn small primary"
                        disabled={!wbInput.trim() || !selectedThreadId}
                        onClick={() => {
                          const txt = wbInput.trim()
                          setWbInput('')
                          void addWhiteboardNote(txt)
                        }}
                      >
                        {c.whiteboardAddBtn}
                      </button>
                    </div>
                  </div>
                )}
              </aside>
            }
          />
        </div>
      )}
    </div>
  )
}
