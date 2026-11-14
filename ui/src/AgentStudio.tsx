import { useEffect, useMemo, useState } from 'react'
import {
  AGENT_COPY,
  MCP_BRIDGE_TOOLS,
  NEWSMAN_APP_TOOLS,
  type AgentSchedule,
  type AgentThread,
  type AgentWorkspace,
  type AppAgentConfig,
  type BridgeStatus,
  type HubSkillGroupSummary,
  type HubSkillSummary,
  type MemoryMode,
  type ReplyLanguage,
  type RuntimeKind,
  type TelegramBridgeConfig,
  type TelegramChatPolicy,
  type TelegramRouteConfig,
  type TelegramTransportMode,
} from './agentTypes.ts'
import { Select } from './Select.tsx'
import { stripSkillFrontmatter } from './useAppAgents.ts'

type StudioTab = 'identity' | 'persona' | 'runtime' | 'scope' | 'schedules' | 'telegram' | 'skills'

const RUNTIME_KINDS: readonly {
  id: RuntimeKind
  label_vi: string
  label_en: string
  desc_vi: string
  desc_en: string
}[] = [
  {
    id: 'partner_fast',
    label_vi: 'Trực tiếp tốc độ cao (LNC Hub)',
    label_en: 'Fast Direct Loop (LNC Hub)',
    desc_vi: 'Phản hồi tức thì qua LNC Hub kèm tra cứu bài báo và chuỗi số liệu tòa soạn.',
    desc_en: 'Low-latency analytical loop via LNC Hub with native newsroom source lookups.',
  },
  {
    id: 'openhands',
    label_vi: 'Không gian nghiên cứu đa bước (OpenHands)',
    label_en: 'Deep Research Runtime (OpenHands)',
    desc_vi: 'Phù hợp cho các phiên tổng hợp báo cáo vĩ mô và đối chiếu nhiều nguồn dài hơi.',
    desc_en: 'Extended agentic environment for multi-step macro research and brief synthesis.',
  },
  {
    id: 'codex_acp',
    label_vi: 'Nhánh phân tích Codex',
    label_en: 'Codex Analytical Worker',
    desc_vi: 'Chuyên sâu xử lý dữ liệu định lượng, chuỗi thời gian và kiểm chứng logic.',
    desc_en: 'Specialized analytical worker backed by Codex for quantitative and series checks.',
  },
  {
    id: 'claude_acp',
    label_vi: 'Nhánh biên tập & pháp quy Claude',
    label_en: 'Claude Editorial & Policy Worker',
    desc_vi: 'Chuyên sâu đọc hiểu văn bản dài, đối chiếu pháp quy và biên tập bản tin.',
    desc_en: 'Specialized worker backed by Claude for long-form regulatory reading and drafting.',
  },
  {
    id: 'remote_agent',
    label_vi: 'Trợ lý kết nối từ xa',
    label_en: 'Custom Remote Endpoint',
    desc_vi: 'Kết nối tới dịch vụ trợ lý riêng, xác thực an toàn bằng đường dẫn khóa trên Hub Vault.',
    desc_en: 'Connect to an external agent endpoint authenticated via a Hub Vault reference.',
  },
] as const

const COLOR_PRESETS = ['#236466', '#ba7044', '#4a5470', '#8c4a3a', '#2f6a4a', '#6b5a3e'] as const
const EMOJI_PRESETS = ['📈', '⚖️', '🗞️', '🌐', '🏦', '📊', '🏛️', '🧭'] as const

function parseSlugList(input: string): string[] {
  return input
    .split(',')
    .map((s) => s.trim().toLowerCase())
    .filter((s) => s.length > 0)
}

export interface AgentStudioProps {
  lang: 'vi' | 'en'
  agents: AppAgentConfig[]
  models: string[]
  threads: AgentThread[]
  schedules: AgentSchedule[]
  telegramConfig: TelegramBridgeConfig
  telegramRoutes: TelegramRouteConfig[]
  bridgeStatus: BridgeStatus
  skills?: HubSkillSummary[]
  skillGroups?: HubSkillGroupSummary[]
  allowedMcpTools?: string[]
  workspaces?: AgentWorkspace[]
  onSelectWorkspace?: (workspaceId: string) => void
  onSaveAgent: (agent: AppAgentConfig) => Promise<void>
  onDeleteAgent: (slug: string) => Promise<void>
  onCreateSchedule: (input: {
    name: string
    cron_or_interval_secs: string
    agent_slug: string
    thread_id?: number | string | null
    prompt: string
    enabled?: boolean
  }) => Promise<void>
  onUpdateSchedule: (id: number | string, patch: Partial<AgentSchedule>) => Promise<void>
  onDeleteSchedule: (id: number | string) => Promise<void>
  onRunScheduleNow: (id: number | string) => Promise<void>
  onSaveTelegramConfig: (input: {
    transport_mode: TelegramTransportMode
    bot_name: string
    cygnus_url: string
    chat_policy: TelegramChatPolicy
    bot_token?: string
    webhook_secret?: string
    cygnus_key?: string
  }) => Promise<void>
  onCreateTelegramRoute: (input: Omit<TelegramRouteConfig, 'id'>) => Promise<void>
  onDeleteTelegramRoute: (id: number | string) => Promise<void>
  onSaveBridgeConfig: (patch: {
    astraea_url?: string
    hub_url?: string
    default_model?: string
    astraea_token?: string
    hub_token?: string
  }) => Promise<void>
  onPullAgentFromHub?: (slug: string) => Promise<{ source?: string; agent?: Record<string, unknown> }>
  onPushAgentDraft?: (slug: string, notes?: string) => Promise<{ local_saved?: boolean; hub_synced?: boolean }>
  onGenerateTelegramLinkCode?: (input?: {
    agent_slug?: string
    thread_id?: string | null
  }) => Promise<{
    code: string
    command: string
    agent_slug: string
    ttl_seconds: number
    expires_at: string
  }>
}

export function AgentStudio({
  lang,
  agents,
  models,
  threads,
  schedules,
  telegramConfig,
  telegramRoutes,
  bridgeStatus,
  skills = [],
  skillGroups = [],
  allowedMcpTools = [],
  workspaces = [],
  onSelectWorkspace,
  onSaveAgent,
  onDeleteAgent,
  onCreateSchedule,
  onUpdateSchedule,
  onDeleteSchedule,
  onRunScheduleNow,
  onSaveTelegramConfig,
  onCreateTelegramRoute,
  onDeleteTelegramRoute,
  onSaveBridgeConfig,
  onPullAgentFromHub,
  onPushAgentDraft,
  onGenerateTelegramLinkCode,
}: AgentStudioProps) {
  const c = AGENT_COPY[lang]
  const [activeTab, setActiveTab] = useState<StudioTab>('identity')
  const [selectedSlug, setSelectedSlug] = useState<string>(() => agents[0]?.slug ?? 'macro-argus')
  const [isNewAgent, setIsNewAgent] = useState(false)
  const [busy, setBusy] = useState(false)
  const [statusMsg, setStatusMsg] = useState('')
  const [errorMsg, setErrorMsg] = useState('')
  const [linkCodeResult, setLinkCodeResult] = useState<{
    code: string
    command: string
    ttl_seconds: number
  } | null>(null)

  const emptyDraft = (): AppAgentConfig => ({
    slug: '',
    display_name: '',
    description: '',
    emoji: '📈',
    color: '#236466',
    avatar_url: null,
    reply_language: 'vi',
    soul_md: '',
    app_role_addendum_md:
      lang === 'vi'
        ? 'Tuân thủ phạm vi vùng và chủ đề được phép; luôn giữ nguyên trạng thái chưa xác minh (verified = false) và miễn trừ pháp lý/thuế khi dẫn lịch thuế hoặc văn bản pháp quy.'
        : 'Respect allowed regions and topics; always preserve unverified status (verified = false) and statutory tax/legal disclaimers when citing tax deadlines or regulatory filings.',
    sync_blueprint_to_hub: true,
    runtime_kind: 'partner_fast',
    primary_model: models[0] ?? 'gpt-5.4-mini',
    backup_model: models[1] ?? 'claude-3-7-sonnet',
    reasoning_effort: 'medium',
    remote_endpoint_cfg: { url: '', secret_ref: '', transport: 'sse' },
    max_tool_steps: 8,
    enabled_app_tools: [
      'latest_articles',
      'article_get',
      'documents',
      'tax_calendar',
      'series',
      'observations',
      'analyses',
    ],
    enabled_mcp_tools: [],
    enabled_skills: [],
    enabled_skill_groups: [],
    default_resource_ctx: { regions: ['global', 'vn'], topics: [] },
    memory_mode: 'app_isolated',
  })

  const [draft, setDraft] = useState<AppAgentConfig>(() => agents[0] ?? emptyDraft())
  const [regionsInput, setRegionsInput] = useState('')
  const [topicsInput, setTopicsInput] = useState('')

  const [schedForm, setSchedForm] = useState({
    name: '',
    cron_or_interval_secs: '0 7 * * *',
    agent_slug: agents[0]?.slug ?? 'macro-argus',
    thread_id: '',
    prompt: '',
  })
  const [runningSchedId, setRunningSchedId] = useState<number | string | null>(null)

  const [tgForm, setTgForm] = useState({
    transport_mode: telegramConfig.transport_mode,
    bot_name: telegramConfig.bot_name,
    cygnus_url: telegramConfig.cygnus_url ?? '',
    chat_policy: telegramConfig.chat_policy,
    bot_token: '',
    webhook_secret: '',
    cygnus_key: '',
  })

  const [routeForm, setRouteForm] = useState({
    transport_mode: telegramConfig.transport_mode,
    policy: telegramConfig.chat_policy,
    chat_id: '',
    message_thread_id: '',
    agent_slug: agents[0]?.slug ?? 'macro-argus',
    thread_id: '',
    bot_name: telegramConfig.bot_name,
    send_progress: true,
    send_tool_hints: false,
  })

  const [bridgeFormOpen, setBridgeFormOpen] = useState(false)
  const [bridgeForm, setBridgeForm] = useState({
    astraea_url: bridgeStatus.astraea_url ?? '',
    hub_url: bridgeStatus.hub_url ?? '',
    default_model: bridgeStatus.default_model ?? 'gpt-5.4-mini',
    astraea_token: '',
    hub_token: '',
  })

  useEffect(() => {
    setTgForm((prev) => ({
      ...prev,
      transport_mode: telegramConfig.transport_mode,
      bot_name: telegramConfig.bot_name,
      cygnus_url: telegramConfig.cygnus_url ?? '',
      chat_policy: telegramConfig.chat_policy,
    }))
  }, [telegramConfig])

  useEffect(() => {
    setBridgeForm((prev) => ({
      ...prev,
      astraea_url: bridgeStatus.astraea_url ?? '',
      hub_url: bridgeStatus.hub_url ?? '',
      default_model: bridgeStatus.default_model ?? 'gpt-5.4-mini',
    }))
  }, [bridgeStatus])

  useEffect(() => {
    if (isNewAgent) return
    const found = agents.find((a) => a.slug === selectedSlug) ?? agents[0]
    if (found) {
      setDraft(found)
      setRegionsInput((found.default_resource_ctx?.regions ?? []).join(', '))
      setTopicsInput((found.default_resource_ctx?.topics ?? []).join(', '))
    }
  }, [agents, isNewAgent, selectedSlug])

  const selectExistingAgent = (slug: string) => {
    setIsNewAgent(false)
    setSelectedSlug(slug)
    setStatusMsg('')
    setErrorMsg('')
  }

  const startNewAgent = () => {
    setIsNewAgent(true)
    const fresh = emptyDraft()
    setDraft(fresh)
    setRegionsInput('global, vn')
    setTopicsInput('')
    setActiveTab('identity')
    setStatusMsg('')
    setErrorMsg('')
  }

  const toggleAppTool = (toolName: string) => {
    setDraft((prev) => {
      const exists = prev.enabled_app_tools.includes(toolName)
      return {
        ...prev,
        enabled_app_tools: exists
          ? prev.enabled_app_tools.filter((t) => t !== toolName)
          : [...prev.enabled_app_tools, toolName],
      }
    })
  }

  const toggleMcpTool = (toolName: string) => {
    setDraft((prev) => {
      const exists = prev.enabled_mcp_tools.includes(toolName)
      return {
        ...prev,
        enabled_mcp_tools: exists
          ? prev.enabled_mcp_tools.filter((t) => t !== toolName)
          : [...prev.enabled_mcp_tools, toolName],
      }
    })
  }

  const secretRefInvalid = useMemo(() => {
    const ref = (draft.remote_endpoint_cfg?.secret_ref ?? '').trim()
    return ref.length > 0 && !ref.startsWith('vault://')
  }, [draft.remote_endpoint_cfg?.secret_ref])

  const handleSaveAgent = async () => {
    setErrorMsg('')
    setStatusMsg('')
    if (!draft.slug.trim()) {
      setErrorMsg(lang === 'vi' ? 'Vui lòng nhập mã định danh ngắn.' : 'Short handle is required.')
      return
    }
    if (secretRefInvalid) {
      setErrorMsg(c.studioSecretRefInvalid)
      return
    }

    const regions = parseSlugList(regionsInput)
    const topics = parseSlugList(topicsInput)
    const payload: AppAgentConfig = {
      ...draft,
      slug: draft.slug.trim(),
      display_name: draft.display_name.trim() || draft.slug.trim(),
      default_resource_ctx: {
        ...(regions.length > 0 ? { regions } : {}),
        ...(topics.length > 0 ? { topics } : {}),
      },
    }

    setBusy(true)
    try {
      await onSaveAgent(payload)
      setIsNewAgent(false)
      setSelectedSlug(payload.slug)
      setStatusMsg(lang === 'vi' ? 'Đã lưu cấu hình trợ lý.' : 'Analyst saved.')
    } catch (err) {
      setErrorMsg(err instanceof Error ? err.message : 'Save failed')
    } finally {
      setBusy(false)
    }
  }

  const handleDeleteAgent = async () => {
    if (!draft.slug || isNewAgent) return
    if (!window.confirm(`${c.deleteAgentConfirm}\n\n${draft.emoji} ${draft.display_name || draft.slug}`)) {
      return
    }
    setBusy(true)
    setErrorMsg('')
    setStatusMsg('')
    try {
      await onDeleteAgent(draft.slug)
      const remaining = agents.filter((a) => a.slug !== draft.slug)
      if (remaining[0]) {
        setSelectedSlug(remaining[0].slug)
      }
      setStatusMsg(lang === 'vi' ? 'Đã xoá trợ lý.' : 'Analyst deleted.')
    } catch (err) {
      setErrorMsg(err instanceof Error ? err.message : 'Delete failed')
    } finally {
      setBusy(false)
    }
  }

  const handlePullHub = async () => {
    if (!draft.slug || isNewAgent || !onPullAgentFromHub) return
    setBusy(true)
    setErrorMsg('')
    setStatusMsg('')
    try {
      const res = await onPullAgentFromHub(draft.slug)
      setStatusMsg(
        lang === 'vi'
          ? `Đã đồng bộ hồ sơ từ ${res.source === 'hub' ? 'Hub' : 'bộ nhớ cục bộ'}.`
          : `Pulled profile from ${res.source === 'hub' ? 'Hub' : 'local store'}.`,
      )
    } catch (err) {
      setErrorMsg(err instanceof Error ? err.message : 'Pull failed')
    } finally {
      setBusy(false)
    }
  }

  const handlePushDraft = async () => {
    if (!draft.slug || isNewAgent || !onPushAgentDraft) return
    setBusy(true)
    setErrorMsg('')
    setStatusMsg('')
    try {
      await handleSaveAgent()
      const res = await onPushAgentDraft(draft.slug, 'Updated from LNC Newsman v2 Studio')
      setStatusMsg(
        res.hub_synced
          ? lang === 'vi'
            ? 'Đã lưu & đẩy bản nháp lên Hub.'
            : 'Saved & pushed draft to Hub.'
          : lang === 'vi'
            ? 'Đã lưu cục bộ (Hub chưa nhận push).'
            : 'Saved locally (Hub offline or read-only).',
      )
    } catch (err) {
      setErrorMsg(err instanceof Error ? err.message : 'Push failed')
    } finally {
      setBusy(false)
    }
  }

  const combinedPromptPreview = useMemo(() => {
    const regions = parseSlugList(regionsInput)
    const topics = parseSlugList(topicsInput)
    const scopeHeader =
      lang === 'vi'
        ? `## Hướng dẫn Tòa soạn (Vùng: ${regions.join(', ') || c.salonScopeRegionsAll} · Chủ đề: ${topics.join(', ') || c.salonScopeTopicsAll})`
        : `## Newsroom Guidelines (Regions: ${regions.join(', ') || c.salonScopeRegionsAll} · Topics: ${topics.join(', ') || c.salonScopeTopicsAll})`
    const toolsHeader =
      lang === 'vi'
        ? `## Công cụ được phép (${draft.enabled_app_tools.length})`
        : `## Allowed Tools (${draft.enabled_app_tools.length})`
    const toolLabels = draft.enabled_app_tools
      .map((name) => {
        const found = NEWSMAN_APP_TOOLS.find((t) => t.name === name)
        return found ? (lang === 'vi' ? found.label_vi : found.label_en) : name
      })
      .join(' · ')
    const parts = [
      `# ${draft.emoji} ${draft.display_name || draft.slug} (@${draft.slug || 'analyst'})`,
      draft.soul_md.trim() ||
        (lang === 'vi' ? '*(Chưa nhập tính cách cốt lõi)*' : '*(No core persona entered)*'),
      scopeHeader,
      draft.app_role_addendum_md.trim(),
      toolsHeader,
      toolLabels || (lang === 'vi' ? 'Chưa bật công cụ nào' : 'None'),
    ]
    return parts.join('\n\n')
  }, [c.salonScopeRegionsAll, c.salonScopeTopicsAll, draft, lang, regionsInput, topicsInput])

  const toggleSkill = (skillSlug: string) => {
    setDraft((prev) => {
      const exists = prev.enabled_skills.includes(skillSlug)
      return {
        ...prev,
        enabled_skills: exists
          ? prev.enabled_skills.filter((s) => s !== skillSlug)
          : [...prev.enabled_skills, skillSlug],
      }
    })
  }

  const toggleSkillGroup = (groupSlug: string) => {
    setDraft((prev) => {
      const exists = prev.enabled_skill_groups.includes(groupSlug)
      return {
        ...prev,
        enabled_skill_groups: exists
          ? prev.enabled_skill_groups.filter((g) => g !== groupSlug)
          : [...prev.enabled_skill_groups, groupSlug],
      }
    })
  }

  const tabs: { id: StudioTab; label: string }[] = [
    { id: 'identity', label: c.studioTabIdentity },
    { id: 'persona', label: c.studioTabPersona },
    { id: 'runtime', label: c.studioTabRuntime },
    { id: 'scope', label: c.studioTabScope },
    { id: 'schedules', label: c.studioTabSchedules },
    { id: 'telegram', label: c.studioTabTelegram },
    { id: 'skills', label: c.studioTabSkills },
  ]

  const isDirectTg =
    tgForm.transport_mode === 'direct_poll' || tgForm.transport_mode === 'direct_webhook'

  const policyShortLabel = (p: TelegramChatPolicy) =>
    p === 'open'
      ? c.studioTelegramPolicyOpen
      : p === 'topics_only'
        ? c.studioTelegramPolicyTopics
        : c.studioTelegramPolicyMention

  return (
    <div className="agent-studio ak-scope">
      <header className="studio-header">
        <div>
          <p className="eyebrow">{c.agentsEyebrow}</p>
          <h3>{c.studioHeaderTitle}</h3>
          <p className="muted">{c.studioHeaderSub}</p>
        </div>
        <div className="btn-row">
          <button
            type="button"
            className="btn secondary"
            onClick={() => setBridgeFormOpen((v) => !v)}
          >
            ⛭ {c.bridgeConfigureBtn}
          </button>
          {!isNewAgent && onPullAgentFromHub && (
            <button
              type="button"
              className="btn secondary"
              disabled={busy}
              onClick={() => void handlePullHub()}
            >
              ⇣ {c.v2StudioPullHubBtn}
            </button>
          )}
          {!isNewAgent && onPushAgentDraft && (
            <button
              type="button"
              className="btn secondary"
              disabled={busy || secretRefInvalid}
              onClick={() => void handlePushDraft()}
            >
              ⇡ {c.v2StudioPushDraftBtn}
            </button>
          )}
          <button type="button" className="btn secondary" onClick={startNewAgent}>
            {c.studioNewAgentBtn}
          </button>
          <button
            type="button"
            className="btn primary"
            onClick={() => void handleSaveAgent()}
            disabled={busy || secretRefInvalid}
          >
            {c.studioSaveAgentBtn}
          </button>
        </div>
      </header>

      {bridgeFormOpen && (
        <section className="settings-card studio-bridge-drawer">
          <div className="catalog-form-row">
            <label className="field">
              <span>{c.bridgeAstraeaUrl}</span>
              <input
                value={bridgeForm.astraea_url}
                placeholder="https://astraea.lengoc.me"
                onChange={(e) => setBridgeForm({ ...bridgeForm, astraea_url: e.target.value })}
              />
            </label>
            <label className="field">
              <span>{c.bridgeHubUrl}</span>
              <input
                value={bridgeForm.hub_url}
                placeholder="https://agents-hub.lengoc.me"
                onChange={(e) => setBridgeForm({ ...bridgeForm, hub_url: e.target.value })}
              />
            </label>
            <label className="field">
              <span>{c.bridgeDefaultModel}</span>
              <input
                list="studio-hub-models"
                value={bridgeForm.default_model}
                onChange={(e) => setBridgeForm({ ...bridgeForm, default_model: e.target.value })}
              />
            </label>
          </div>
          <div className="catalog-form-row">
            <label className="field">
              <span>{c.bridgeAstraeaToken}</span>
              <input
                type="password"
                autoComplete="new-password"
                value={bridgeForm.astraea_token}
                placeholder={bridgeStatus.has_astraea_token ? '••••••••' : 'ast_bridge_…'}
                onChange={(e) => setBridgeForm({ ...bridgeForm, astraea_token: e.target.value })}
              />
              {bridgeStatus.has_astraea_token && <small>{c.bridgeTokenSaved}</small>}
            </label>
            <label className="field">
              <span>{c.bridgeHubToken}</span>
              <input
                type="password"
                autoComplete="new-password"
                value={bridgeForm.hub_token}
                placeholder={bridgeStatus.has_hub_token ? '••••••••' : 'lnc_hub_…'}
                onChange={(e) => setBridgeForm({ ...bridgeForm, hub_token: e.target.value })}
              />
              {bridgeStatus.has_hub_token && <small>{c.bridgeTokenSaved}</small>}
            </label>
          </div>
          <div className="btn-row">
            <button
              type="button"
              className="btn primary"
              disabled={busy}
              onClick={() => {
                setBusy(true)
                void onSaveBridgeConfig(bridgeForm)
                  .then(() => {
                    setBridgeForm((f) => ({ ...f, astraea_token: '', hub_token: '' }))
                    setStatusMsg(
                      lang === 'vi' ? 'Đã lưu cấu hình Kết nối.' : 'Connection settings saved.',
                    )
                  })
                  .finally(() => setBusy(false))
              }}
            >
              {lang === 'vi' ? 'Lưu kết nối' : 'Save Connection'}
            </button>
          </div>
        </section>
      )}

      {statusMsg && <p className="status-ok">{statusMsg}</p>}
      {errorMsg && <p className="form-error">{errorMsg}</p>}

      <div className="studio-layout">
        <aside className="studio-roster">
          {agents.map((ag) => (
            <button
              key={ag.slug}
              type="button"
              className={
                !isNewAgent && ag.slug === selectedSlug
                  ? 'studio-agent-card active'
                  : 'studio-agent-card'
              }
              onClick={() => selectExistingAgent(ag.slug)}
            >
              <span className="studio-agent-avatar" style={{ background: ag.color }}>
                {ag.emoji || '📈'}
              </span>
              <span className="studio-agent-meta">
                <strong>{ag.display_name}</strong>
                <code>@{ag.slug}</code>
                <small>{ag.primary_model}</small>
              </span>
            </button>
          ))}
        </aside>

        <div className="studio-editor">
          <div className="catalog-tabs studio-tabs" role="tablist">
            {tabs.map((t) => (
              <button
                key={t.id}
                type="button"
                role="tab"
                aria-selected={activeTab === t.id}
                className={activeTab === t.id ? 'active' : ''}
                onClick={() => setActiveTab(t.id)}
              >
                {t.label}
              </button>
            ))}
          </div>

          <datalist id="studio-hub-models">
            {models.map((m) => (
              <option key={m} value={m} />
            ))}
          </datalist>

          {activeTab === 'identity' && (
            <section className="studio-panel">
              <div className="catalog-form-row">
                <label className="field">
                  <span>{c.studioSlugLabel}</span>
                  <input
                    value={draft.slug}
                    disabled={!isNewAgent}
                    placeholder="macro-argus"
                    onChange={(e) =>
                      setDraft({
                        ...draft,
                        slug: e.target.value.toLowerCase().replace(/[^a-z0-9_-]/g, '-'),
                      })
                    }
                  />
                </label>
                <label className="field">
                  <span>{c.studioDisplayNameLabel}</span>
                  <input
                    value={draft.display_name}
                    placeholder="Chuyên gia Vĩ mô Argus"
                    onChange={(e) => setDraft({ ...draft, display_name: e.target.value })}
                  />
                </label>
              </div>

              <label className="field">
                <span>{c.studioDescriptionLabel}</span>
                <input
                  value={draft.description}
                  onChange={(e) => setDraft({ ...draft, description: e.target.value })}
                />
              </label>

              <div className="catalog-form-row studio-form-row-3">
                <div className="field">
                  <span>{c.studioEmojiLabel}</span>
                  <div className="chip-row">
                    {EMOJI_PRESETS.map((em) => (
                      <button
                        key={em}
                        type="button"
                        className={draft.emoji === em ? 'chip active' : 'chip'}
                        onClick={() => setDraft({ ...draft, emoji: em })}
                      >
                        {em}
                      </button>
                    ))}
                  </div>
                </div>

                <div className="field">
                  <span>{c.studioColorLabel}</span>
                  <div className="studio-color-row">
                    {COLOR_PRESETS.map((col) => (
                      <button
                        key={col}
                        type="button"
                        className={draft.color === col ? 'color-swatch chosen' : 'color-swatch'}
                        style={{ background: col }}
                        onClick={() => setDraft({ ...draft, color: col })}
                        aria-label={col}
                      />
                    ))}
                  </div>
                </div>

                <Select
                  variant="field"
                  label={c.studioReplyLangLabel}
                  value={draft.reply_language}
                  onChange={(val) => setDraft({ ...draft, reply_language: val as ReplyLanguage })}
                  options={[
                    { value: 'vi', label: c.studioLangVi },
                    { value: 'en', label: c.studioLangEn },
                    { value: 'auto', label: c.studioLangAuto },
                  ]}
                />
              </div>

              {!isNewAgent && (
                <div className="btn-row">
                  <button
                    type="button"
                    className="btn ghost danger"
                    onClick={() => void handleDeleteAgent()}
                    disabled={busy}
                  >
                    🗑 {c.studioDeleteAgentBtn}
                  </button>
                </div>
              )}
            </section>
          )}

          {activeTab === 'persona' && (
            <section className="studio-panel">
              <div className="studio-split-persona">
                <div className="studio-persona-inputs">
                  <label className="field">
                    <span>{c.studioSoulMdLabel}</span>
                    <textarea
                      rows={6}
                      value={draft.soul_md}
                      onChange={(e) => setDraft({ ...draft, soul_md: e.target.value })}
                    />
                    <small>{c.studioSoulMdHint}</small>
                  </label>
                  <label className="field">
                    <span>{c.studioRoleAddendumLabel}</span>
                    <textarea
                      rows={4}
                      value={draft.app_role_addendum_md}
                      onChange={(e) => setDraft({ ...draft, app_role_addendum_md: e.target.value })}
                    />
                  </label>
                  <label className="toggle-field">
                    <span>{c.studioSyncHubLabel}</span>
                    <input
                      type="checkbox"
                      checked={draft.sync_blueprint_to_hub}
                      onChange={(e) =>
                        setDraft({ ...draft, sync_blueprint_to_hub: e.target.checked })
                      }
                    />
                  </label>
                </div>
                <div className="studio-persona-preview">
                  <span className="eyebrow">{c.studioRolePreviewLabel}</span>
                  <pre className="studio-markdown-preview">{combinedPromptPreview}</pre>
                </div>
              </div>
            </section>
          )}

          {activeTab === 'runtime' && (
            <section className="studio-panel">
              <div className="field">
                <span>{c.studioRuntimeKindLabel}</span>
                <div className="studio-runtime-grid">
                  {RUNTIME_KINDS.map((rk) => (
                    <button
                      key={rk.id}
                      type="button"
                      className={
                        draft.runtime_kind === rk.id
                          ? 'studio-runtime-card active'
                          : 'studio-runtime-card'
                      }
                      onClick={() => setDraft({ ...draft, runtime_kind: rk.id })}
                    >
                      <strong>{lang === 'vi' ? rk.label_vi : rk.label_en}</strong>
                      <small>{lang === 'vi' ? rk.desc_vi : rk.desc_en}</small>
                    </button>
                  ))}
                </div>
              </div>

              <div className="catalog-form-row">
                <label className="field">
                  <span>{c.studioPrimaryModelLabel}</span>
                  <input
                    list="studio-hub-models"
                    value={draft.primary_model}
                    onChange={(e) => setDraft({ ...draft, primary_model: e.target.value })}
                  />
                </label>
                <label className="field">
                  <span>{c.studioBackupModelLabel}</span>
                  <input
                    list="studio-hub-models"
                    value={draft.backup_model}
                    onChange={(e) => setDraft({ ...draft, backup_model: e.target.value })}
                  />
                </label>
              </div>

              <div className="catalog-form-row">
                <Select
                  variant="field"
                  label={c.studioReasoningEffortLabel}
                  value={String(draft.reasoning_effort)}
                  onChange={(val) => setDraft({ ...draft, reasoning_effort: val })}
                  options={[
                    { value: 'low', label: c.studioEffortLow },
                    { value: 'medium', label: c.studioEffortMedium },
                    { value: 'high', label: c.studioEffortHigh },
                  ]}
                />
                <label className="field">
                  <span>
                    {c.studioMaxToolStepsLabel}: <b>{draft.max_tool_steps}</b>
                  </span>
                  <input
                    type="range"
                    min={1}
                    max={20}
                    value={draft.max_tool_steps}
                    onChange={(e) => setDraft({ ...draft, max_tool_steps: Number(e.target.value) })}
                  />
                </label>
              </div>

              <div className="catalog-form-row">
                <label className="field">
                  <span>{c.studioRemoteUrlLabel}</span>
                  <input
                    value={draft.remote_endpoint_cfg?.url ?? ''}
                    placeholder="https://astraea.lengoc.me/acp/codex"
                    onChange={(e) =>
                      setDraft({
                        ...draft,
                        remote_endpoint_cfg: { ...draft.remote_endpoint_cfg, url: e.target.value },
                      })
                    }
                  />
                </label>
                <label className="field">
                  <span>{c.studioSecretRefLabel}</span>
                  <input
                    value={draft.remote_endpoint_cfg?.secret_ref ?? ''}
                    placeholder="vault://lnc-hub/agents/macro-argus"
                    onChange={(e) =>
                      setDraft({
                        ...draft,
                        remote_endpoint_cfg: {
                          ...draft.remote_endpoint_cfg,
                          secret_ref: e.target.value,
                        },
                      })
                    }
                  />
                  <small>{c.studioSecretRefHint}</small>
                  {secretRefInvalid && <small className="form-error">{c.studioSecretRefInvalid}</small>}
                </label>
              </div>
            </section>
          )}

          {activeTab === 'scope' && (
            <section className="studio-panel">
              <div className="tax-guardrail-callout">⚖️ {c.studioTaxDisclaimerNotice}</div>

              <div className="field">
                <span>{c.studioAppToolsTitle}</span>
                <div className="studio-tools-grid">
                  {NEWSMAN_APP_TOOLS.map((tool) => {
                    const checked = draft.enabled_app_tools.includes(tool.name)
                    return (
                      <label
                        key={tool.name}
                        className={checked ? 'studio-tool-item active' : 'studio-tool-item'}
                      >
                        <input
                          type="checkbox"
                          checked={checked}
                          onChange={() => toggleAppTool(tool.name)}
                        />
                        <div className="studio-tool-body">
                          <div className="studio-tool-head">
                            <strong>{lang === 'vi' ? tool.label_vi : tool.label_en}</strong>
                            {tool.tier === 'write_gated' && (
                              <span className="tier-badge status-pending">{c.toolApprovalRequired}</span>
                            )}
                          </div>
                          <small>{lang === 'vi' ? tool.desc_vi : tool.desc_en}</small>
                        </div>
                      </label>
                    )
                  })}
                </div>
              </div>

              <div className="field">
                <span>{c.studioMcpToolsTitle}</span>
                {allowedMcpTools.length > 0 && <small className="muted-sub">{c.studioMcpToolsHintHub}</small>}
                <div className="chip-row">
                  {Array.from(
                    new Map<string, string>([
                      ...MCP_BRIDGE_TOOLS.map(
                        (m): [string, string] => [
                          m.name,
                          lang === 'vi' ? m.label_vi : m.label_en,
                        ],
                      ),
                      ...allowedMcpTools.map((name): [string, string] => [name, name]),
                      ...draft.enabled_mcp_tools.map((name): [string, string] => [name, name]),
                    ]).entries(),
                  ).map(([name, label]) => {
                    const active = draft.enabled_mcp_tools.includes(name)
                    return (
                      <button
                        key={name}
                        type="button"
                        className={active ? 'chip active' : 'chip'}
                        onClick={() => toggleMcpTool(name)}
                      >
                        {active ? '✓ ' : '+ '}
                        {label}
                      </button>
                    )
                  })}
                </div>
              </div>

              <div className="studio-scope-box">
                <p className="eyebrow">{c.studioResourceCtxTitle}</p>
                <div className="catalog-form-row">
                  <label className="field">
                    <span>{c.studioAllowedRegionsLabel}</span>
                    <input
                      value={regionsInput}
                      placeholder="vn, us, global"
                      onChange={(e) => setRegionsInput(e.target.value)}
                    />
                  </label>
                  <label className="field">
                    <span>{c.studioAllowedTopicsLabel}</span>
                    <input
                      value={topicsInput}
                      placeholder="macro, tax, markets"
                      onChange={(e) => setTopicsInput(e.target.value)}
                    />
                  </label>
                </div>
                <Select
                  variant="field"
                  label={c.studioMemoryModeLabel}
                  value={draft.memory_mode}
                  onChange={(val) => setDraft({ ...draft, memory_mode: val as MemoryMode })}
                  options={[
                    { value: 'app_isolated', label: c.studioMemoryIsolated },
                    { value: 'cross_app_shared', label: c.studioMemoryShared },
                  ]}
                />
              </div>
            </section>
          )}

          {activeTab === 'schedules' && (
            <section className="studio-panel">
              <p className="eyebrow">{c.studioSchedulesTitle}</p>
              <div className="chat-sched-table">
                {schedules.length === 0 && (
                  <p className="muted">
                    {lang === 'vi'
                      ? 'Chưa có lịch hẹn tự động nào.'
                      : 'No automated schedules configured yet.'}
                  </p>
                )}
                {schedules.map((s) => (
                  <div key={s.id} className="chat-sched-row">
                    <label className="chat-sched-toggle">
                      <input
                        type="checkbox"
                        checked={s.enabled}
                        onChange={() => void onUpdateSchedule(s.id, { enabled: !s.enabled })}
                      />
                    </label>
                    <div className="chat-sched-main">
                      <b>{s.name}</b>
                      <code>
                        @{s.agent_slug} · {s.cron_or_interval_secs}
                      </code>
                      <small className="muted">{s.prompt}</small>
                    </div>
                    <div className="chat-sched-runs">
                      <small>
                        {c.studioScheduleLastStatus}: <b>{s.last_status ?? '—'}</b>
                      </small>
                      {s.last_run_at && <small>{s.last_run_at}</small>}
                    </div>
                    <div className="btn-row">
                      <button
                        type="button"
                        className="btn small secondary"
                        disabled={runningSchedId === s.id}
                        onClick={() => {
                          setRunningSchedId(s.id)
                          void onRunScheduleNow(s.id).finally(() => setRunningSchedId(null))
                        }}
                      >
                        {runningSchedId === s.id ? c.studioScheduleRunning : c.studioScheduleRunNow}
                      </button>
                      <button
                        type="button"
                        className="icon-del"
                        onClick={() => void onDeleteSchedule(s.id)}
                        aria-label={lang === 'vi' ? 'Xoá lịch hẹn' : 'Delete schedule'}
                        title={lang === 'vi' ? 'Xoá lịch hẹn' : 'Delete schedule'}
                      >
                        🗑
                      </button>
                    </div>
                  </div>
                ))}
              </div>

              <div className="chat-sched-form">
                <div className="catalog-form-row studio-form-row-3">
                  <label className="field">
                    <span>{c.studioScheduleNameLabel}</span>
                    <input
                      value={schedForm.name}
                      placeholder={
                        lang === 'vi' ? 'Điểm tin vĩ mô 07:30' : '07:30 Macro Morning Brief'
                      }
                      onChange={(e) => setSchedForm({ ...schedForm, name: e.target.value })}
                    />
                  </label>
                  <label className="field">
                    <span>{c.studioScheduleCronLabel}</span>
                    <input
                      value={schedForm.cron_or_interval_secs}
                      placeholder="30 7 * * *"
                      onChange={(e) =>
                        setSchedForm({ ...schedForm, cron_or_interval_secs: e.target.value })
                      }
                    />
                  </label>
                  <Select
                    variant="field"
                    label={c.studioScheduleAgentLabel}
                    value={schedForm.agent_slug}
                    onChange={(val) => setSchedForm({ ...schedForm, agent_slug: val })}
                    options={agents.map((a) => ({
                      value: a.slug,
                      label: `${a.emoji} ${a.display_name}`,
                      hint: `@${a.slug}`,
                    }))}
                  />
                </div>
                <label className="field">
                  <span>{c.studioSchedulePromptLabel}</span>
                  <input
                    value={schedForm.prompt}
                    placeholder={
                      lang === 'vi'
                        ? 'Tổng hợp biến động lãi suất, tỷ giá và hạn nộp thuế sắp tới.'
                        : 'Summarize overnight interest rate moves, FX series, and upcoming tax deadlines.'
                    }
                    onChange={(e) => setSchedForm({ ...schedForm, prompt: e.target.value })}
                  />
                </label>
                <div className="btn-row">
                  <button
                    type="button"
                    className="btn primary"
                    disabled={!schedForm.name.trim() || !schedForm.prompt.trim()}
                    onClick={() => {
                      void onCreateSchedule({
                        name: schedForm.name.trim(),
                        cron_or_interval_secs: schedForm.cron_or_interval_secs.trim(),
                        agent_slug: schedForm.agent_slug,
                        thread_id: schedForm.thread_id ? Number(schedForm.thread_id) : null,
                        prompt: schedForm.prompt.trim(),
                      }).then(() => setSchedForm((f) => ({ ...f, name: '', prompt: '' })))
                    }}
                  >
                    + {lang === 'vi' ? 'Thêm lịch hẹn' : 'Add Schedule'}
                  </button>
                </div>
              </div>
            </section>
          )}

          {activeTab === 'telegram' && (
            <section className="studio-panel">
              <div className="catalog-notice" style={{ display: 'flex', flexWrap: 'wrap', gap: 8, alignItems: 'center', justifyContent: 'space-between' }}>
                <span>{c.studioTelegramCoexistenceNotice}</span>
                <span className="tier-badge tier-read">
                  {c.studioTelegramRouteLabel}:{' '}
                  {telegramConfig.telegram_route === 'hub_cygnus'
                    ? c.routeViaHubCygnus
                    : telegramConfig.telegram_route === 'direct_cygnus'
                      ? c.routeViaDirectCygnus
                      : telegramConfig.telegram_route === 'direct_bot_api'
                        ? c.routeViaDirectBot
                        : c.routeDisabled}
                </span>
              </div>

              <div className="field">
                <span>{c.studioTelegramModeTitle}</span>
                <div className="chip-row">
                  {(
                    [
                      { id: 'direct_poll', label: `${c.studioTelegramDirectMode} · Poll` },
                      { id: 'direct_webhook', label: `${c.studioTelegramDirectMode} · Webhook` },
                      { id: 'cygnus_poll', label: `${c.studioTelegramCygnusMode} · Poll` },
                      { id: 'cygnus_webhook', label: `${c.studioTelegramCygnusMode} · Webhook` },
                    ] as const
                  ).map((mode) => (
                    <button
                      key={mode.id}
                      type="button"
                      className={tgForm.transport_mode === mode.id ? 'chip active' : 'chip'}
                      onClick={() => setTgForm({ ...tgForm, transport_mode: mode.id })}
                    >
                      {mode.label}
                    </button>
                  ))}
                </div>
              </div>

              <div className="catalog-form-row">
                <label className="field">
                  <span>{c.studioTelegramBotName}</span>
                  <input
                    value={tgForm.bot_name}
                    placeholder="@lnc_newsman_bot"
                    onChange={(e) => setTgForm({ ...tgForm, bot_name: e.target.value })}
                  />
                </label>
                <Select
                  variant="field"
                  label={c.studioTelegramPolicyLabel}
                  value={tgForm.chat_policy}
                  onChange={(val) =>
                    setTgForm({ ...tgForm, chat_policy: val as TelegramChatPolicy })
                  }
                  options={[
                    { value: 'open', label: c.studioTelegramPolicyOpen },
                    { value: 'mention', label: c.studioTelegramPolicyMention },
                    { value: 'topics_only', label: c.studioTelegramPolicyTopics },
                  ]}
                />
              </div>

              {isDirectTg ? (
                <div className="catalog-form-row">
                  <label className="field">
                    <span>{c.studioTelegramBotToken}</span>
                    <input
                      type="password"
                      autoComplete="new-password"
                      value={tgForm.bot_token}
                      placeholder={telegramConfig.has_bot_token ? '••••••••' : '123456:ABC-DEF…'}
                      onChange={(e) => setTgForm({ ...tgForm, bot_token: e.target.value })}
                    />
                    {telegramConfig.has_bot_token && <small>{c.bridgeTokenSaved}</small>}
                  </label>
                  <label className="field">
                    <span>{c.studioTelegramWebhookSecret}</span>
                    <input
                      type="password"
                      autoComplete="new-password"
                      value={tgForm.webhook_secret}
                      placeholder={
                        telegramConfig.has_webhook_secret ? '••••••••' : 'tg_wh_secret_…'
                      }
                      onChange={(e) => setTgForm({ ...tgForm, webhook_secret: e.target.value })}
                    />
                  </label>
                </div>
              ) : (
                <div className="catalog-form-row">
                  <label className="field">
                    <span>{c.studioTelegramCygnusUrl}</span>
                    <input
                      value={tgForm.cygnus_url}
                      placeholder={c.routeViaHubCygnus}
                      onChange={(e) => setTgForm({ ...tgForm, cygnus_url: e.target.value })}
                    />
                    <small className="muted-sub">{c.studioTelegramHubBrokerNote}</small>
                  </label>
                  <label className="field">
                    <span>{c.studioTelegramCygnusKey}</span>
                    <input
                      type="password"
                      autoComplete="new-password"
                      value={tgForm.cygnus_key}
                      placeholder={telegramConfig.has_cygnus_key ? '••••••••' : 'cyg_…'}
                      onChange={(e) => setTgForm({ ...tgForm, cygnus_key: e.target.value })}
                    />
                    {telegramConfig.has_cygnus_key && <small>{c.bridgeTokenSaved}</small>}
                  </label>
                </div>
              )}

              <div className="btn-row">
                <button
                  type="button"
                  className="btn primary"
                  disabled={busy}
                  onClick={() => {
                    setBusy(true)
                    void onSaveTelegramConfig(tgForm)
                      .then(() => {
                        setTgForm((f) => ({
                          ...f,
                          bot_token: '',
                          webhook_secret: '',
                          cygnus_key: '',
                        }))
                        setStatusMsg(
                          lang === 'vi'
                            ? 'Đã lưu cấu hình kênh Telegram.'
                            : 'Telegram channel saved.',
                        )
                      })
                      .finally(() => setBusy(false))
                  }}
                >
                  {lang === 'vi' ? 'Lưu kênh Telegram' : 'Save Telegram Channel'}
                </button>
              </div>

              {onGenerateTelegramLinkCode && (
                <div className="studio-scope-box">
                  <div className="studio-inline-head">
                    <p className="eyebrow">
                      {lang === 'vi' ? 'Liên kết nhanh bằng /link' : 'One-Tap /link Pairing'}
                    </p>
                    <button
                      type="button"
                      className="btn secondary btn-sm"
                      disabled={busy}
                      onClick={() => {
                        setBusy(true)
                        setErrorMsg('')
                        void onGenerateTelegramLinkCode({
                          agent_slug: routeForm.agent_slug || draft.slug,
                          thread_id: routeForm.thread_id || null,
                        })
                          .then((res) => setLinkCodeResult(res))
                          .catch((err) =>
                            setErrorMsg(err instanceof Error ? err.message : 'Link code failed'),
                          )
                          .finally(() => setBusy(false))
                      }}
                    >
                      🔗 {c.v2TelegramGenCodeBtn}
                    </button>
                  </div>
                  <p className="muted">
                    {lang === 'vi'
                      ? 'Tạo mã ngắn (10 phút) rồi gửi lệnh này vào nhóm hoặc DM Telegram của bạn.'
                      : 'Generate a 10-minute pairing code and send the command inside any Telegram group or DM.'}
                  </p>
                  {linkCodeResult && (
                    <div className="btn-row">
                      <code>{linkCodeResult.command}</code>
                      <button
                        type="button"
                        className="btn secondary btn-sm"
                        onClick={() =>
                          void navigator.clipboard?.writeText(linkCodeResult.command)
                        }
                      >
                        {c.v2CopyBtn}
                      </button>
                    </div>
                  )}
                </div>
              )}

              <div className="studio-scope-box">
                <p className="eyebrow">{c.studioTelegramRoutesTitle}</p>
                <div className="chat-sched-table">
                  {telegramRoutes.map((rt) => (
                    <div key={rt.id} className="chat-sched-row">
                      <div className="chat-sched-main">
                        <b>
                          {lang === 'vi' ? 'Nhóm' : 'Group'} {rt.chat_id}
                          {rt.message_thread_id != null
                            ? ` · ${lang === 'vi' ? 'Chủ đề' : 'Topic'} #${rt.message_thread_id}`
                            : ''}
                        </b>
                        <small className="muted">
                          → @{rt.agent_slug}
                          {rt.thread_id ? ` (#${rt.thread_id})` : ''} · {policyShortLabel(rt.policy)}
                          {rt.send_progress ? ' · Progress' : ''}
                          {rt.send_tool_hints ? ' · Tools' : ''}
                        </small>
                      </div>
                      <button
                        type="button"
                        className="icon-del"
                        onClick={() => void onDeleteTelegramRoute(rt.id)}
                        aria-label={lang === 'vi' ? 'Xoá liên kết nhóm' : 'Remove group binding'}
                        title={lang === 'vi' ? 'Xoá liên kết nhóm' : 'Remove group binding'}
                      >
                        🗑
                      </button>
                    </div>
                  ))}
                </div>

                <div className="catalog-form-row">
                  <label className="field">
                    <span>{c.studioTelegramChatId}</span>
                    <input
                      value={routeForm.chat_id}
                      placeholder="-100192837465"
                      onChange={(e) => setRouteForm({ ...routeForm, chat_id: e.target.value })}
                    />
                  </label>
                  <label className="field">
                    <span>{c.studioTelegramTopicId}</span>
                    <input
                      value={routeForm.message_thread_id}
                      placeholder="88"
                      onChange={(e) =>
                        setRouteForm({ ...routeForm, message_thread_id: e.target.value })
                      }
                    />
                  </label>
                  <Select
                    variant="field"
                    label={c.studioRouteAgentLabel}
                    value={routeForm.agent_slug}
                    onChange={(val) => setRouteForm({ ...routeForm, agent_slug: val })}
                    options={agents.map((a) => ({
                      value: a.slug,
                      label: `${a.emoji} ${a.display_name}`,
                      hint: `@${a.slug}`,
                    }))}
                  />
                  <Select
                    variant="field"
                    label={c.studioRouteThreadLabel}
                    value={routeForm.thread_id}
                    onChange={(val) => setRouteForm({ ...routeForm, thread_id: val })}
                    options={[
                      { value: '', label: c.studioRouteAutoThread },
                      ...threads.map((th) => ({
                        value: String(th.id),
                        label: `#${th.id} ${th.title}`,
                      })),
                    ]}
                  />
                </div>
                <div className="checkbox-row">
                  <label className="check-inline">
                    <input
                      type="checkbox"
                      checked={routeForm.send_progress}
                      onChange={(e) =>
                        setRouteForm({ ...routeForm, send_progress: e.target.checked })
                      }
                    />
                    <span>{lang === 'vi' ? 'Gửi tiến trình live' : 'Send live progress'}</span>
                  </label>
                  <label className="check-inline">
                    <input
                      type="checkbox"
                      checked={routeForm.send_tool_hints}
                      onChange={(e) =>
                        setRouteForm({ ...routeForm, send_tool_hints: e.target.checked })
                      }
                    />
                    <span>{lang === 'vi' ? 'Hiện gợi ý công cụ' : 'Show tool hints'}</span>
                  </label>
                </div>
                <div className="btn-row">
                  <button
                    type="button"
                    className="btn secondary"
                    disabled={!routeForm.chat_id.trim()}
                    onClick={() => {
                      void onCreateTelegramRoute({
                        transport_mode: tgForm.transport_mode,
                        policy: tgForm.chat_policy,
                        chat_id: routeForm.chat_id.trim(),
                        message_thread_id: routeForm.message_thread_id.trim()
                          ? Number(routeForm.message_thread_id.trim())
                          : null,
                        agent_slug: routeForm.agent_slug,
                        thread_id: routeForm.thread_id ? Number(routeForm.thread_id) : null,
                        bot_name: tgForm.bot_name.trim() || '@lnc_newsman_bot',
                        send_progress: routeForm.send_progress,
                        send_tool_hints: routeForm.send_tool_hints,
                        enabled: true,
                      }).then(() =>
                        setRouteForm((f) => ({ ...f, chat_id: '', message_thread_id: '' })),
                      )
                    }}
                  >
                    + {c.studioTelegramAddRoute}
                  </button>
                </div>
              </div>
            </section>
          )}

          {activeTab === 'skills' && (
            <section className="studio-panel">
              <div className="studio-field-group">
                <h4>{c.studioAssignedWorkspacesLabel}</h4>
                <div className="chip-row">
                  {workspaces
                    .filter((w) => w.default_agent_slugs.includes(draft.slug))
                    .map((ws) => (
                      <button
                        key={ws.id}
                        type="button"
                        className="chip active"
                        onClick={() => onSelectWorkspace?.(ws.id)}
                      >
                        {ws.emoji} {ws.name}
                      </button>
                    ))}
                  {workspaces.filter((w) => w.default_agent_slugs.includes(draft.slug)).length ===
                    0 && (
                    <span className="muted-inline">
                      {lang === 'vi'
                        ? 'Chưa gắn mặc định trong không gian nào.'
                        : 'Not assigned as default in any workspace yet.'}
                    </span>
                  )}
                </div>
              </div>

              <div className="studio-field-group">
                <h4>{c.studioSkillGroupsLabel}</h4>
                <div className="tool-checklist">
                  {skillGroups.map((grp) => {
                    const checked = draft.enabled_skill_groups.includes(grp.slug)
                    return (
                      <label
                        key={grp.slug}
                        className={checked ? 'tool-check active' : 'tool-check'}
                      >
                        <input
                          type="checkbox"
                          checked={checked}
                          onChange={() => toggleSkillGroup(grp.slug)}
                        />
                        <div className="tool-check-body">
                          <div className="tool-check-head">
                            <strong>
                              {grp.icon || '📦'} {grp.display_name?.trim() || grp.name}
                            </strong>
                            <span className="scope-badge">
                              {grp.granted_count}/{grp.total_count}
                            </span>
                          </div>
                          <p>{grp.description || grp.skill_slugs.join(', ')}</p>
                        </div>
                      </label>
                    )
                  })}
                </div>
              </div>

              <div className="studio-field-group">
                <h4>{c.studioGrantedSkillsLabel}</h4>
                <div className="tool-checklist">
                  {skills.map((sk) => {
                    const checked = draft.enabled_skills.includes(sk.slug)
                    const locked = !sk.granted || !sk.enabled
                    return (
                      <label
                        key={sk.slug}
                        className={
                          locked
                            ? 'tool-check locked'
                            : checked
                              ? 'tool-check active'
                              : 'tool-check'
                        }
                      >
                        <input
                          type="checkbox"
                          checked={checked}
                          disabled={!sk.granted}
                          onChange={() => toggleSkill(sk.slug)}
                        />
                        <div className="tool-check-body">
                          <div className="tool-check-head">
                            <strong>
                              {sk.granted ? '🧩' : '🔒'} {sk.display_name?.trim() || sk.name}
                            </strong>
                            <span
                              className={
                                sk.granted ? 'tier-badge status-approved' : 'tier-badge status-local'
                              }
                            >
                              {sk.granted ? c.skillsGrantedBadge : c.skillsLockedBadge}
                            </span>
                          </div>
                          <p>
                            {stripSkillFrontmatter(
                              sk.description || (sk.granted ? sk.body_md : sk.body_preview || ''),
                            ).slice(0, 160)}
                          </p>
                        </div>
                      </label>
                    )
                  })}
                </div>
              </div>
            </section>
          )}
        </div>
      </div>
    </div>
  )
}
