import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { AgentStudio } from './AgentStudio.tsx'
import { AgentChatHeaderBar } from './agent-kit/AgentChatHeaderBar.tsx'
import { AgentContextBudgetChip } from './agent-kit/AgentContextBudgetChip.tsx'
import { AgentExecutionTrace } from './agent-kit/AgentExecutionTrace.tsx'
import { AgentMarkdown } from './agent-kit/AgentMarkdown.tsx'
import {
  AgentFoldableComposerTray,
  AgentModePopover,
  AgentPartnersConsultPopover,
  AgentResourcesPopover,
  AgentRoleplayPreferencesPopover,
} from './agent-kit/AgentPersonaRoleplayMenus.tsx'
import { AgentQueuedMessages } from './agent-kit/AgentQueuedMessages.tsx'
import { AgentSelect, type ProviderStatusDot } from './agent-kit/AgentSelect.tsx'
import { AgentSelectionToolbar } from './agent-kit/AgentSelectionToolbar.tsx'
import { AgentSessionActivity } from './agent-kit/AgentSessionActivity.tsx'
import { AgentComposerLead, AgentSkillPicker } from './agent-kit/AgentSkillPicker.tsx'
import { AgentSplitLayout } from './agent-kit/AgentSplitPane.tsx'
import { AgentTurnNavigator } from './agent-kit/AgentTurnNavigator.tsx'
import { AgentWorkspaceDock, useThreadTabStash } from './agent-kit/AgentWorkspaceDock.tsx'
import {
  AGENT_COPY,
  buildContextBudgetV2,
  buildSessionActivityV2,
  formatBridgeModeLabel,
  formatDiscussionModeShort,
  type AccessRequestTargetKind,
  type AgentMessage,
  type AgentThread,
  type AgentWorkspace,
  type AppAgentConfig,
  type CapabilityTriState,
  type ChatHeaderControlsConfigV2,
  type ComposerModeOptionV2,
  type CoWriterDraftRow,
  type DeclaredAppAgentV2,
  type DiscussionMode,
  type ExecutionTraceStep,
  type MemoryGraphNode,
  type MergedToolInvocation,
  type QueuedMessageV2,
  type ResourceCategoryItemV2,
  type ResponseStyleOptionV2,
  type SelectionAnchorV2,
  type SessionArtifactFileV2,
  type SideChatResponderV2,
  type SkillOptionV2,
  type TurnOutlineEntryV2,
  type WorkspaceSurface,
} from './agentTypes.ts'
import {
  computeEffectiveScope,
  deriveAgentStreamPhaseV2,
  extractMentions,
  buildSeatTraceId,
  filterSurfacesByManifest,
  mapToolToTraceStepV2,
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
  initialPrompt?: string | null
  onConsumeInitialPrompt?: () => void
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

export function AgentsWorkspace({
  lang,
  onOpenArticle,
  onOpenRegions,
  initialPrompt,
  onConsumeInitialPrompt,
}: AgentsWorkspaceProps) {
  const {
    bridgeStatus,
    manifest,
    modelCatalog,
    mcpNamespaces,
    memoryGraph,
    agents,
    models,
    threads,
    workspaces,
    activeWorkspaceId,
    setActiveWorkspaceId,
    skills,
    customSkills,
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
    saveModelRoles,
    fetchModelCatalog,
    toggleCatalogModels,
    upsertCatalogModel,
    bulkUpsertCatalogModels,
    probeConnections,
    saveMcpNamespaces,
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
    toggleSkillPin,
    saveCustomSkill,
    deleteCustomSkill,
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

  const uiLang: 'vi' | 'en' = lang === 'en' ? 'en' : 'vi'
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

  const [surface, setSurface] = useState<'chat' | 'cowriter' | 'studio' | 'memory' | 'settings'>('chat')
  const [studioSubTab, setStudioSubTab] = useState<'agents' | 'workspaces' | 'skills'>('agents')
  const caps = manifest?.capabilities ?? manifest?.features
  const visibleSettingsTabs = useMemo<NewsmanSettingsTab[]>(() => {
    const tabs: NewsmanSettingsTab[] = ['connections', 'models', 'mcp', 'memory', 'general']
    if (caps?.telegram !== false) tabs.push('telegram')
    return tabs
  }, [caps?.telegram])
  const [settingsTab, setSettingsTab] = useState<NewsmanSettingsTab>('connections')
  useEffect(() => {
    if (!visibleSettingsTabs.includes(settingsTab)) {
      setSettingsTab('connections')
    }
  }, [settingsTab, visibleSettingsTabs])
  const [selectedThreadId, setSelectedThreadId] = useState<number | string | null>(null)
  const [railOpen, setRailOpen] = useState(true)
  const [sidePanelOpen, setSidePanelOpen] = useState(true)
  const [sidePanelTab, setSidePanelTab] = useState<'trace' | 'context'>('trace')
  const [selectedTraceId, setSelectedTraceId] = useState<string | null>(null)
  const [composerText, setComposerText] = useState('')
  const [caretPos, setCaretPos] = useState(0)
  const [turnSkillSlugs, setTurnSkillSlugs] = useState<string[]>([])
  const [applySkillEveryTurn, setApplySkillEveryTurn] = useState(true)
  const [turnModelOverride, setTurnModelOverride] = useState('')
  const [copiedMsgId, setCopiedMsgId] = useState<string | null>(null)
  const [composerPopover, setComposerPopover] = useState<null | 'scope' | 'mcp'>(null)
  const [queuedMessages, setQueuedMessages] = useState<QueuedMessageV2[]>([])
  const [selectionBar, setSelectionBar] = useState<{
    anchor: SelectionAnchorV2
    coords: { x: number; y: number }
  } | null>(null)
  const [selectedResponderSlug, setSelectedResponderSlug] = useState<string>('macro-argus')
  const [activeResponderSlug, setActiveResponderSlug] = useState<string | null>(null)
  const [prefsEveryTurn, setPrefsEveryTurn] = useState(true)
  const [consultPartnerSlugs, setConsultPartnerSlugs] = useState<string[]>([])
  const [partnersEveryTurn, setPartnersEveryTurn] = useState(false)
  const [controlsConfig, setControlsConfig] = useState<ChatHeaderControlsConfigV2>(() => {
    const defaults: ChatHeaderControlsConfigV2 = {
      showSkills: true,
      showMcp: true,
      showPartners: true,
      showPreferences: true,
      showContextRing: true,
    }
    try {
      const raw = window.localStorage.getItem('lnc-ak-hdr-controls:newsman')
      if (!raw) return defaults
      const parsed = JSON.parse(raw) as Partial<ChatHeaderControlsConfigV2>
      return { ...defaults, ...parsed }
    } catch {
      return defaults
    }
  })
  const updateControlsConfig = (next: ChatHeaderControlsConfigV2) => {
    setControlsConfig(next)
    try {
      window.localStorage.setItem('lnc-ak-hdr-controls:newsman', JSON.stringify(next))
    } catch {
      /* ignore */
    }
  }
  const [isCompactViewport, setIsCompactViewport] = useState<boolean>(() =>
    typeof window !== 'undefined' ? window.innerWidth < 960 : false,
  )
  useEffect(() => {
    const onResize = () => setIsCompactViewport(window.innerWidth < 960)
    window.addEventListener('resize', onResize)
    return () => window.removeEventListener('resize', onResize)
  }, [])
  useEffect(() => {
    if (!composerPopover) return
    const closePop = () => setComposerPopover(null)
    const onEsc = (ev: KeyboardEvent) => {
      if (ev.key === 'Escape') closePop()
    }
    window.addEventListener('keydown', onEsc)
    window.addEventListener('hashchange', closePop)
    window.addEventListener('popstate', closePop)
    return () => {
      window.removeEventListener('keydown', onEsc)
      window.removeEventListener('hashchange', closePop)
      window.removeEventListener('popstate', closePop)
    }
  }, [composerPopover])
  const [createSkillOpen, setCreateSkillOpen] = useState(false)
  const [createSkillForm, setCreateSkillForm] = useState({
    slug: '',
    name: '',
    description: '',
    slash_command: '',
    skill_md: '',
  })
  const [createSkillBusy, setCreateSkillBusy] = useState(false)
  const [bulkImportOpen, setBulkImportOpen] = useState(false)
  const [bulkImportProvider, setBulkImportProvider] = useState('hub')
  const [bulkImportRaw, setBulkImportRaw] = useState('')
  const [bulkPreserveEnabled, setBulkPreserveEnabled] = useState(true)
  const [bulkImportBusy, setBulkImportBusy] = useState(false)
  const [fetchingCatalog, setFetchingCatalog] = useState(false)
  const [smartRoleDraft, setSmartRoleDraft] = useState('')
  const [fastRoleDraft, setFastRoleDraft] = useState('')
  const [chatMode, setChatMode] = useState<string>('')
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

  // Settings / MCP Namespace state
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
    interface_language: (bridgeStatus.interface_language ?? 'en') as 'en' | 'vi',
    model_output_language: (bridgeStatus.model_output_language ?? 'vi') as 'vi' | 'en' | 'auto',
    send_key_mode: (bridgeStatus.send_key_mode ?? 'enter') as 'enter' | 'ctrl_enter',
    max_tool_rounds: bridgeStatus.max_tool_rounds ?? 8,
    max_group_hops: bridgeStatus.max_group_hops ?? 3,
    default_temperature: bridgeStatus.default_temperature ?? 0.4,
    allowed_hosts: (bridgeStatus.allowed_hosts ?? []).join(', '),
    astraea_token: '',
    thread_title: bridgeStatus.task_models?.thread_title ?? '',
    round_summary: bridgeStatus.task_models?.round_summary ?? '',
    companion_brief: bridgeStatus.task_models?.companion_brief ?? '',
    tax_doc_digest: bridgeStatus.task_models?.tax_doc_digest ?? '',
  })
  const [connNotice, setConnNotice] = useState('')
  const [connError, setConnError] = useState('')

  const listEndRef = useRef<HTMLDivElement | null>(null)
  const scrollRootRef = useRef<HTMLDivElement | null>(null)
  const composerRef = useRef<HTMLTextAreaElement | null>(null)

  useEffect(() => {
    if (!initialPrompt || !initialPrompt.trim()) return
    setSurface('chat')
    setComposerText(initialPrompt.trim())
    onConsumeInitialPrompt?.()
    const timer = window.setTimeout(() => {
      composerRef.current?.focus()
    }, 30)
    return () => window.clearTimeout(timer)
  }, [initialPrompt, onConsumeInitialPrompt])

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
    setSmartRoleDraft(bridgeStatus.default_smart_model ?? roleSummary.smartModel ?? '')
    setFastRoleDraft(bridgeStatus.fast_background_model ?? roleSummary.fastModel ?? '')
  }, [
    bridgeStatus.default_smart_model,
    bridgeStatus.fast_background_model,
    roleSummary.smartModel,
    roleSummary.fastModel,
  ])

  useEffect(() => {
    const next: Record<string, boolean> = {}
    for (const ns of mcpNamespaces) next[ns.namespace] = ns.enabled
    setMcpDraft(next)
  }, [mcpNamespaces])

  useEffect(() => {
    setConnForm((prev) => ({
      ...prev,
      astraea_url: bridgeStatus.astraea_url ?? prev.astraea_url,
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
    const cm = activeThread?.chat_mode ?? ''
    setChatMode(cm === 'chat' ? '' : cm)
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
    reconnectNotice,
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
      } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'j' && surface === 'chat') {
        e.preventDefault()
        setSidePanelOpen((v) => !v)
      } else if (e.key === 'Escape' && paletteOpen) {
        setPaletteOpen(false)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [paletteOpen, surface])

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

  const dockStash = useThreadTabStash([
    { id: 'activity', kind: 'activity', title: `📈 ${c.v2DrawerActivity}`, closable: false },
    { id: 'notes', kind: 'notes', title: `📝 ${c.v2DrawerNotes}`, closable: false },
  ])
  const dockThreadKey = String(activeThread?.id ?? 'default')
  const dockState = dockStash.ensure(dockThreadKey)
  const activeDockTab =
    dockState.tabs.find((t) => t.id === dockState.activeId) ?? dockState.tabs[0] ?? null

  useEffect(() => {
    setQueuedMessages([])
    setSelectionBar(null)
  }, [selectedThreadId])

  const handleOpenTrace = (traceId: string) => {
    setSelectedTraceId(traceId)
    setSidePanelTab('trace')
    dockStash.selectTab(dockThreadKey, 'activity')
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

  const toggleTurnSkill = (slug: string) => {
    setTurnSkillSlugs((prev) =>
      prev.includes(slug) ? prev.filter((item) => item !== slug) : [...prev, slug],
    )
  }

  const handleSend = async (overrideText?: string, bypassQueue = false) => {
    const baseText = (overrideText ?? composerText).trim()
    if (!baseText) return
    if (sending && !bypassQueue) {
      setQueuedMessages((prev) => [
        ...prev,
        {
          id: `q_${Date.now()}_${Math.random().toString(36).slice(2, 7)}`,
          content: baseText,
          steerable: true,
        },
      ])
      if (!overrideText) {
        setComposerText('')
        setGhostHint('')
      }
      return
    }
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
    const activeSkillsSnapshot = [...turnSkillSlugs]
    const responderSnapshot = activeResponderSlug
    const consultPartnersSnapshot = [...consultPartnerSlugs]
    if (!overrideText) {
      setComposerText('')
      setGhostHint('')
    }
    if (!applySkillEveryTurn && activeSkillsSnapshot.length > 0) {
      setTurnSkillSlugs([])
    }
    if (!prefsEveryTurn && responderSnapshot) {
      setActiveResponderSlug(null)
    }
    if (!partnersEveryTurn && consultPartnersSnapshot.length > 0) {
      setConsultPartnerSlugs([])
    }
    await sendTurn(raw, {
      mentions,
      skillSlugs: activeSkillsSnapshot,
      modelOverride: turnModelOverride || null,
      chatMode: chatMode || 'chat',
      personaStyle,
      responderAgentSlug: responderSnapshot,
      consultPartnerSlugs: consultPartnersSnapshot,
    })
  }

  useEffect(() => {
    if (sending || queuedMessages.length === 0) return
    const [nextItem, ...rest] = queuedMessages
    setQueuedMessages(rest)
    void handleSend(nextItem.content, true)
  }, [sending, queuedMessages])

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

  const handleExportThreadMd = () => {
    if (!activeThread) return
    const lines: string[] = [
      `# ${activeThread.title}`,
      '',
      ...messages.map((m) => {
        const who =
          m.role === 'user'
            ? uiLang === 'vi'
              ? 'Biên tập viên'
              : 'Editor'
            : `@${m.sender_slug ?? 'analyst'}`
        return `### ${who}\n\n${m.content}`
      }),
    ]
    if (whiteboard.length > 0) {
      lines.push('', `## ${c.whiteboardTitle}`, ...whiteboard.map((w) => `- ${w.content}`))
    }
    const blob = new Blob([lines.join('\n\n')], { type: 'text/markdown' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `newsman-thread-${activeThread.id}.md`
    a.click()
    URL.revokeObjectURL(url)
  }

  const declaredAgentsV2 = useMemo<DeclaredAppAgentV2[]>(
    () =>
      agents.map((ag) => ({
        id: ag.slug,
        name: ag.display_name,
        kind: ag.runtime_kind === 'partner_fast' ? 'partner' : 'subagent',
        description: ag.description.slice(0, 80),
        emoji: ag.emoji,
        color: ag.color,
      })),
    [agents],
  )

  const composerModesV2 = useMemo<ComposerModeOptionV2[]>(
    () => [
      {
        value: 'morning_brief',
        title: c.v23ModeMorningBrief,
        description: c.v23ModeMorningBriefSub,
        icon: '☀️',
      },
      {
        value: 'macro_rates',
        title: c.v23ModeMacroRates,
        description: c.v23ModeMacroRatesSub,
        icon: '📈',
      },
      {
        value: 'tax_compliance',
        title: c.v23ModeTaxCheck,
        description: c.v23ModeTaxCheckSub,
        icon: '⚖️',
      },
      {
        value: 'cross_source',
        title: c.v23ModeCrossSource,
        description: c.v23ModeCrossSourceSub,
        icon: '🌐',
      },
    ],
    [
      c.v23ModeCrossSource,
      c.v23ModeCrossSourceSub,
      c.v23ModeMacroRates,
      c.v23ModeMacroRatesSub,
      c.v23ModeMorningBrief,
      c.v23ModeMorningBriefSub,
      c.v23ModeTaxCheck,
      c.v23ModeTaxCheckSub,
    ],
  )

  const responseStylesV2 = useMemo<ResponseStyleOptionV2[]>(
    () => [
      {
        id: 'analytical',
        name: uiLang === 'vi' ? 'Phân tích dẫn nguồn' : 'Analytical & Cited',
        description:
          uiLang === 'vi' ? 'Đối chiếu số liệu & nguồn gốc' : 'Data-backed with source links',
      },
      {
        id: 'concise',
        name: uiLang === 'vi' ? 'Ngắn gọn · Điểm chính' : 'Concise Bullet Brief',
        description: uiLang === 'vi' ? 'Tóm lược ý cốt lõi' : 'Tight executive bullets',
      },
      {
        id: 'executive',
        name: uiLang === 'vi' ? 'Tổng thuật Tòa soạn' : 'Executive Memo',
        description:
          uiLang === 'vi' ? 'Bối cảnh, kịch bản & rủi ro' : 'Context, scenarios & key risks',
      },
      {
        id: 'neutral',
        name: uiLang === 'vi' ? 'Trung lập khách quan' : 'Neutral Wire Style',
        description:
          uiLang === 'vi' ? 'Tách biệt sự kiện và nhận định' : 'Strict fact vs inference split',
      },
    ],
    [uiLang],
  )

  const resourceCategoriesV2 = useMemo<ResourceCategoryItemV2[]>(
    () => [
      {
        key: 'articles',
        label: `📰 ${c.v23ResArticles}`,
        group: 'reference',
      },
      {
        key: 'regions',
        label: `🌍 ${c.v23ResRegions}`,
        group: 'reference',
      },
      {
        key: 'series',
        label: `📊 ${c.v23ResSeries}`,
        group: 'reference',
      },
      {
        key: 'tax_calendar',
        label: `📅 ${c.v23ResTax}`,
        group: 'reference',
      },
      {
        key: 'history',
        label: `💬 ${c.v23ResHistory}`,
        group: 'reference',
        selectedCount: threads.length > 0 ? threads.length : undefined,
      },
      {
        key: 'desk_notes',
        label: `📝 ${c.v23ResNotes}`,
        group: 'reference',
        selectedCount: whiteboard.length > 0 ? whiteboard.length : undefined,
      },
      {
        key: 'active_desk_scope',
        label: `${activeWorkspace?.emoji ?? '🗂️'} ${activeWorkspace?.name ?? c.salonScopeNoWorkspace}`,
        group: 'linked',
        fullRow: true,
        summary: effectiveThreadScope.regions?.join(', ') || c.salonScopeRegionsAll,
      },
    ],
    [
      activeWorkspace?.emoji,
      activeWorkspace?.name,
      c.salonScopeNoWorkspace,
      c.salonScopeRegionsAll,
      c.v23ResArticles,
      c.v23ResHistory,
      c.v23ResNotes,
      c.v23ResRegions,
      c.v23ResSeries,
      c.v23ResTax,
      effectiveThreadScope.regions,
      threads.length,
      whiteboard.length,
    ],
  )

  const activeModeOption = useMemo(
    () => composerModesV2.find((m) => m.value === chatMode) ?? null,
    [chatMode, composerModesV2],
  )

  const activeResponderAgent = useMemo(
    () => (activeResponderSlug ? agentMap.get(activeResponderSlug) ?? null : null),
    [activeResponderSlug, agentMap],
  )

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
      label: c.v22SettingsModelsNav,
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

  const providerDotStatus: ProviderStatusDot =
    bridgeStatus.mode === 'disconnected' ? 'error' : 'connected'

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
    for (const ms of manifest?.skills ?? []) {
      if (!map.has(ms.slug)) {
        map.set(ms.slug, {
          slug: ms.slug,
          name: ms.name,
          description: ms.description,
          source: ms.source,
          slash_command: ms.slash_command ?? `/${ms.slug}`,
          skill_md: ms.skill_md,
          tags: ms.tags,
          pinned: Boolean(ms.pinned),
        })
      }
    }
    return Array.from(map.values())
  }, [customSkills, grantedSkills, manifest?.skills])

  const selectedSkillOptionsV2 = useMemo<SkillOptionV2[]>(
    () =>
      turnSkillSlugs.map(
        (slug) =>
          allSkillOptionsV2.find((s) => s.slug === slug) ?? {
            slug,
            name: slug,
            description: '',
            source: 'app' as const,
            slash_command: `/${slug}`,
          },
      ),
    [allSkillOptionsV2, turnSkillSlugs],
  )

  const pinnedSkillSlugs = useMemo(
    () => allSkillOptionsV2.filter((s) => s.pinned).map((s) => s.slug),
    [allSkillOptionsV2],
  )

  const turnOutlineEntries = useMemo<TurnOutlineEntryV2[]>(() => {
    return messages
      .filter((m) => m.role === 'user')
      .map((m) => ({
        key: String(m.id),
        prompt: m.content.trim(),
        mode: chatMode !== 'chat' ? chatMode : undefined,
        weight: Math.min(1, Math.max(0.25, m.content.length / 280)),
      }))
  }, [chatMode, messages])

  const deskResponders = useMemo<SideChatResponderV2[]>(
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

  const activeCatalogEntry = useMemo(() => {
    const activeModelId = turnModelOverride || roleSummary.smartModel
    return modelCatalog.find((m) => m.model_id === activeModelId) ?? modelCatalog[0] ?? null
  }, [modelCatalog, roleSummary.smartModel, turnModelOverride])

  const contextBudget = useMemo(() => {
    const activeModelId = turnModelOverride || roleSummary.smartModel
    const windowTokens =
      activeCatalogEntry?.detected_context_window ?? activeCatalogEntry?.context_window ?? 128000
    const systemText = seatOrder
      .map((a) => `${a.soul_md}\n${a.app_role_addendum_md}` || a.description)
      .join('\n')
    const skillsText = selectedSkillOptionsV2
      .map((s) => s.skill_md || s.description || s.name)
      .join('\n')
    const memoryText = [
      notesDraft,
      safeMemoryGraph.l1_scratchpad,
      ...safeMemoryGraph.nodes.map((n) => `${n.title}: ${n.content}`),
    ]
      .filter(Boolean)
      .join('\n')
    const sourcesText = [
      ...(effectiveThreadScope.regions ?? []),
      ...(effectiveThreadScope.topics ?? []),
      turnScopeFilter.region,
      turnScopeFilter.series,
      turnScopeFilter.topic,
      ...whiteboard.map((w) => w.content),
    ]
      .filter(Boolean)
      .join(' · ')
    const conversationText = messages.map((m) => m.content).join('\n')
    const enabledMcpCount = mcpNamespaces.filter((n) => n.enabled).length
    return buildContextBudgetV2({
      window: windowTokens,
      model: activeModelId,
      systemText,
      appToolCount: 8,
      mcpToolCount: Math.max(enabledMcpCount * 4, allowedMcpTools.length),
      skillsText,
      memoryText,
      sourcesText,
      conversationText,
      composerDraft: composerText,
    })
  }, [
    activeCatalogEntry,
    allowedMcpTools.length,
    composerText,
    effectiveThreadScope.regions,
    effectiveThreadScope.topics,
    mcpNamespaces,
    messages,
    notesDraft,
    roleSummary.smartModel,
    safeMemoryGraph.l1_scratchpad,
    safeMemoryGraph.nodes,
    seatOrder,
    selectedSkillOptionsV2,
    turnModelOverride,
    turnScopeFilter.region,
    turnScopeFilter.series,
    turnScopeFilter.topic,
    whiteboard,
  ])

  const sessionActivity = useMemo(() => {
    const toolNames: string[] = []
    const citedIds = new Set<number>()
    for (const m of messages) {
      for (const tc of m.tool_calls ?? []) {
        toolNames.push(tc.tool)
      }
      if (!m.tool_calls?.length && m.trace_steps?.length) {
        for (const st of m.trace_steps) {
          if (typeof st.tool === 'string') toolNames.push(st.tool)
        }
      }
      const matches = m.content.matchAll(/#article-(\d+)/gi)
      for (const match of matches) {
        const num = Number(match[1])
        if (Number.isFinite(num) && num > 0) citedIds.add(num)
      }
    }
    for (const pill of streamState.toolPills) {
      toolNames.push(pill.tool)
    }

    const kbs: string[] = ['Newsman FTS5 Reader', 'Declared Tax Calendar']
    for (const ns of mcpNamespaces) {
      if (ns.enabled) kbs.push(`MCP · ${ns.namespace}`)
    }

    const generatedFiles: SessionArtifactFileV2[] = []
    for (const artId of citedIds) {
      generatedFiles.push({
        id: `article-${artId}`,
        filename: `Article #${artId}.md`,
        ext: 'md',
        previewText: `# Cited Article #${artId}\n\nReferenced inside thread “${activeThread?.title ?? 'Newsroom Desk'}”.`,
      })
    }
    for (const draft of cwHistory.slice(0, 4)) {
      generatedFiles.push({
        id: `cw-${draft.id}`,
        filename: `${draft.title} (${draft.status}).md`,
        ext: 'md',
        previewText: `## ${draft.title}\n\n${draft.after_text}`,
      })
    }
    for (const wb of whiteboard.slice(0, 5)) {
      generatedFiles.push({
        id: `wb-${wb.id}`,
        filename: `Takeaway #${wb.id} (@${wb.author_slug ?? 'owner'}).md`,
        ext: 'md',
        previewText: wb.content,
      })
    }

    const attachments: SessionArtifactFileV2[] = []
    if (activeThread?.article_id) {
      attachments.push({
        id: `scoped-art-${activeThread.article_id}`,
        filename: `Scoped Article #${activeThread.article_id}.md`,
        ext: 'md',
        previewText: `Thread is anchored to Newsman Article #${activeThread.article_id}.`,
      })
    }

    return buildSessionActivityV2({
      toolNames,
      knowledgeBases: kbs,
      personas: seatOrder.map((ag) => ({
        slug: ag.slug,
        name: `${ag.emoji} ${ag.display_name}`,
        role: ag.description,
      })),
      regions: effectiveThreadScope.regions?.length
        ? effectiveThreadScope.regions
        : [c.salonScopeRegionsAll],
      topics: effectiveThreadScope.topics?.length
        ? effectiveThreadScope.topics.map((t) => `#${t}`)
        : [c.salonScopeTopicsAll],
      memoryItems:
        safeMemoryGraph.nodes.length > 0
          ? safeMemoryGraph.nodes.slice(0, 4).map((n) => ({
              id: n.id,
              title: n.title,
              layer: n.layer,
            }))
          : [{ id: 'l1', title: notesDraft ? 'L1 Scratchpad active' : 'L1–L3 ready', layer: 'L1' }],
      generatedFiles,
      attachments,
    })
  }, [
    activeThread?.article_id,
    activeThread?.title,
    c.salonScopeRegionsAll,
    c.salonScopeTopicsAll,
    cwHistory,
    effectiveThreadScope.regions,
    effectiveThreadScope.topics,
    mcpNamespaces,
    messages,
    notesDraft,
    safeMemoryGraph.nodes,
    seatOrder,
    streamState.toolPills,
    whiteboard,
  ])

  const handleProseMouseUp = (msg: AgentMessage) => {
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
        id: `msg-${msg.id}`,
        quote: quote.slice(0, 600),
        sourceTitle: msg.sender_slug ? `@${msg.sender_slug}` : 'assistant',
      },
      coords: {
        x: rect.left + rect.width / 2,
        y: rect.top,
      },
    })
  }

  const handleQuoteIntoComposer = (quote: string) => {
    const clean = quote
      .split('\n')
      .map((line) => `> ${line}`)
      .join('\n')
    setComposerText((prev) => (prev.trim() ? `${clean}\n\n${prev}` : `${clean}\n\n`))
    setSelectionBar(null)
    window.getSelection()?.removeAllRanges()
    composerRef.current?.focus()
  }

  const handleAskWithResponder = (anchor: SelectionAnchorV2, responderSlug?: string) => {
    const targetSlug = responderSlug || selectedResponderSlug || seatOrder[0]?.slug || 'macro-argus'
    const prompt = `@${targetSlug} ${
      uiLang === 'vi' ? 'Phân tích đoạn trích này:' : 'Analyze this excerpt:'
    }\n> ${anchor.quote.replace(/\n+/g, ' ')}`
    setSelectionBar(null)
    window.getSelection()?.removeAllRanges()
    void handleSend(prompt)
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
      ...(connForm.astraea_token ? { astraea_token: connForm.astraea_token } : {}),
    })
      .then(() => {
        setConnForm((p) => ({
          ...p,
          astraea_token: '',
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
        <nav className="pplx-tabs ak-tab-strip" role="tablist" aria-label={c.agentsWorkspaceTitle}>
          {surfaceGate.surfaces.map((id) => (
            <button
              key={id}
              type="button"
              role="tab"
              aria-selected={surface === id}
              className={surface === id ? 'pplx-tab ak-tab-btn active' : 'pplx-tab ak-tab-btn'}
              onClick={() => setSurface(id)}
            >
              {surfaceLabelMap[id]}
            </button>
          ))}
        </nav>
        <div className="ak-btn-group">
          <button
            type="button"
            className="ak-top-pill-btn"
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
            className="ak-top-pill-select"
            ariaLabel={c.workspaceActiveLabel}
            value={activeWorkspaceId ?? ''}
            onChange={(v) => setActiveWorkspaceId(v || null)}
            options={[
              { value: '', label: c.workspaceAll },
              ...workspaces.map((ws) => ({ value: ws.id, label: `${ws.emoji} ${ws.name}` })),
            ]}
          />
          <a href="#mcp-connectors" className="ak-top-pill-link">
            🔌 {c.mcpConnectorsShort}
          </a>
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
          <div className="pplx-tabs ak-subtabs">
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
                className={studioSubTab === subId ? 'pplx-tab ak-tab-btn active' : 'pplx-tab ak-tab-btn'}
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
                            {uiLang === 'vi'
                              ? 'Nhà cung cấp AI (LNC Agents Hub & OpenAI tương thích), API key và vai trò Smart/Fast Model được cấu hình thống nhất trong Nhà cung cấp AI.'
                              : 'LNC Agents Hub credentials, custom LLM providers, and Smart/Fast model roles are managed centrally under LLM Providers.'}
                          </p>
                        </div>
                        <span className="tier-badge">{statusLabel}</span>
                      </div>

                      <div className="chip-row" style={{ margin: '10px 0' }}>
                        <span className="chip">
                          Hub: <strong>{bridgeStatus.hub_url || '—'}</strong>
                        </span>
                        <span className="chip">
                          Smart: <strong>{roleSummary.smartModel}</strong>
                        </span>
                        <span className="chip">
                          Fast: <strong>{roleSummary.fastModel}</strong>
                        </span>
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
                        <a className="btn primary btn-sm" href="#llm-providers">
                          {uiLang === 'vi' ? 'Cấu hình Nhà cung cấp AI →' : 'Configure LLM Providers →'}
                        </a>
                        <a className="btn secondary btn-sm" href="#mcp-connectors">
                          {uiLang === 'vi' ? 'Trình kết nối MCP →' : 'MCP Connectors →'}
                        </a>
                        <button
                          type="button"
                          className="btn secondary btn-sm"
                          disabled={probingConnections}
                          onClick={() => void probeConnections()}
                        >
                          {probingConnections ? c.modelsProbing : `🔌 ${c.modelsProbeBtn}`}
                        </button>
                        <button
                          type="button"
                          className="btn secondary btn-sm"
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
                      open={Boolean(connForm.astraea_url)}
                    >
                      <summary style={{ cursor: 'pointer', fontWeight: 600 }}>
                        {uiLang === 'vi'
                          ? 'Ghi đè Astraea Relay & Host Allowlist'
                          : 'Astraea Relay Overrides & Allowed Hosts'}
                      </summary>
                      <p className="muted-sub" style={{ marginTop: 6 }}>
                        {uiLang === 'vi'
                          ? 'Chỉ dùng khi cần trỏ riêng Astraea WS/HTTP Relay hoặc thêm host nội bộ vào allowlist.'
                          : 'Only needed to override the Astraea WS/HTTP relay target or allow additional internal hosts.'}
                      </p>

                      <div className="chip-row" style={{ margin: '8px 0' }}>
                        <span className={bridgeStatus.has_astraea_token ? 'bp-chip ok' : 'bp-chip muted'}>
                          Astraea: {bridgeStatus.has_astraea_token ? c.bridgeTokenSaved : 'unset'}
                        </span>
                        <a className="btn secondary btn-sm" href="#mcp-connectors">
                          {uiLang === 'vi' ? 'Quản lý tại Trình kết nối MCP ↗' : 'Manage in MCP Connectors ↗'}
                        </a>
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
                    </details>
                  </>
                )}

                {settingsTab === 'models' && (
                  <>
                    <section className="studio-section">
                      <div className="ak-toolbar">
                        <div>
                          <h4>{c.v22SettingsModelsNav}</h4>
                          <p className="muted-sub" style={{ marginTop: 2 }}>
                            {uiLang === 'vi'
                              ? 'Chọn vai trò Smart/Fast Model, nhập hàng loạt danh mục và ghi đè năng lực 3 trạng thái.'
                              : 'Configure Smart/Fast model roles, bulk-import catalog entries, and set tri-state capability overrides.'}
                          </p>
                        </div>
                        <div className="ak-btn-group">
                          <button
                            type="button"
                            className="btn secondary btn-sm"
                            disabled={fetchingCatalog}
                            onClick={() => {
                              setFetchingCatalog(true)
                              setConnError('')
                              void fetchModelCatalog()
                                .then((res) =>
                                  setConnNotice(
                                    `${c.v2FetchCatalogBtn}: ${res.fetched_count ?? res.items?.length ?? 0}`,
                                  ),
                                )
                                .catch((err) =>
                                  setConnError(err instanceof Error ? err.message : 'Fetch failed'),
                                )
                                .finally(() => setFetchingCatalog(false))
                            }}
                          >
                            {fetchingCatalog ? c.modelsProbing : `↻ ${c.v2FetchCatalogBtn}`}
                          </button>
                          <button
                            type="button"
                            className="btn primary btn-sm"
                            onClick={() => setBulkImportOpen(true)}
                          >
                            {c.v22BulkImportBtn}
                          </button>
                        </div>
                      </div>

                      <div className="catalog-form-row" style={{ marginTop: 10 }}>
                        <div className="field">
                          <span>{c.v2SmartModelLabel}</span>
                          <AgentSelect
                            ariaLabel={c.v2SmartModelLabel}
                            value={smartRoleDraft}
                            onChange={setSmartRoleDraft}
                            options={selectableModels.map((m) => ({
                              value: m,
                              label: m,
                              dots: [providerDotStatus],
                            }))}
                          />
                        </div>
                        <div className="field">
                          <span>{c.v2FastModelLabel}</span>
                          <AgentSelect
                            ariaLabel={c.v2FastModelLabel}
                            value={fastRoleDraft}
                            onChange={setFastRoleDraft}
                            options={selectableModels.map((m) => ({
                              value: m,
                              label: m,
                              dots: [providerDotStatus],
                            }))}
                          />
                        </div>
                      </div>
                      <div className="btn-row" style={{ marginTop: 8 }}>
                        <button
                          type="button"
                          className="btn primary btn-sm"
                          onClick={() => {
                            setConnError('')
                            void saveModelRoles({
                              default_smart_model: smartRoleDraft,
                              fast_background_model: fastRoleDraft,
                            })
                              .then(() => setConnNotice(c.v2SaveRolesBtn))
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
                        <strong>
                          Catalog ({modelCatalog.filter((m) => m.enabled).length}/{modelCatalog.length})
                        </strong>
                      </div>
                      <div style={{ display: 'flex', flexDirection: 'column', gap: 8, marginTop: 10 }}>
                        {modelCatalog.map((entry) => {
                          const overrides = entry.capability_overrides ?? {}
                          const ctxWin = entry.detected_context_window ?? entry.context_window ?? 128000
                          const triOptions: Array<{ value: CapabilityTriState; label: string }> = [
                            { value: 'auto', label: c.v22CapOverrideAuto },
                            { value: 'enabled', label: c.v22CapOverrideEnabled },
                            { value: 'disabled', label: c.v22CapOverrideDisabled },
                          ]
                          const updateOverride = (
                            capKey: 'reasoning' | 'vision' | 'tools',
                            nextVal: CapabilityTriState,
                          ) => {
                            const nextOverrides = { ...overrides, [capKey]: nextVal }
                            void upsertCatalogModel({
                              provider_id: entry.provider_id,
                              model_id: entry.model_id,
                              display_name: entry.display_name,
                              context_window: entry.context_window,
                              enabled: entry.enabled,
                              capability_overrides: nextOverrides,
                            })
                          }
                          return (
                            <div
                              key={`${entry.provider_id}:${entry.model_id}`}
                              className="probe-card"
                              style={{ display: 'flex', flexDirection: 'column', gap: 8 }}
                            >
                              <div className="probe-card-head">
                                <div>
                                  <strong>{entry.display_name || entry.model_id}</strong>
                                  <div className="muted-sub">
                                    <code>{entry.model_id}</code> · {entry.provider_id} · ctx:{' '}
                                    {Math.round(ctxWin / 1000)}k
                                    {entry.last_test_fingerprint ? (
                                      <>
                                        {' '}
                                        · sha256:{' '}
                                        <code>{entry.last_test_fingerprint.slice(0, 10)}</code>
                                      </>
                                    ) : null}
                                  </div>
                                </div>
                                <button
                                  type="button"
                                  className={entry.enabled ? 'btn small primary' : 'btn small secondary'}
                                  onClick={() =>
                                    void toggleCatalogModels([
                                      { model_id: entry.model_id, enabled: !entry.enabled },
                                    ])
                                  }
                                >
                                  {entry.enabled ? c.skillsEnableToggle : c.skillsDisableToggle}
                                </button>
                              </div>
                              <div className="catalog-form-row">
                                {(['reasoning', 'vision', 'tools'] as const).map((capKey) => (
                                  <div key={capKey} className="field">
                                    <span>{capKey}</span>
                                    <AgentSelect
                                      size="sm"
                                      ariaLabel={`${entry.model_id} ${capKey}`}
                                      value={(overrides[capKey] as CapabilityTriState | undefined) ?? 'auto'}
                                      onChange={(v) => updateOverride(capKey, v as CapabilityTriState)}
                                      options={triOptions}
                                    />
                                  </div>
                                ))}
                              </div>
                            </div>
                          )
                        })}
                      </div>
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
            left={
              <aside className="agents-rail" style={{ width: '100%', border: 0 }}>
                <div className="rail-actions">
                  <button
                    type="button"
                    className="btn primary rail-pill-btn"
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
                    className="btn secondary rail-pill-btn"
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
                <AgentChatHeaderBar
                  lang={uiLang}
                  title={
                    activeThread
                      ? `${activeThread.parent_thread_id ? '↳ ' : ''}${activeThread.title}`
                      : c.newResearchThreadTitle
                  }
                  subtitle={
                    activeResponderAgent
                      ? `${activeResponderAgent.emoji} ${activeResponderAgent.display_name} · Role-play`
                      : activeThread
                        ? `${seatOrder.map((ag) => `${ag.emoji} ${ag.display_name}`).join(' · ')}${streamConnected ? ` · ● ${c.liveStreamBadge}` : ''}`
                        : undefined
                  }
                  leftSidebarOpen={railOpen}
                  onToggleLeftSidebar={() => setRailOpen((v) => !v)}
                  rightDockOpen={sidePanelOpen}
                  onToggleRightDock={() => setSidePanelOpen((v) => !v)}
                  onOpenNoteTab={() => {
                    setSidePanelTab('context')
                    dockStash.selectTab(dockThreadKey, 'notes')
                    setSidePanelOpen(true)
                  }}
                  controlsConfig={controlsConfig}
                  onChangeControlsConfig={updateControlsConfig}
                  moreActions={
                    activeThread
                      ? [
                          {
                            key: 'pin',
                            label: activeThread.pinned ? c.v2UnpinThreadBtn : c.v2PinThreadBtn,
                            onSelect: () =>
                              void updateThread(activeThread.id, { pinned: !activeThread.pinned }),
                          },
                          {
                            key: 'rename',
                            label: c.v23RenameThreadBtn,
                            onSelect: () => {
                              const nextTitle = window.prompt(
                                c.v23RenameThreadPrompt,
                                activeThread.title,
                              )
                              if (nextTitle && nextTitle.trim()) {
                                void updateThread(activeThread.id, { title: nextTitle.trim() })
                              }
                            },
                          },
                          ...(messages.length > 0
                            ? [
                                {
                                  key: 'branch',
                                  label: c.v2BranchBtn,
                                  onSelect: () =>
                                    void handleBranchFromMessage(messages[messages.length - 1].id),
                                },
                              ]
                            : []),
                          {
                            key: 'export_md',
                            label: c.v23ExportMdBtn,
                            onSelect: handleExportThreadMd,
                          },
                          {
                            key: 'delete',
                            label: c.deleteThreadLabel,
                            danger: true,
                            onSelect: () => handleDeleteThread(activeThread),
                          },
                        ]
                      : []
                  }
                />

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

                    {activeThread.kind === 'group' && (
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

                <div className="ak-stage-stream-wrap" style={{ flex: 1, minHeight: 0, display: 'flex' }}>
                  <AgentTurnNavigator
                    entries={turnOutlineEntries}
                    scrollRootRef={scrollRootRef}
                    onJump={(turnKey) => {
                      const root = scrollRootRef.current
                      if (!root) return
                      const el = root.querySelector<HTMLElement>(`[data-turn-key="${turnKey}"]`)
                      el?.scrollIntoView({ behavior: 'smooth', block: 'start' })
                    }}
                    onJumpToBottom={() =>
                      listEndRef.current?.scrollIntoView({ behavior: 'smooth', block: 'end' })
                    }
                  />
                  <div
                    ref={scrollRootRef}
                    className="stage-messages"
                    style={{ flex: 1, minHeight: 0, overflowY: 'auto' }}
                  >
                  {messages.length === 0 && (
                    <div className="pplx-empty-hero">
                      <div className="pplx-empty-head">
                        <h3>{c.emptyHeroTitle}</h3>
                        <p>{c.emptyHeroSub}</p>
                      </div>
                      <div className="pplx-starter-grid">
                        {[
                          {
                            title: c.starterCardMacroTitle,
                            desc: c.starterCardMacroDesc,
                            prompt: c.starterCardMacroPrompt,
                          },
                          {
                            title: c.starterCardTaxTitle,
                            desc: c.starterCardTaxDesc,
                            prompt: c.starterCardTaxPrompt,
                          },
                          {
                            title: c.starterCardCowriterTitle,
                            desc: c.starterCardCowriterDesc,
                            prompt: c.starterCardCowriterPrompt,
                          },
                          {
                            title: c.starterCardFactTitle,
                            desc: c.starterCardFactDesc,
                            prompt: c.starterCardFactPrompt,
                          },
                        ].map((card) => (
                          <button
                            key={card.title}
                            type="button"
                            className="pplx-starter-card"
                            onClick={() => void handleSend(card.prompt)}
                          >
                            <strong>{card.title}</strong>
                            <span>{card.desc}</span>
                          </button>
                        ))}
                      </div>
                    </div>
                  )}

                  {messages.map((m: AgentMessage) => {
                    const isUser = m.role === 'user'
                    const ag = m.sender_slug ? agentMap.get(m.sender_slug) : undefined
                    const traceId = buildSeatTraceId(m)
                    const toolCalls = m.tool_calls ?? []
                    const traceSteps =
                      toolCalls.length > 0
                        ? toolCalls.map((tc) => mapToolToTraceStepV2(tc))
                        : (m.trace_steps ?? []).map((raw, idx) =>
                            mapToolToTraceStepV2({
                              id:
                                typeof raw.merge_id === 'string'
                                  ? raw.merge_id
                                  : `step-${m.id}-${idx}`,
                              tool: typeof raw.tool === 'string' ? raw.tool : 'tool',
                              status: typeof raw.status === 'string' ? raw.status : 'done',
                              duration_ms:
                                typeof raw.duration_ms === 'number' ? raw.duration_ms : undefined,
                            }),
                          )
                    const approvalPills = toolCalls.filter((tc) => Boolean(tc.approval_required))
                    const hasTrace = Boolean(m.reasoning) || traceSteps.length > 0
                    return (
                      <div
                        key={m.id}
                        data-turn-key={isUser ? String(m.id) : undefined}
                        className={
                          isUser
                            ? 'salon-turn salon-turn-user ak-msg--user'
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
                                  {c.v2ExecutionTrace} · {traceSteps.length} {c.traceToolsUnit} ▸
                                </button>
                              )}
                            </div>
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
                                <ToolInvocationPill key={tc.merge_id} pill={tc} lang={uiLang} />
                              ))}
                            </div>
                          )}

                          <div
                            className="salon-turn-prose"
                            onMouseUp={!isUser ? () => handleProseMouseUp(m) : undefined}
                          >
                            <AgentMarkdown content={m.content} onCitationClick={onOpenArticle} />
                          </div>

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

                  {(sending ||
                    streamState.toolPills.length > 0 ||
                    Object.keys(streamState.drafts).length > 0 ||
                    Boolean(reconnectNotice)) &&
                    (() => {
                      const draftsList = Object.values(streamState.drafts)
                      if (draftsList.length === 0) {
                        const liveSteps = streamState.toolPills.map((pill) =>
                          mapToolToTraceStepV2(pill),
                        )
                        const runningCount = liveSteps.filter((s) => s.state === 'running').length
                        const doneCount = liveSteps.length - runningCount
                        const liveApprovalPills = streamState.toolPills.filter((p) =>
                          Boolean(p.approval_required),
                        )
                        return (
                          <>
                            <AgentExecutionTrace
                              phase={deriveAgentStreamPhaseV2({
                                isLive: sending,
                                hasError: Boolean(streamState.lastError),
                                runningToolCount: runningCount,
                                doneToolCount: doneCount,
                              })}
                              steps={liveSteps}
                              responderLabel={activeResponderAgent?.display_name}
                              responderAvatar={activeResponderAgent?.emoji}
                              reconnectNotice={reconnectNotice}
                              onCitationClick={onOpenArticle}
                            />
                            {liveApprovalPills.length > 0 && (
                              <div className="salon-tool-list">
                                {liveApprovalPills.map((p) => (
                                  <ToolInvocationPill key={p.merge_id} pill={p} lang={uiLang} />
                                ))}
                              </div>
                            )}
                          </>
                        )
                      }
                      return draftsList.map((draft) => {
                        const ag = agentMap.get(draft.agent_slug)
                        const seatPills = streamState.toolPills.filter(
                          (p) => !p.agent_slug || p.agent_slug === draft.agent_slug,
                        )
                        const liveSteps = seatPills.map((p) => mapToolToTraceStepV2(p))
                        const runningCount = liveSteps.filter((s) => s.state === 'running').length
                        const doneCount = liveSteps.length - runningCount
                        return (
                          <div
                            key={draft.agent_slug}
                            className="salon-turn salon-turn-agent partner-seat streaming"
                          >
                            <div className="salon-turn-card">
                              <header className="salon-turn-meta">
                                <b>
                                  {ag?.emoji ?? '📈'} {ag?.display_name ?? draft.agent_slug}
                                </b>
                                <span className="tier-badge running">{c.seatSpeaking}</span>
                              </header>
                              <AgentExecutionTrace
                                phase={deriveAgentStreamPhaseV2({
                                  isLive: true,
                                  hasError: Boolean(streamState.lastError),
                                  hasText: Boolean(draft.text),
                                  hasReasoning: Boolean(draft.reasoning),
                                  runningToolCount: runningCount,
                                  doneToolCount: doneCount,
                                })}
                                thinkingText={draft.reasoning || undefined}
                                steps={liveSteps}
                                responderLabel={
                                  activeResponderAgent?.display_name ??
                                  ag?.display_name ??
                                  draft.agent_slug
                                }
                                responderAvatar={activeResponderAgent?.emoji ?? ag?.emoji}
                                reconnectNotice={reconnectNotice}
                                onCitationClick={onOpenArticle}
                              />
                              <div className="salon-turn-prose">
                                <AgentMarkdown
                                  content={draft.text}
                                  onCitationClick={onOpenArticle}
                                />
                              </div>
                            </div>
                          </div>
                        )
                      })
                    })()}
                  <div ref={listEndRef} />
                  </div>
                </div>

                <AgentSelectionToolbar
                  anchor={selectionBar?.anchor ?? null}
                  coords={selectionBar?.coords ?? null}
                  showListen={Boolean(caps?.voice_tts)}
                  responders={deskResponders}
                  selectedResponderSlug={selectedResponderSlug}
                  onSelectResponder={setSelectedResponderSlug}
                  onAddToChat={(anchor) => handleQuoteIntoComposer(anchor.quote)}
                  onAsk={(anchor, slug) => handleAskWithResponder(anchor, slug)}
                  onDismiss={() => setSelectionBar(null)}
                />

                {(error || streamState.lastError) && (
                  <p className="form-error">{error ?? streamState.lastError}</p>
                )}

                <AgentQueuedMessages
                  messages={queuedMessages}
                  isStreaming={sending}
                  onWithdraw={(id) =>
                    setQueuedMessages((prev) => prev.filter((item) => item.id !== id))
                  }
                />

                <footer className="composer-tray">
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

                  <AgentComposerLead
                    modeLabel={
                      activeModeOption
                        ? activeModeOption.title
                        : chatMode && chatMode !== 'chat'
                          ? chatMode
                          : null
                    }
                    onClearMode={() => {
                      setChatMode('')
                      if (activeThread) {
                        void updateThread(activeThread.id, { chat_mode: 'chat' })
                      }
                    }}
                    selectedSkills={selectedSkillOptionsV2}
                    onRemoveSkill={(slug) => toggleTurnSkill(slug)}
                    applyEveryTurn={applySkillEveryTurn}
                  />

                  <AgentFoldableComposerTray
                    storageKey="lnc-ak-composer-tray:newsman"
                    leftCluster={
                      <>
                        <AgentResourcesPopover
                          items={resourceCategoriesV2}
                          label={c.v23ResourcesBtn}
                          selectedTotal={
                            (turnScopeFilter.region.trim() ? 1 : 0) +
                            (turnScopeFilter.series.trim() ? 1 : 0) +
                            (turnScopeFilter.topic.trim() ? 1 : 0)
                          }
                          onSelectCategory={(categoryId: string) => {
                            if (categoryId === 'regions' && onOpenRegions) {
                              onOpenRegions()
                            } else if (categoryId === 'desk_notes') {
                              setSidePanelTab('context')
                              dockStash.selectTab(dockThreadKey, 'notes')
                              setSidePanelOpen(true)
                            } else if (categoryId === 'history') {
                              setRailOpen(true)
                            } else {
                              setComposerPopover((prev) => (prev === 'scope' ? null : 'scope'))
                            }
                          }}
                        />

                        {composerPopover === 'scope' && (
                          <div className="ak-popover-anchor">
                            <div className="ak-popover" role="dialog">
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
                            </div>
                          </div>
                        )}

                        <AgentModePopover
                          modes={composerModesV2}
                          value={chatMode}
                          label={c.v2ModeBtn}
                          onChange={(nextMode: string) => {
                            setChatMode(nextMode)
                            if (activeThread) {
                              void updateThread(activeThread.id, {
                                chat_mode: nextMode || 'chat',
                              })
                            }
                          }}
                        />

                        {controlsConfig.showSkills && (
                          <AgentSkillPicker
                            size="sm"
                            label={c.v2SkillsBtn}
                            searchPlaceholder={c.skillsSearchPlaceholder}
                            skills={allSkillOptionsV2}
                            selectedSlugs={turnSkillSlugs}
                            onToggleSkill={toggleTurnSkill}
                            applyEveryTurn={applySkillEveryTurn}
                            onToggleApplyEveryTurn={setApplySkillEveryTurn}
                            pinnedSlugs={pinnedSkillSlugs}
                            onTogglePin={(slug) =>
                              void toggleSkillPin(slug, !pinnedSkillSlugs.includes(slug))
                            }
                            onCreateSkill={() => setCreateSkillOpen(true)}
                            onDeleteCustomSkill={(slug) => void deleteCustomSkill(slug)}
                          />
                        )}

                        {controlsConfig.showMcp && (
                          <div className="ak-popover-anchor">
                            <button
                              type="button"
                              className={`ak-tray-pill${composerPopover === 'mcp' ? ' is-active' : ''}`}
                              onClick={() =>
                                setComposerPopover((prev) => (prev === 'mcp' ? null : 'mcp'))
                              }
                            >
                              <span>{c.v2McpBtn}</span>
                              <span className="ak-tray-chev">▾</span>
                            </button>
                            {composerPopover === 'mcp' && (
                              <div className="ak-popover" role="dialog">
                                <div className="chip-row">
                                  {mcpNamespaces.map((ns) => (
                                    <span
                                      key={ns.namespace}
                                      className={ns.enabled ? 'chip active' : 'chip'}
                                    >
                                      {ns.enabled ? '✓ ' : ''}
                                      {ns.namespace}
                                    </span>
                                  ))}
                                </div>
                              </div>
                            )}
                          </div>
                        )}
                      </>
                    }
                    rightCluster={
                      <>
                        {controlsConfig.showPartners && (
                          <AgentPartnersConsultPopover
                            agents={declaredAgentsV2}
                            selectedIds={consultPartnerSlugs}
                            everyTurn={partnersEveryTurn}
                            onToggleAgent={(slug: string) =>
                              setConsultPartnerSlugs((prev) =>
                                prev.includes(slug)
                                  ? prev.filter((s) => s !== slug)
                                  : [...prev, slug],
                              )
                            }
                            onEveryTurnChange={setPartnersEveryTurn}
                            label={c.v23PartnersBtn}
                            onCreateGroup={() => {
                              setNewRoomKind('group')
                              setNewRoomMembers(
                                activeWorkspace?.default_agent_slugs?.length
                                  ? activeWorkspace.default_agent_slugs
                                  : agents.map((a) => a.slug),
                              )
                              setNewRoomOpen(true)
                              setRailOpen(true)
                            }}
                            onManageAgents={() => {
                              setSurface('studio')
                              setStudioSubTab('agents')
                            }}
                          />
                        )}

                        {controlsConfig.showPreferences && (
                          <AgentRoleplayPreferencesPopover
                            styles={responseStylesV2}
                            selectedStyleId={personaStyle}
                            onSelectStyle={(nextStyle: string) => {
                              const resolved = nextStyle || 'analytical'
                              setPersonaStyle(resolved)
                              if (activeThread) {
                                void updateThread(activeThread.id, { persona_style: resolved })
                              }
                            }}
                            declaredAgents={declaredAgentsV2}
                            activeResponderId={activeResponderSlug}
                            onSelectResponder={setActiveResponderSlug}
                            everyTurn={prefsEveryTurn}
                            onEveryTurnChange={setPrefsEveryTurn}
                            label={c.v23PreferencesBtn}
                            onOpenPersonalization={() => {
                              setSurface('studio')
                              setStudioSubTab('agents')
                            }}
                          />
                        )}
                      </>
                    }
                  />

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

                  <div
                    className="composer-tray-bar"
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      justifyContent: 'space-between',
                      gap: 8,
                      flexWrap: 'wrap',
                    }}
                  >
                    <div
                      className="composer-tray-left"
                      style={{ display: 'inline-flex', alignItems: 'center', gap: 6, flexWrap: 'wrap' }}
                    >
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
                          {
                            value: '',
                            label: `${c.composerModelDefault} (${roleSummary.smartModel})`,
                            dots: [providerDotStatus],
                          },
                          ...selectableModels.map((m) => ({
                            value: m,
                            label: m,
                            dots: [providerDotStatus],
                          })),
                        ]}
                      />

                      {controlsConfig.showContextRing && (
                        <AgentContextBudgetChip budget={contextBudget} />
                      )}

                      {composerBytes > 0 && (
                        <span className="context-budget-chip" title={c.composerBudgetLabel}>
                          {composerBytes < 1024 ? '<1' : (composerBytes / 1024).toFixed(1)} / 16 KB
                        </span>
                      )}
                    </div>

                    <div className="composer-tray-right">
                      <button
                        type="button"
                        className="pplx-submit-btn"
                        aria-label={sending ? c.sendingTurnBtn : c.sendTurnBtn}
                        title={sending ? c.sendingTurnBtn : c.sendTurnBtn}
                        disabled={!composerText.trim()}
                        onClick={() => void handleSend()}
                      >
                        <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                          <line x1="12" y1="19" x2="12" y2="5" />
                          <polyline points="5 12 12 5 19 12" />
                        </svg>
                      </button>
                    </div>
                  </div>
                </footer>
              </section>
            }
            dock={
              sidePanelOpen ? (
                <AgentWorkspaceDock
                  storageKey="newsman:chat-dock"
                  threadId={dockThreadKey}
                  open={sidePanelOpen}
                  onCloseDock={() => setSidePanelOpen(false)}
                  onBackToConversation={
                    isCompactViewport ? () => setSidePanelOpen(false) : undefined
                  }
                  backLabel={c.v22DockBackBtn}
                  tabs={dockState.tabs}
                  activeTabId={dockState.activeId}
                  onSelectTab={(tabId) => {
                    dockStash.selectTab(dockThreadKey, tabId)
                    setSidePanelTab(tabId === 'notes' ? 'context' : 'trace')
                  }}
                  onCloseTab={(tabId) => dockStash.closeTab(dockThreadKey, tabId)}
                  launcherOptions={[
                    {
                      kind: 'activity',
                      label: `📈 ${c.v2DrawerActivity}`,
                      targetId: 'activity',
                    },
                    {
                      kind: 'notes',
                      label: `📝 ${c.v2DrawerNotes}`,
                      targetId: 'notes',
                    },
                    {
                      kind: 'custom',
                      label: '✦ Summarize Round',
                      targetId: 'prompt:summarize',
                    },
                  ]}
                  onLaunchTab={(opt) => {
                    if (opt.kind === 'activity') dockStash.selectTab(dockThreadKey, 'activity')
                    else if (opt.kind === 'notes') dockStash.selectTab(dockThreadKey, 'notes')
                    else if (opt.targetId === 'prompt:summarize') {
                      setComposerText(
                        uiLang === 'vi'
                          ? 'Tóm tắt các điểm chính và rủi ro thuế/vĩ mô của phiên thảo luận này.'
                          : 'Summarize the key macro and tax takeaways from this discussion round.',
                      )
                      composerRef.current?.focus()
                    }
                  }}
                  recentlyClosed={dockState.closed}
                  onReopenClosed={(tab) => dockStash.reopenClosed(dockThreadKey, tab)}
                  onPromptTrigger={(prompt) => {
                    setComposerText(prompt)
                    composerRef.current?.focus()
                  }}
                  renderTabBody={(tab) => {
                    if (tab.id === 'activity' || tab.kind === 'activity') {
                      return (
                        <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
                          <AgentSessionActivity
                            activity={sessionActivity}
                            onOpenFile={(file) => {
                              dockStash.openFileTab(dockThreadKey, file)
                              setSidePanelOpen(true)
                            }}
                            onNavigateSpace={(href) => {
                              if (href === '#regions' && onOpenRegions) {
                                onOpenRegions()
                              } else if (href === '#memory') {
                                setSurface('memory')
                              } else if (href === '#studio') {
                                setSurface('studio')
                              }
                            }}
                            configSection={
                              traceEntries.length > 0 ? (
                                <div className="side-panel-trace-list">
                                  {traceEntries.map(({ traceId, message, agent }) => (
                                    <article
                                      key={traceId}
                                      data-trace-id={traceId}
                                      className={
                                        selectedTraceId === traceId ? 'trace-card active' : 'trace-card'
                                      }
                                    >
                                      <header className="trace-card-head">
                                        <strong>
                                          {agent?.emoji ?? '📈'}{' '}
                                          {agent?.display_name ?? message.sender_slug ?? 'Agent'}
                                        </strong>
                                        {message.round_idx && (
                                          <span className="round-tag">R{message.round_idx}</span>
                                        )}
                                      </header>
                                      <AgentExecutionTrace
                                        phase="settled"
                                        thinkingText={message.reasoning ?? undefined}
                                        steps={(message.tool_calls ?? []).map((tc) =>
                                          mapToolToTraceStepV2(tc),
                                        )}
                                        responderLabel={
                                          agent?.display_name ?? message.sender_slug ?? undefined
                                        }
                                        responderAvatar={agent?.emoji}
                                        onCitationClick={onOpenArticle}
                                      />
                                    </article>
                                  ))}
                                </div>
                              ) : undefined
                            }
                          />
                        </div>
                      )
                    }

                    if (tab.id === 'notes' || tab.kind === 'notes') {
                      return (
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
                                    void updateThread(activeThread.id, {
                                      scratchpad_md: notesDraft,
                                    }).finally(() => setNotesSaving(false))
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
                              {notesDraft.trim() && (
                                <div style={{ marginTop: 8 }}>
                                  <AgentMarkdown
                                    content={notesDraft}
                                    onCitationClick={onOpenArticle}
                                  />
                                </div>
                              )}
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
                                    {note.pinned && (
                                      <span className="tier-badge">{c.whiteboardPinnedBadge}</span>
                                    )}{' '}
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
                                <AgentMarkdown
                                  content={note.content}
                                  onCitationClick={onOpenArticle}
                                />
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
                      )
                    }

                    const artMatch = /^file:(?:scoped-art-|article-)(\d+)$/.exec(tab.id)
                    const metaFile = tab.meta?.file as SessionArtifactFileV2 | undefined
                    const previewBody = String(metaFile?.previewText ?? '')
                    return (
                      <div
                        className="studio-section"
                        style={{ display: 'flex', flexDirection: 'column', gap: 10 }}
                      >
                        <strong>{tab.title}</strong>
                        {previewBody ? (
                          <div className="skill-playbook-body">
                            <AgentMarkdown content={previewBody} onCitationClick={onOpenArticle} />
                          </div>
                        ) : (
                          <p className="muted-sub">—</p>
                        )}
                        {artMatch && onOpenArticle && (
                          <div className="btn-row">
                            <button
                              type="button"
                              className="btn primary btn-sm"
                              onClick={() => onOpenArticle(Number(artMatch[1]))}
                            >
                              📰 {c.companionJumpArticle} #{artMatch[1]}
                            </button>
                          </div>
                        )}
                      </div>
                    )
                  }}
                />
              ) : undefined
            }
          />
        </div>
      )}

      {createSkillOpen && (
        <div className="modal-backdrop" role="dialog" aria-label={c.v22CreateSkillTitle}>
          <div className="modal-card">
            <h3>{c.v22CreateSkillTitle}</h3>
            <div className="catalog-form-row">
              <label className="field">
                <span>{c.v22CreateSkillSlugLabel}</span>
                <input
                  value={createSkillForm.slug}
                  placeholder="macro-flash-digest"
                  onChange={(e) =>
                    setCreateSkillForm((p) => ({
                      ...p,
                      slug: e.target.value.toLowerCase().replace(/[^a-z0-9-_]/g, '-'),
                    }))
                  }
                />
              </label>
              <label className="field">
                <span>{c.v22CreateSkillNameLabel}</span>
                <input
                  value={createSkillForm.name}
                  placeholder="Macro Flash Digest"
                  onChange={(e) => setCreateSkillForm((p) => ({ ...p, name: e.target.value }))}
                />
              </label>
            </div>
            <label className="field">
              <span>{c.v22CreateSkillDescLabel}</span>
              <input
                value={createSkillForm.description}
                onChange={(e) => setCreateSkillForm((p) => ({ ...p, description: e.target.value }))}
              />
            </label>
            <label className="field">
              <span>{c.v22CreateSkillMdLabel}</span>
              <textarea
                rows={6}
                value={createSkillForm.skill_md}
                placeholder="# Instructions\n1. Check primary RSS sources...\n2. Keep declared tax disclaimers..."
                onChange={(e) => setCreateSkillForm((p) => ({ ...p, skill_md: e.target.value }))}
              />
            </label>
            <div className="btn-row">
              <button
                type="button"
                className="btn primary"
                disabled={createSkillBusy || !createSkillForm.slug.trim() || !createSkillForm.name.trim()}
                onClick={() => {
                  setCreateSkillBusy(true)
                  void saveCustomSkill({
                    slug: createSkillForm.slug.trim(),
                    name: createSkillForm.name.trim(),
                    description: createSkillForm.description.trim(),
                    slash_command: `/${createSkillForm.slug.trim()}`,
                    skill_md: createSkillForm.skill_md,
                    pinned: true,
                  })
                    .then((created) => {
                      if (created) {
                        setTurnSkillSlugs((prev) =>
                          prev.includes(created.slug) ? prev : [...prev, created.slug],
                        )
                      }
                      setCreateSkillForm({
                        slug: '',
                        name: '',
                        description: '',
                        slash_command: '',
                        skill_md: '',
                      })
                      setCreateSkillOpen(false)
                    })
                    .finally(() => setCreateSkillBusy(false))
                }}
              >
                {c.v22CreateSkillSaveBtn}
              </button>
              <button
                type="button"
                className="btn secondary"
                onClick={() => setCreateSkillOpen(false)}
              >
                {c.cancelBtn}
              </button>
            </div>
          </div>
        </div>
      )}

      {bulkImportOpen && (
        <div className="modal-backdrop" role="dialog" aria-label={c.v22BulkImportTitle}>
          <div className="modal-card">
            <h3>{c.v22BulkImportTitle}</h3>
            <label className="field">
              <span>{c.v22BulkProviderLabel}</span>
              <input
                value={bulkImportProvider}
                placeholder="hub"
                onChange={(e) => setBulkImportProvider(e.target.value)}
              />
            </label>
            <label className="field">
              <span>{c.v22BulkModelsInputLabel}</span>
              <textarea
                rows={5}
                value={bulkImportRaw}
                placeholder="gpt-5.4\nclaude-sonnet-4-6\ngemini-2.5-pro"
                onChange={(e) => setBulkImportRaw(e.target.value)}
              />
            </label>
            <label style={{ display: 'flex', alignItems: 'center', gap: 8, margin: '8px 0' }}>
              <input
                type="checkbox"
                checked={bulkPreserveEnabled}
                onChange={(e) => setBulkPreserveEnabled(e.target.checked)}
              />
              <span>{c.v22BulkPreserveEnabled}</span>
            </label>
            <div className="btn-row">
              <button
                type="button"
                className="btn primary"
                disabled={bulkImportBusy || !bulkImportRaw.trim()}
                onClick={() => {
                  const ids = bulkImportRaw
                    .split(/[\n,]+/)
                    .map((s) => s.trim())
                    .filter(Boolean)
                  if (ids.length === 0) return
                  setBulkImportBusy(true)
                  void bulkUpsertCatalogModels({
                    provider_id: bulkImportProvider.trim() || 'hub',
                    preserve_existing_enabled: bulkPreserveEnabled,
                    models: ids.map((model_id) => ({ model_id, enabled: true })),
                  })
                    .then((res) => {
                      setConnNotice(`${c.v22BulkSubmitBtn}: ${res.upserted_count ?? 0}`)
                      setBulkImportRaw('')
                      setBulkImportOpen(false)
                    })
                    .catch((err) =>
                      setConnError(err instanceof Error ? err.message : 'Bulk import failed'),
                    )
                    .finally(() => setBulkImportBusy(false))
                }}
              >
                {c.v22BulkSubmitBtn}
              </button>
              <button
                type="button"
                className="btn secondary"
                onClick={() => setBulkImportOpen(false)}
              >
                {c.cancelBtn}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
