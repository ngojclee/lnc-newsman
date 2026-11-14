// Canonical TypeScript types & bilingual copy for App #2 (`lnc-newsman`):
// Editorial Salon, Multi-Agent Briefings, Studio, Reader Companion & Telegram Bridge.

export type DiscussionMode =
  | 'parallel_panel'
  | 'sequential_build'
  | 'cross_debate'
  | 'mention_or_lead'

export type RuntimeKind =
  | 'partner_fast'
  | 'openhands'
  | 'codex_acp'
  | 'claude_acp'
  | 'remote_agent'

export type ReplyLanguage = 'vi' | 'en' | 'auto'

export type MemoryMode = 'app_isolated' | 'cross_app_shared'

export type ReasoningEffort = 'low' | 'medium' | 'high'

export type TelegramTransportMode =
  | 'direct_poll'
  | 'direct_webhook'
  | 'cygnus_poll'
  | 'cygnus_webhook'

export type TelegramChatPolicy = 'open' | 'mention' | 'topics_only'

export interface ResourceCtx {
  regions?: string[]
  topics?: string[]
}

export interface RemoteEndpointCfg {
  url?: string
  /** Vault reference only (e.g. vault://lnc-hub/agents/macro-argus) — never raw keys. */
  secret_ref?: string
  transport?: 'sse' | 'ws' | 'http' | string
}

export interface AppAgentConfig {
  slug: string
  display_name: string
  description: string
  emoji: string
  color: string
  avatar_url?: string | null
  reply_language: ReplyLanguage
  soul_md: string
  app_role_addendum_md: string
  sync_blueprint_to_hub: boolean
  runtime_kind: RuntimeKind
  primary_model: string
  backup_model: string
  reasoning_effort: ReasoningEffort | string
  remote_endpoint_cfg: RemoteEndpointCfg
  max_tool_steps: number
  enabled_app_tools: string[]
  enabled_mcp_tools: string[]
  enabled_skills: string[]
  enabled_skill_groups: string[]
  default_resource_ctx: ResourceCtx
  memory_mode: MemoryMode
  updated_at?: string | null
}

export interface AgentThread {
  id: number | string
  kind: 'direct' | 'group'
  title: string
  article_id?: number | null
  report_id?: number | null
  member_slugs: string[]
  discussion_mode: DiscussionMode
  lead_slug?: string | null
  workspace_id?: string | null
  pinned_skills?: string[]
  model_override?: string | null
  created_at?: string
  updated_at?: string
}

export interface TaskModelsConfig {
  thread_title?: string | null
  round_summary?: string | null
  companion_brief?: string | null
  tax_doc_digest?: string | null
}

export interface AgentWorkspace {
  id: string
  name: string
  description: string
  emoji: string
  custom_instructions_md: string
  resource_ctx: ResourceCtx
  default_skill_slugs: string[]
  default_agent_slugs: string[]
  default_discussion_mode: DiscussionMode
  is_default: boolean
  updated_at?: string | null
}

export interface HubSkillSummary {
  slug: string
  name: string
  display_name?: string
  description: string
  category: string
  group_slugs: string[]
  body_md: string
  body_preview?: string
  granted: boolean
  enabled: boolean
  version: string
  source: string
  updated_at?: string | null
}

export interface HubSkillGroupSummary {
  slug: string
  name: string
  display_name?: string
  description?: string
  category?: string
  icon?: string
  skill_slugs: string[]
  granted_count: number
  total_count: number
  granted?: boolean
}

export type AccessRequestTargetKind =
  | 'capability_bundle'
  | 'mcp_group'
  | 'skill_group'
  | 'skill'
  | 'model_alias'
  | 'channel'

export interface HubCapabilityBundleSummary {
  slug: string
  name: string
  display_name?: string
  description?: string
  icon?: string
  skill_group_slugs: string[]
  mcp_group_slugs: string[]
  allowed_model_aliases: string[]
  allowed_channels: string[]
  status: string
  granted: boolean
}

export interface HubMcpGroupSummary {
  slug: string
  name: string
  display_name?: string
  description?: string
  category?: string
  icon?: string
  status: string
  patterns: string[]
  item_count: number
  granted: boolean
}

export interface HubEffectiveBundleCache {
  astraea_bridge_ready: boolean
  ws_relay_path: string
  app_agents_proxy_path: string
  mcp_endpoint_path: string
  llm_base_path: string
  telegram_send_path: string
  telegram_replies_path: string
  capability_bundles: HubCapabilityBundleSummary[]
  mcp_groups: HubMcpGroupSummary[]
  allowed_mcp_tools: string[]
  allowed_model_aliases: string[]
  channel_grants: string[]
  synced_at: string
}

export interface BridgeRoutingSummary {
  astraea_http: 'hub' | 'direct' | 'none' | string
  astraea_ws: 'hub' | 'direct' | 'none' | string
  models: 'hub' | 'direct' | 'none' | string
  skills_and_mcp: 'hub' | 'none' | string
  telegram: 'hub_cygnus' | 'direct_cygnus' | 'direct_bot_api' | 'disabled' | string
}

export interface SkillAccessRequest {
  id: string
  target_kind: AccessRequestTargetKind | string
  target_slug: string
  reason: string
  status: string
  hub_request_id?: string | null
  approval_request_id?: string | null
  reviewed_by?: string | null
  review_note?: string | null
  created_at: string
  updated_at: string
}

export interface ProbeServiceResult {
  configured: boolean
  ok: boolean
  url_host?: string | null
  status?: number | null
  detail?: string | null
  models_count?: number | null
  relay_connected?: boolean | null
}

export interface ConnectionProbeReport {
  ok: boolean
  probed_at: string
  hub: ProbeServiceResult
  astraea: ProbeServiceResult
  llm: ProbeServiceResult
  relay_route?: 'hub' | 'direct' | 'disabled' | string
  routing?: BridgeRoutingSummary
}

export type WorkspaceSurface = 'salon' | 'agents' | 'workspaces' | 'skills' | 'models'

export interface ApprovalRequiredMeta {
  action_type: string
  summary: string
  request_ref?: string
}

export interface MergedToolInvocation {
  merge_id: string
  call_id?: string
  tool: string
  agent_slug?: string
  arguments?: Record<string, unknown> | string
  status: 'running' | 'done' | 'error' | 'approval_required'
  result_text?: string
  is_error?: boolean
  approval_required?: ApprovalRequiredMeta | null
  resource_ctx?: ResourceCtx | null
}

export interface ActiveViewContext {
  kind: 'article' | 'report' | string
  ref_id: string
  article_id?: number | null
  report_id?: number | null
  title?: string
  source_attribution?: string | null
  selected_text?: string | null
  content_text?: string
  excerpt?: string
}

export interface AgentMessage {
  id: number | string
  thread_id: number | string
  role: 'user' | 'assistant' | 'agent' | 'system' | string
  sender_slug?: string | null
  content: string
  reasoning?: string | null
  tool_calls?: MergedToolInvocation[]
  context?: ActiveViewContext | null
  round_idx?: number | null
  created_at: string
}

export type ThreadStreamEventKind =
  | 'seat_state'
  | 'text'
  | 'reasoning'
  | 'tool'
  | 'tool_result'
  | 'result'
  | 'error'

export interface ThreadStreamEvent {
  kind: ThreadStreamEventKind
  type?: ThreadStreamEventKind
  thread_id?: number | string
  round_idx?: number
  agent_slug?: string
  seat_state?: 'thinking' | 'speaking' | 'tool_calling' | 'done' | 'idle' | 'error' | string
  delta?: string
  text?: string
  content?: string
  reasoning?: string
  merge_id?: string
  call_id?: string
  tool?: string
  arguments?: Record<string, unknown> | string
  result?: unknown
  is_error?: boolean
  approval_required?: ApprovalRequiredMeta | null
  resource_ctx?: ResourceCtx | null
  message?: AgentMessage
  error?: string
}

export interface WhiteboardNote {
  id: number | string
  thread_id: number | string
  author_slug?: string | null
  content: string
  pinned: boolean
  created_at: string
}

export interface AgentSchedule {
  id: number | string
  name: string
  cron_or_interval_secs: string
  agent_slug: string
  thread_id: number | string | null
  prompt: string
  enabled: boolean
  next_run_at?: string | null
  last_run_at?: string | null
  last_status?: string | null
}

export interface TelegramRouteConfig {
  id: number | string
  transport_mode: TelegramTransportMode
  policy: TelegramChatPolicy
  chat_id: string
  message_thread_id?: number | string | null
  agent_slug: string
  thread_id: number | string | null
  bot_name: string
  enabled: boolean
}

export interface TelegramBridgeConfig {
  transport_mode: TelegramTransportMode
  telegram_route?: 'hub_cygnus' | 'direct_cygnus' | 'direct_bot_api' | 'disabled' | string
  bot_name: string
  cygnus_url: string
  chat_policy: TelegramChatPolicy
  has_bot_token: boolean
  has_webhook_secret: boolean
  has_cygnus_key: boolean
}

export interface BridgeStatus {
  mode: 'astraea_relay' | 'hub_standby' | 'disconnected'
  relay_connected: boolean
  astraea_reachable: boolean
  relay_route?: 'hub' | 'direct' | 'disabled' | string
  routing?: BridgeRoutingSummary
  hub_bundle?: HubEffectiveBundleCache
  secret_store_open?: boolean
  astraea_url?: string
  hub_url?: string
  default_model?: string
  llm_base_url?: string
  allowed_hosts?: string[]
  task_models?: TaskModelsConfig
  has_astraea_token?: boolean
  has_hub_token?: boolean
  has_llm_api_key?: boolean
  has_telegram_token?: boolean
  has_cygnus_key?: boolean
}

export interface NewsmanToolDescriptor {
  name: string
  tier: 'read' | 'write_safe' | 'write_gated'
  scope: string
  label_vi: string
  label_en: string
  desc_vi: string
  desc_en: string
}

export const NEWSMAN_APP_TOOLS: readonly NewsmanToolDescriptor[] = [
  {
    name: 'latest_articles',
    tier: 'read',
    scope: 'newsman:read',
    label_vi: 'Tìm & liệt kê bài báo',
    label_en: 'Search & list articles',
    desc_vi: 'Tìm kiếm toàn văn và duyệt tin mới trong phạm vi vùng và chủ đề được phép.',
    desc_en: 'Full-text search and latest headlines within allowed regions and topics.',
  },
  {
    name: 'article_get',
    tier: 'read',
    scope: 'newsman:read',
    label_vi: 'Đọc toàn văn bài báo',
    label_en: 'Read article full text',
    desc_vi: 'Đọc nội dung sạch đã lưu, tóm tắt nguồn và thông tin tác giả/tòa soạn.',
    desc_en: 'Read stored clean article text, feed summary, and source attribution.',
  },
  {
    name: 'documents',
    tier: 'read',
    scope: 'newsman:read',
    label_vi: 'Văn bản chính thức & Pháp quy',
    label_en: 'Official & regulatory documents',
    desc_vi: 'Tra cứu văn bản pháp quy, công báo kèm miễn trừ trách nhiệm pháp lý của hệ thống.',
    desc_en: 'Search official filings and regulatory documents with server-attached legal disclaimers.',
  },
  {
    name: 'tax_calendar',
    tier: 'read',
    scope: 'newsman:read',
    label_vi: 'Lịch thuế khai báo (Kèm miễn trừ pháp lý)',
    label_en: 'Declared tax calendar (With legal disclaimer)',
    desc_vi: 'Xem hạn nộp thuế theo gói vùng; luôn giữ nguyên trạng thái chưa xác minh (verified = false) và miễn trừ tư vấn thuế/pháp lý.',
    desc_en: 'Query declared tax deadlines; strictly preserves unverified status (verified = false) and statutory tax/legal disclaimers.',
  },
  {
    name: 'series',
    tier: 'read',
    scope: 'newsman:read',
    label_vi: 'Danh mục chuỗi số liệu vĩ mô',
    label_en: 'Macroeconomic time series catalog',
    desc_vi: 'Liệt kê các chuỗi chỉ số kinh tế, lãi suất và tỷ giá theo từng vùng.',
    desc_en: 'List macroeconomic indicators, interest rates, and FX series by region.',
  },
  {
    name: 'observations',
    tier: 'read',
    scope: 'newsman:read',
    label_vi: 'Điểm dữ liệu chuỗi thời gian',
    label_en: 'Series observations & revisions',
    desc_vi: 'Đọc các điểm số liệu lịch sử và các lần hiệu chỉnh của chuỗi vĩ mô.',
    desc_en: 'Read historical observation points and revisions for a macro series.',
  },
  {
    name: 'analyses',
    tier: 'read',
    scope: 'newsman:read',
    label_vi: 'Đọc Báo cáo & Bản tin đã lưu',
    label_en: 'Read stored briefs & analyses',
    desc_vi: 'Tra cứu các bản tin phân tích đã lưu cùng danh sách bài báo nguồn trích dẫn.',
    desc_en: 'Inspect stored analytical briefs and their cited source articles.',
  },
  {
    name: 'notes',
    tier: 'write_safe',
    scope: 'newsman:write',
    label_vi: 'Ghi chú nghiên cứu (Đồng bộ 2 chiều)',
    label_en: 'Research notes (Two-way sync)',
    desc_vi: 'Đọc hoặc thêm ghi chú nghiên cứu gắn tên trợ lý trên bài báo và báo cáo.',
    desc_en: 'Read or append analyst-attributed research notes on articles and briefs.',
  },
  {
    name: 'brief_propose',
    tier: 'write_gated',
    scope: 'newsman:write',
    label_vi: 'Đề xuất xuất bản Báo cáo (Chờ duyệt)',
    label_en: 'Propose brief publication (Requires approval)',
    desc_vi: 'Gửi bản thảo Báo cáo vào hàng đợi để chủ tòa soạn xem xét và phê duyệt trước khi đăng.',
    desc_en: 'Submit a draft brief for human editorial review and approval before publication.',
  },
] as const

export const MCP_BRIDGE_TOOLS: readonly { name: string; label_vi: string; label_en: string }[] = [
  { name: 'cortex__memory_search', label_vi: 'Tìm bộ nhớ Cortex', label_en: 'Cortex memory search' },
  { name: 'cortex__knowledge_search', label_vi: 'Tra cứu tri thức Cortex', label_en: 'Cortex knowledge search' },
  { name: 'obsidian__vault_notes', label_vi: 'Đồng bộ sổ ghi chú Obsidian', label_en: 'Obsidian notebook sync' },
] as const

/** Strip blank write-only secret fields before PUT so saved credentials are preserved. */
export function omitBlankSecrets<T extends Record<string, unknown>>(
  payload: T,
  secretKeys: readonly (keyof T)[],
): Partial<T> {
  const next: Record<string, unknown> = { ...payload }
  for (const key of secretKeys) {
    const raw = next[key as string]
    if (typeof raw === 'string') {
      const trimmed = raw.trim()
      if (!trimmed) {
        delete next[key as string]
      } else {
        next[key as string] = trimmed
      }
    }
  }
  return next as Partial<T>
}

export const AGENT_COPY = {
  vi: {
    agents: 'Bàn Trợ lý',
    navAgents: 'Bàn Trợ lý',
    agentsEyebrow: 'Tòa soạn Phân tích & Bàn tròn Trợ lý',
    agentsWorkspaceTitle: 'Bàn Trợ lý & Hội thảo Tin tức',
    agentsWorkspaceSub: 'Đối thoại 1:1 hoặc mở bàn tròn nhiều trợ lý phân tích vĩ mô, lịch thuế và văn bản pháp quy — giới hạn chặt theo vùng và chủ đề.',
    agentsTabWorkspace: 'Phòng họp Tòa soạn',
    agentsTabStudio: 'Thiết kế Trợ lý',
    bridgeRelayOnline: 'Trực tuyến · Cầu nối Relay',
    bridgeHubStandby: 'Sẵn sàng tại chỗ',
    bridgeDisconnected: 'Chưa kết nối cầu nối',
    bridgeConfigureBtn: 'Cấu hình Kết nối',
    bridgeAstraeaUrl: 'Địa chỉ máy chủ Astraea',
    bridgeHubUrl: 'Địa chỉ LNC Agent Hub',
    bridgeDefaultModel: 'Mô hình mặc định',
    bridgeAstraeaToken: 'Khóa cầu nối Astraea (Chỉ ghi)',
    bridgeHubToken: 'Khóa LNC Hub (Chỉ ghi)',
    bridgeTokenSaved: 'Khóa đã được mã hóa an toàn trên máy chủ và không hiển thị lại.',
    directThreadsTitle: 'Trao đổi 1:1',
    groupRoomsTitle: 'Bàn tròn Phân tích',
    newDirectBtn: '+ 1:1 mới',
    newGroupBtn: '+ Bàn tròn',
    threadTitleInput: 'Tiêu đề phiên phân tích…',
    pickMembersLabel: 'Chọn trợ lý tham gia',
    pickLeadLabel: 'Biên tập viên chủ trì',
    discussionModeLabel: 'Cách phối hợp thảo luận',
    modeParallelPanel: 'Hội đồng song song',
    modeParallelPanelDesc: 'Các trợ lý độc lập phân tích cùng lúc từ chung một tập tin tức và số liệu.',
    modeSequentialBuild: 'Tiếp sức bồi đắp',
    modeSequentialBuildDesc: 'Trợ lý sau đọc ý của trợ lý trước để bổ sung góc nhìn pháp lý và số liệu.',
    modeCrossDebate: 'Tranh biện 2 vòng',
    modeCrossDebateDesc: 'Vòng 1 nêu nhận định độc lập, vòng 2 phản biện chéo các giả định rủi ro.',
    modeMentionOrLead: 'Gọi tên @ hoặc Chủ trì',
    modeMentionOrLeadDesc: 'Chuyển câu hỏi tới trợ lý được nhắc tên @ hoặc người chủ trì phiên.',
    modeParallelShort: 'Song song',
    modeSequentialShort: 'Tiếp sức',
    modeDebateShort: 'Tranh biện',
    modeLeadShort: 'Chủ trì',
    createRoomAction: 'Mở phiên thảo luận',
    cancelBtn: 'Huỷ',
    railHideBtn: 'Ẩn danh sách phiên',
    railShowBtn: 'Hiện danh sách phiên',
    deleteThreadLabel: 'Xoá phiên thảo luận',
    deleteThreadConfirm: 'Bạn có chắc muốn xoá phiên thảo luận này không?',
    deleteWorkspaceConfirm: 'Bạn có chắc muốn xoá không gian làm việc này không?',
    deleteAgentConfirm: 'Bạn có chắc muốn xoá trợ lý này không?',
    noThreadsYet: 'Chưa có phiên nào. Hãy tạo trao đổi 1:1 hoặc mở bàn tròn mới.',
    selectThreadPrompt: 'Chọn một phiên thảo luận ở bên trái hoặc bắt đầu ngay bằng gợi ý bên dưới.',
    composerPlaceholder: 'Đặt câu hỏi phân tích… (Enter để gửi, Shift+Enter xuống dòng)',
    composerMentionBtn: '@ Trợ lý',
    composerSkillBtn: '/ Kỹ năng',
    composerMentionTitle: 'Chèn @ để gọi trợ lý cụ thể',
    composerSkillTitle: 'Chèn / để gắn kỹ năng chuyên đề',
    sendTurnBtn: 'Gửi',
    sendingTurnBtn: 'Đang phân tích…',
    mentionAutocompleteHint: 'Gợi ý gọi nhanh trợ lý:',
    reasoningTraceSummary: 'Mạch suy luận & đối chiếu nguồn',
    toolCallRunning: 'Đang tra cứu dữ liệu',
    toolCallDone: 'Đã tra cứu dữ liệu',
    toolCallError: 'Lỗi truy vấn',
    toolApprovalRequired: 'Chờ biên tập viên duyệt',
    v4sScopedBadge: 'Giới hạn vùng & chủ đề',
    seatThinking: 'đang suy luận…',
    seatSpeaking: 'đang soạn…',
    seatToolCalling: 'đang tra cứu nguồn…',
    seatDone: 'hoàn tất',
    seatError: 'gián đoạn',
    liveStreamBadge: 'Trực tiếp',
    whiteboardTitle: 'Bảng ghim Kết luận',
    whiteboardSub: 'Các ý chính ghim tại đây sẽ được giữ làm ngữ cảnh cho những vòng thảo luận tiếp theo.',
    whiteboardPlaceholder: 'Ghi chú nhận định vĩ mô, mốc lịch thuế hoặc kết luận cần ghim…',
    whiteboardAddBtn: 'Ghim kết luận',
    whiteboardEmpty: 'Chưa có kết luận nào được ghim trong phiên này.',
    whiteboardSummarizeBtn: 'Tóm tắt vòng này',
    whiteboardSummarizing: 'Đang tóm tắt…',
    whiteboardPinnedBadge: 'Đã ghim',
    whiteboardToggleDrawer: 'Bảng ghim',
    studioHeaderTitle: 'Thiết kế & Quản lý Trợ lý',
    studioHeaderSub: 'Tùy chỉnh vai trò, giọng văn, mô hình AI, phạm vi vùng/chủ đề được phép đọc, lịch hẹn điểm tin và kênh Telegram.',
    studioNewAgentBtn: '+ Tạo trợ lý mới',
    studioSaveAgentBtn: 'Lưu cấu hình trợ lý',
    studioDeleteAgentBtn: 'Xoá trợ lý',
    studioTabIdentity: '1. Định danh & Giọng',
    studioTabPersona: '2. Vai trò & Nguyên tắc',
    studioTabRuntime: '3. Mô hình & Bộ máy',
    studioTabScope: '4. Quyền dữ liệu & Công cụ',
    studioTabSchedules: '5. Lịch hẹn tự động',
    studioTabTelegram: '6. Kênh Telegram',
    studioSlugLabel: 'Mã định danh ngắn',
    studioDisplayNameLabel: 'Tên hiển thị',
    studioDescriptionLabel: 'Mô tả chuyên môn',
    studioEmojiLabel: 'Biểu tượng',
    studioColorLabel: 'Màu nhận diện',
    studioReplyLangLabel: 'Ngôn ngữ trả lời',
    studioLangVi: 'Tiếng Việt (Mặc định)',
    studioLangEn: 'Tiếng Anh (English)',
    studioLangAuto: 'Tự động theo người hỏi',
    studioSoulMdLabel: 'Tính cách & nguyên tắc cốt lõi',
    studioSoulMdHint: 'Định hình cách kiểm chứng nguồn tin, dẫn chiếu bài báo và giọng điệu phân tích.',
    studioRoleAddendumLabel: 'Hướng dẫn nghiệp vụ tòa soạn bổ sung',
    studioRolePreviewLabel: 'Xem trước bản chỉ dẫn tổng hợp',
    studioSyncHubLabel: 'Đồng bộ hồ sơ trợ lý lên LNC Agent Hub',
    studioRuntimeKindLabel: 'Phương thức vận hành',
    studioPrimaryModelLabel: 'Mô hình chính',
    studioBackupModelLabel: 'Mô hình dự phòng',
    studioReasoningEffortLabel: 'Độ sâu suy luận',
    studioEffortLow: 'Nhanh · Gọn',
    studioEffortMedium: 'Cân bằng (Khuyên dùng)',
    studioEffortHigh: 'Chuyên sâu · Đa bước',
    studioMaxToolStepsLabel: 'Số bước tra cứu tối đa mỗi lượt',
    studioRemoteUrlLabel: 'Địa chỉ trợ lý từ xa (tùy chọn)',
    studioSecretRefLabel: 'Đường dẫn khóa trên Hub Vault (bắt đầu bằng vault://)',
    studioSecretRefHint: 'Không bao giờ dán khóa bí mật trực tiếp tại đây; chỉ dùng tham chiếu vault:// do Hub quản lý.',
    studioSecretRefInvalid: 'Đường dẫn khóa phải bắt đầu bằng vault:// (không nhập khóa thô).',
    studioAppToolsTitle: 'Công cụ tra cứu nội bộ Tòa soạn',
    studioMcpToolsTitle: 'Công cụ kết nối tri thức mở rộng',
    studioResourceCtxTitle: 'Giới hạn vùng địa lý & chủ đề được phép đọc',
    studioAllowedRegionsLabel: 'Mã vùng cho phép (cách nhau dấu phẩy, vd: vn, us, global — để trống là mọi vùng đang bật)',
    studioAllowedTopicsLabel: 'Chủ đề cho phép (cách nhau dấu phẩy, vd: macro, tax, tech — để trống là mọi chủ đề)',
    studioTaxDisclaimerNotice: 'Nguyên tắc bắt buộc: Công cụ Lịch thuế và Văn bản pháp quy luôn giữ nguyên trạng thái chưa xác minh (verified = false) với quy tắc chưa đối chiếu cơ quan thuế, đồng thời gắn kèm miễn trừ trách nhiệm pháp lý/thuế. Trợ lý tuyệt đối không trình bày hạn nộp khai báo như tư vấn pháp lý chính thức.',
    studioMemoryModeLabel: 'Phạm vi ghi nhớ của trợ lý',
    studioMemoryIsolated: 'Riêng biệt trong Newsman (Khuyên dùng)',
    studioMemoryShared: 'Liên thông tri thức qua Cortex',
    studioSchedulesTitle: 'Lịch hẹn tự động (Điểm tin sáng & Rà soát lịch thuế)',
    studioScheduleNameLabel: 'Tên lịch hẹn',
    studioScheduleCronLabel: 'Lịch chạy (biểu thức Cron hoặc số giây)',
    studioScheduleAgentLabel: 'Trợ lý phụ trách',
    studioSchedulePromptLabel: 'Yêu cầu giao cho trợ lý',
    studioScheduleRunNow: 'Chạy thử ngay',
    studioScheduleRunning: 'Đang chạy…',
    studioScheduleLastStatus: 'Lần chạy gần nhất',
    studioTelegramModeTitle: 'Phương thức kết nối hội thoại Telegram 2 chiều',
    studioTelegramCoexistenceNotice: 'Lưu ý: Kênh gửi cảnh báo tự động một chiều (trong mục Cài đặt → Kênh Telegram) vẫn hoạt động độc lập. Phần này chỉ dành cho hội thoại 2 chiều với trợ lý theo từng chủ đề nhóm.',
    studioTelegramDirectMode: 'Kết nối Bot trực tiếp',
    studioTelegramCygnusMode: 'Qua cổng Cygnus Gateway',
    studioTelegramBotName: 'Tên tài khoản Bot (@ten_bot)',
    studioTelegramBotToken: 'Khóa Bot Telegram (Chỉ ghi — không hiển thị lại)',
    studioTelegramWebhookSecret: 'Khóa xác thực Webhook (Chỉ ghi)',
    studioTelegramCygnusUrl: 'Địa chỉ cổng Cygnus',
    studioTelegramCygnusKey: 'Khóa kết nối Cygnus (Chỉ ghi — không hiển thị lại)',
    studioTelegramPolicyLabel: 'Quy tắc phản hồi trong nhóm chat',
    studioTelegramPolicyOpen: 'Trả lời mọi tin nhắn trong nhóm đã gắn',
    studioTelegramPolicyMention: 'Chỉ trả lời khi được @nhắc tên',
    studioTelegramPolicyTopics: 'Chỉ trả lời trong đúng Chủ đề (Topic) đã gắn',
    studioTelegramRoutesTitle: 'Danh sách gắn Nhóm / Chủ đề Telegram với Trợ lý',
    studioTelegramChatId: 'Mã nhóm chat Telegram',
    studioTelegramTopicId: 'Mã chủ đề trong nhóm (nếu có)',
    studioRouteAgentLabel: 'Trợ lý tiếp nhận',
    studioRouteThreadLabel: 'Phiên hội thoại gắn kèm',
    studioRouteAutoThread: '— Tự động tạo theo chủ đề —',
    studioTelegramAddRoute: 'Thêm liên kết nhóm',
    companionToggleBtn: 'Hỏi trợ lý về bài này',
    companionToggleReportBtn: 'Hỏi trợ lý về báo cáo này',
    companionHeaderTitle: 'Trợ lý Đọc & Phân tích tin',
    companionContextBadge: 'Đọc cùng bài viết đang mở',
    companionContextReadyTitle: 'Ngữ cảnh tiêu đề bài viết',
    companionContextReadyLoaded: 'Đã nạp nội dung bài đang đọc',
    companionSelectedTextLabel: 'Đoạn đang chọn:',
    companionArticleExcerptLabel: 'Trích đoạn bài viết:',
    companionQuickExplain: 'Phân tích tác động & bối cảnh',
    companionQuickSummarize: 'Tóm tắt 3 ý cốt lõi',
    companionQuickVerifyTax: 'Rà soát lưu ý pháp lý / lịch thuế liên quan',
    companionJumpArticle: 'Mở bài báo',
    surfaceSalon: '💬 Thảo luận',
    surfaceAgents: '👥 Trợ lý & Thiết kế',
    surfaceWorkspaces: '🗂 Không gian',
    surfaceSkills: '🧩 Kho Kỹ năng',
    surfaceModels: '🔌 Mô hình & Kết nối',
    workspaceAll: 'Mọi không gian',
    workspaceActiveLabel: 'Không gian làm việc hiện tại',
    workspaceDefaultBadge: 'Mặc định',
    workspaceNewBtn: '+ Không gian mới',
    workspaceSaveBtn: 'Lưu Không gian',
    workspaceDeleteBtn: 'Xoá Không gian',
    workspaceNameLabel: 'Tên không gian biên tập',
    workspaceDescLabel: 'Mô tả mục tiêu & phạm vi',
    workspaceEmojiLabel: 'Biểu tượng',
    workspaceInstructionsLabel: 'Hướng dẫn biên tập riêng cho không gian này',
    workspaceDefaultSkillsLabel: 'Kỹ năng gắn sẵn mặc định',
    workspaceDefaultAgentsLabel: 'Trợ lý tham gia mặc định',
    workspaceDefaultModeLabel: 'Chế độ thảo luận mặc định',
    workspaceRegionsLabel: 'Giới hạn vùng dữ liệu (cách nhau dấu phẩy)',
    workspaceTopicsLabel: 'Giới hạn chủ đề (cách nhau dấu phẩy)',
    workspaceIsDefaultLabel: 'Đặt làm Không gian mặc định khi mở Bàn Trợ lý',
    workspaceTaxReminder: 'Lưu ý tuân thủ: Trợ lý chỉ được đọc phần dữ liệu nằm đồng thời trong phạm vi của Không gian và của từng Trợ lý; mọi trích dẫn lịch thuế đều giữ nguyên cờ chưa xác minh (verified = false) và miễn trừ trách nhiệm pháp lý.',
    studioTabSkills: '7. Kỹ năng & Gói chuyên đề',
    studioAssignedWorkspacesLabel: 'Các không gian đang chọn trợ lý này',
    studioSkillGroupsLabel: 'Gói kỹ năng chuyên đề được bật',
    studioGrantedSkillsLabel: 'Kỹ năng lẻ được bật cho trợ lý',
    traceSummaryLabel: '🧠 Suy luận',
    traceToolsUnit: 'bước tra cứu',
    panelTabTrace: 'Mạch suy luận',
    panelTabContext: 'Bảng ghim & Ngữ cảnh',
    panelNoTraceYet: 'Bấm vào nhãn "🧠 Suy luận" trên câu trả lời của trợ lý để xem chi tiết lập luận và dữ liệu đã đối chiếu.',
    composerModelLabel: 'Chọn mô hình cho lượt hỏi này',
    composerModelDefault: 'Theo mặc định của trợ lý',
    composerBudgetLabel: 'Độ dài câu hỏi',
    composerSkillAttachHint: 'Bấm @ gọi trợ lý · / gắn kỹ năng',
    actionCopy: 'Sao chép',
    actionQuote: 'Trích dẫn',
    actionAskPeer: 'Hỏi tiếp @',
    starterMacroToday: 'Điểm tin vĩ mô hôm nay',
    starterTaxReview: 'Rà soát các mốc lịch thuế cần lưu ý',
    starterPolicyWatch: 'Tổng hợp văn bản pháp quy nổi bật tuần này',
    skillsSyncBtn: 'Đồng bộ từ Hub',
    skillsSyncing: 'Đang đồng bộ…',
    skillsAllGroups: 'Tất cả nhóm',
    skillsSearchPlaceholder: 'Tìm kỹ năng hoặc gói chuyên đề…',
    skillsEmpty: 'Không có kỹ năng nào khớp bộ lọc hiện tại.',
    skillsGrantedBadge: 'Đã mở quyền',
    skillsLockedBadge: 'Đang khóa · Cần cấp quyền',
    skillsEnableToggle: 'Đang bật',
    skillsDisableToggle: 'Đang tắt tạm thời',
    skillsRequestAccessBtn: 'Xin mở quyền Kỹ năng',
    skillsRequestModalTitle: 'Gửi yêu cầu mở quyền tới LNC Agent Hub',
    skillsRequestKindLabel: 'Loại quyền muốn xin',
    skillsRequestKindSkill: 'Một kỹ năng lẻ',
    skillsRequestKindGroup: 'Trọn gói kỹ năng theo nhóm',
    skillsRequestTargetLabel: 'Tên mã kỹ năng hoặc gói',
    skillsRequestReasonLabel: 'Nhu cầu sử dụng của tòa soạn',
    skillsRequestSubmitBtn: 'Gửi yêu cầu',
    skillsRequestsHistoryTitle: 'Lịch sử yêu cầu mở quyền',
    skillsNoRequests: 'Chưa có yêu cầu mở quyền nào.',
    skillsPlaybookPlaceholder: 'Chọn một kỹ năng ở danh sách bên trái để đọc nội dung hướng dẫn.',
    targetKindSkillTag: 'Kỹ năng',
    targetKindGroupTag: 'Gói kỹ năng',
    modelsProbeBtn: 'Kiểm tra kết nối',
    modelsProbing: 'Đang kiểm tra…',
    modelsProbeLastRun: 'Kiểm tra lúc',
    modelsServiceHub: 'Trung tâm LNC Agent Hub',
    modelsServiceAstraea: 'Cầu nối trực tuyến Astraea',
    modelsServiceLlm: 'Dịch vụ Mô hình Ngôn ngữ',
    modelsConnectedOk: 'Hoạt động tốt',
    modelsDisconnected: 'Chưa kết nối được',
    modelsNotConfigured: 'Chưa thiết lập',
    modelsCatalogTitle: 'Danh sách Mô hình sẵn có',
    taskModelsTitle: 'Chọn Mô hình cho từng tác vụ phụ trợ',
    taskModelsSaveBtn: 'Lưu cấu hình Mô hình & Kết nối',
    taskModelThreadTitle: 'Đặt tiêu đề cuộc trò chuyện tự động',
    taskModelRoundSummary: 'Tóm tắt kết luận vòng thảo luận',
    taskModelCompanionBrief: 'Phân tích nhanh bài viết đang đọc',
    taskModelTaxDocDigest: 'Tóm tắt văn bản & đối chiếu lịch thuế',
    bridgeLlmBaseUrl: 'Địa chỉ máy chủ Mô hình (tương thích OpenAI)',
    bridgeLlmApiKey: 'Khóa truy cập Mô hình (Chỉ ghi — không hiển thị lại)',
    bridgeAllowedHosts: 'Danh sách máy chủ nội bộ tin cậy (cách nhau dấu phẩy)',
    companionSkillChipLabel: 'Gắn kỹ năng nhanh',
    companionWorkspaceBadge: 'Không gian:',
    skillsGroupPacksTitle: 'Các gói kỹ năng chuyên đề từ Hub',
    skillsGroupRequestBtn: 'Xin mở cả gói',
    skillsGroupGrantedBadge: '✓ Đã mở trọn gói',
    skillsAttachToSalonBtn: 'Đưa vào cuộc thảo luận',
    skillsLockedPreviewTitle: 'Tóm tắt giới thiệu (Kỹ năng chưa mở quyền)',
    skillsLockedPreviewEmpty:
      'Chưa có đoạn giới thiệu cho kỹ năng này. Hãy bấm Đồng bộ từ Hub hoặc gửi yêu cầu mở quyền để xem toàn bộ hướng dẫn.',
    skillsRequestSkillBtn: 'Xin mở kỹ năng này',
    skillsRequestByGroupBtn: 'Xin mở theo gói nhóm',
    skillsInspectTaxonomyBtn: 'Xem danh mục Vùng & Chủ đề',
    skillsAttachedNotice: 'Đã gắn kỹ năng vào phiên thảo luận hiện tại.',
    skillsReqStatusApproved: 'Đã duyệt',
    skillsReqStatusPending: 'Chờ duyệt',
    skillsReqStatusRejected: 'Từ chối',
    skillsReqStatusLocal: 'Đã lưu tạm tại máy',
    skillsReqReviewedBy: 'Người duyệt',
    skillsReqApprovalId: 'Mã phiếu',
    salonScopeWorkspaceLabel: 'Không gian làm việc',
    salonScopeEffectiveLabel: 'Phạm vi dữ liệu áp dụng',
    salonScopeRegionsAll: 'Mọi vùng đang bật',
    salonScopeTopicsAll: 'Mọi chủ đề',
    salonScopePinnedSkills: 'Kỹ năng đang gắn',
    salonScopeNoWorkspace: 'Toàn tòa soạn',
    salonScopeDisjoint: 'Không có vùng chung',
    workspaceLaunchSalonBtn: 'Mở phòng thảo luận ngay',
    connHubFirstSub:
      'Cấu hình kết nối thủ công: Điền Địa chỉ Hub + Khóa LNC Hub để định tuyến Astraea HTTP/WS, Model Fabric, Skill/MCP và Telegram Cygnus qua Hub (hoặc mở mục Ngoại lệ bên dưới nếu cần nối trực tiếp từng nhánh).',
    connSyncAllBtn: '🔄 Đồng bộ Hub & Thử kết nối',
    connDirectOverridesTitle: 'Ngoại lệ nối trực tiếp từng nhánh (Tùy chọn)',
    connDirectOverridesHint: 'Để trống để mọi nhánh tự động đi qua LNC Agent Hub. Chỉ điền khi cần vượt trạm trực tiếp tới Astraea hoặc OpenAI-compatible Endpoint riêng.',
    routeMatrixTitle: 'Ma trận Định tuyến 5 Nhánh (Star Topology)',
    routeLegAstraeaHttp: '1. Astraea HTTP (Turns)',
    routeLegAstraeaWs: '2. Astraea Relay (WSS)',
    routeLegModels: '3. Model Fabric (LLM)',
    routeLegSkillsMcp: '4. Gói Quyền, Skill & MCP',
    routeLegTelegram: '5. Kênh Telegram (Cygnus)',
    routeViaHub: 'Qua LNC Hub',
    routeViaDirect: 'Nối trực tiếp (Override)',
    routeViaHubCygnus: 'Cygnus qua Hub',
    routeViaDirectCygnus: 'Cygnus trực tiếp',
    routeViaDirectBot: 'Telegram Bot API trực tiếp',
    routeDisabled: 'Chưa bật',
    skillsBundlesTitle: 'Gói Quyền Hợp nhất (Capability Bundles)',
    skillsBundleRequestBtn: 'Xin mở gói quyền',
    skillsBundleGrantedBadge: '✓ Đã cấp gói quyền',
    skillsMcpGroupsTitle: 'Nhóm công cụ Business MCP được cấp phép',
    skillsMcpGroupRequestBtn: 'Xin mở nhóm MCP',
    skillsMcpGrantedBadge: '✓ Đã cấp nhóm MCP',
    skillsModelsGrantedTitle: 'Mô hình được cấp phép qua Hub',
    skillsChannelsGrantedTitle: 'Kênh giao tiếp được cấp',
    skillsGrantedEmpty: 'Chưa có mục nào được đồng bộ từ LNC Hub.',
    targetKindBundleTag: 'Gói Quyền',
    targetKindMcpGroupTag: 'Nhóm MCP',
    targetKindModelTag: 'Mô hình',
    targetKindChannelTag: 'Kênh',
    skillsRequestKindBundle: 'Gói Quyền hợp nhất (Capability Bundle)',
    skillsRequestKindMcpGroup: 'Nhóm Business MCP (MCP Group)',
    skillsRequestKindModel: 'Mô hình AI (Model Alias)',
    skillsRequestKindChannel: 'Kênh liên lạc (Channel)',
    studioMcpToolsHintHub: 'Danh sách công cụ MCP được hợp nhất từ Business MCP Groups đã cấp trên Hub.',
    studioTelegramRouteLabel: 'Tuyến gửi/nhận Telegram',
    studioTelegramHubBrokerNote: 'Để trống Cygnus URL để gửi/nhận qua Hub Cygnus Broker (chỉ điền URL khi muốn vượt trạm tới Cygnus riêng).',
    connDiagTitle: 'Tổng quan trạng thái vận hành',
    connDiagSub: 'Khóa bí mật được lưu mã hóa an toàn trên máy chủ; mọi kết nối ra ngoài đều đi qua bộ kiểm duyệt địa chỉ mạng của ứng dụng.',
    diagModePrefix: 'Chế độ',
    diagSecretReady: 'Kho khóa mã hóa: Đã mở an toàn',
    diagSecretSeed: 'Kho khóa mã hóa: Chỉ đọc cấu hình khởi tạo',
    diagAgentsCount: 'Trợ lý',
    diagWorkspacesCount: 'Không gian',
    diagSkillsCount: 'Kỹ năng đã mở',
    diagHostsCount: 'Máy chủ tin cậy',
    settingsAgentsCardTitle: 'Bàn Trợ lý Đa tác tử, Mô hình & Kết nối Hub',
    settingsAgentsCardSub: 'Thiết lập kết nối LNC Agent Hub, cầu nối Astraea, mô hình cho từng tác vụ và kho kỹ năng chuyên đề.',
    settingsAgentsCardBtn: 'Mở Cấu hình Mô hình & Kết nối',
  },
  en: {
    agents: 'Agent Desk',
    navAgents: 'Agent Desk',
    agentsEyebrow: 'Editorial Newsroom & Analyst Roundtables',
    agentsWorkspaceTitle: 'Agent Desk & Group Briefings',
    agentsWorkspaceSub: '1:1 newsroom analysts and multi-agent macro, tax, and policy roundtables grounded in stored articles — scoped cleanly by region and topic.',
    agentsTabWorkspace: 'Editorial Salon',
    agentsTabStudio: 'Agent Studio',
    bridgeRelayOnline: 'Online · Live Relay',
    bridgeHubStandby: 'Local Engine Ready',
    bridgeDisconnected: 'Bridge Offline',
    bridgeConfigureBtn: 'Connection Settings',
    bridgeAstraeaUrl: 'Astraea Server URL',
    bridgeHubUrl: 'LNC Agent Hub URL',
    bridgeDefaultModel: 'Default Model',
    bridgeAstraeaToken: 'Astraea Bridge Token (Write-only)',
    bridgeHubToken: 'LNC Hub Token (Write-only)',
    bridgeTokenSaved: 'Encrypted securely on the server and never shown again.',
    directThreadsTitle: 'Direct 1:1 Briefings',
    groupRoomsTitle: 'Group Roundtables',
    newDirectBtn: '+ New 1:1',
    newGroupBtn: '+ Roundtable',
    threadTitleInput: 'Discussion or briefing title…',
    pickMembersLabel: 'Participating analysts',
    pickLeadLabel: 'Lead managing editor',
    discussionModeLabel: 'Discussion format',
    modeParallelPanel: 'Parallel Panel',
    modeParallelPanelDesc: 'Analysts weigh in independently at the same time from the shared news and data snapshot.',
    modeSequentialBuild: 'Sequential Build',
    modeSequentialBuildDesc: 'Ordered turns where each analyst builds on prior macro and regulatory points without repeating.',
    modeCrossDebate: 'Two-Round Debate',
    modeCrossDebateDesc: 'Round 1 independent theses followed by Round 2 cross-examination of key risks.',
    modeMentionOrLead: 'Mention @ or Lead Editor',
    modeMentionOrLeadDesc: 'Routes questions to @mentioned analysts or the room’s lead editor.',
    modeParallelShort: 'Parallel',
    modeSequentialShort: 'Sequential',
    modeDebateShort: 'Debate',
    modeLeadShort: 'Lead / @',
    createRoomAction: 'Start discussion',
    cancelBtn: 'Cancel',
    railHideBtn: 'Hide thread list',
    railShowBtn: 'Show thread list',
    deleteThreadLabel: 'Delete discussion',
    deleteThreadConfirm: 'Are you sure you want to delete this discussion?',
    deleteWorkspaceConfirm: 'Are you sure you want to delete this workspace?',
    deleteAgentConfirm: 'Are you sure you want to delete this analyst?',
    noThreadsYet: 'No discussions yet. Start a 1:1 chat or open a group roundtable.',
    selectThreadPrompt: 'Choose a discussion on the left or start immediately with a prompt below.',
    composerPlaceholder: 'Ask an analytical question… (Enter to send, Shift+Enter for new line)',
    composerMentionBtn: '@ Analyst',
    composerSkillBtn: '/ Skill',
    composerMentionTitle: 'Insert @ to mention an analyst',
    composerSkillTitle: 'Insert / to attach a granted skill',
    sendTurnBtn: 'Send',
    sendingTurnBtn: 'Analysing…',
    mentionAutocompleteHint: 'Quick mention:',
    reasoningTraceSummary: 'Reasoning & source verification',
    toolCallRunning: 'Checking newsroom sources',
    toolCallDone: 'Sources checked',
    toolCallError: 'Lookup failed',
    toolApprovalRequired: 'Awaiting editor approval',
    v4sScopedBadge: 'Region & topic scoped',
    seatThinking: 'thinking…',
    seatSpeaking: 'drafting…',
    seatToolCalling: 'checking sources…',
    seatDone: 'done',
    seatError: 'interrupted',
    liveStreamBadge: 'Live',
    whiteboardTitle: 'Pinned Takeaways',
    whiteboardSub: 'Key conclusions pinned here stay in context for subsequent discussion rounds.',
    whiteboardPlaceholder: 'Jot down a macro takeaway, tax deadline note, or conclusion to pin…',
    whiteboardAddBtn: 'Pin note',
    whiteboardEmpty: 'No takeaways pinned in this discussion yet.',
    whiteboardSummarizeBtn: 'Summarize round',
    whiteboardSummarizing: 'Summarizing…',
    whiteboardPinnedBadge: 'Pinned',
    whiteboardToggleDrawer: 'Whiteboard',
    studioHeaderTitle: 'Analyst Studio',
    studioHeaderSub: 'Customize analyst identity, editorial voice, AI models, allowed regions and topics, recurring briefings, and Telegram chat.',
    studioNewAgentBtn: '+ New Analyst',
    studioSaveAgentBtn: 'Save Analyst',
    studioDeleteAgentBtn: 'Delete Analyst',
    studioTabIdentity: '1. Identity & Voice',
    studioTabPersona: '2. Role & Principles',
    studioTabRuntime: '3. Models & Engine',
    studioTabScope: '4. Data Scope & Tools',
    studioTabSchedules: '5. Automated Schedules',
    studioTabTelegram: '6. Telegram Channel',
    studioSlugLabel: 'Short handle',
    studioDisplayNameLabel: 'Display name',
    studioDescriptionLabel: 'Specialty summary',
    studioEmojiLabel: 'Icon',
    studioColorLabel: 'Accent color',
    studioReplyLangLabel: 'Reply language',
    studioLangVi: 'Vietnamese (Default)',
    studioLangEn: 'English',
    studioLangAuto: 'Match reader language',
    studioSoulMdLabel: 'Core persona & editorial standards',
    studioSoulMdHint: 'Shapes citation rigor, source verification habits, and writing tone.',
    studioRoleAddendumLabel: 'Additional newsroom instructions',
    studioRolePreviewLabel: 'Combined instruction preview',
    studioSyncHubLabel: 'Sync analyst profile with LNC Agent Hub',
    studioRuntimeKindLabel: 'Execution engine',
    studioPrimaryModelLabel: 'Primary model',
    studioBackupModelLabel: 'Fallback model',
    studioReasoningEffortLabel: 'Reasoning depth',
    studioEffortLow: 'Fast · Concise',
    studioEffortMedium: 'Balanced (Recommended)',
    studioEffortHigh: 'Deep · Multi-step',
    studioMaxToolStepsLabel: 'Maximum source lookups per turn',
    studioRemoteUrlLabel: 'Remote agent URL (optional)',
    studioSecretRefLabel: 'Hub Vault secret path (starts with vault://)',
    studioSecretRefHint: 'Never paste raw API keys here; use a vault:// path managed on LNC Agent Hub.',
    studioSecretRefInvalid: 'Vault reference must start with vault:// (raw keys are not allowed).',
    studioAppToolsTitle: 'Newsroom research tools',
    studioMcpToolsTitle: 'Connected knowledge tools',
    studioResourceCtxTitle: 'Allowed regions & topics',
    studioAllowedRegionsLabel: 'Allowed region codes (comma-separated, e.g. vn, us, global — leave blank for all enabled regions)',
    studioAllowedTopicsLabel: 'Allowed topics (comma-separated, e.g. macro, tax, tech — leave blank for all topics)',
    studioTaxDisclaimerNotice: 'Mandatory rule: Tax Calendar and Official Documents always keep `verified = false` on rules not yet checked with an authority and attach the statutory legal/tax disclaimer. Analysts never present unverified deadlines as formal legal or tax advice.',
    studioMemoryModeLabel: 'Memory scope',
    studioMemoryIsolated: 'Isolated to Newsman (Recommended)',
    studioMemoryShared: 'Shared across apps via Cortex',
    studioSchedulesTitle: 'Automated Briefings (Morning Digest & Tax Deadline Checks)',
    studioScheduleNameLabel: 'Schedule name',
    studioScheduleCronLabel: 'Timing (Cron expression or interval in seconds)',
    studioScheduleAgentLabel: 'Assigned analyst',
    studioSchedulePromptLabel: 'Briefing instructions',
    studioScheduleRunNow: 'Run now',
    studioScheduleRunning: 'Running…',
    studioScheduleLastStatus: 'Last run',
    studioTelegramModeTitle: 'Two-way Telegram conversation mode',
    studioTelegramCoexistenceNotice: 'Note: Newsman’s one-way alert channel (under Settings → Telegram channel) stays completely separate. This bridge is dedicated to two-way analyst chats in bound group topics.',
    studioTelegramDirectMode: 'Direct Telegram Bot',
    studioTelegramCygnusMode: 'Via Cygnus Gateway',
    studioTelegramBotName: 'Bot handle (@bot_name)',
    studioTelegramBotToken: 'Telegram Bot Token (Write-only — never shown again)',
    studioTelegramWebhookSecret: 'Webhook Secret (Write-only)',
    studioTelegramCygnusUrl: 'Cygnus Gateway URL',
    studioTelegramCygnusKey: 'Cygnus Access Key (Write-only — never shown again)',
    studioTelegramPolicyLabel: 'Group reply rule',
    studioTelegramPolicyOpen: 'Reply to all messages in bound group',
    studioTelegramPolicyMention: 'Reply only when @mentioned',
    studioTelegramPolicyTopics: 'Reply only inside bound forum topics',
    studioTelegramRoutesTitle: 'Telegram Group & Topic Bindings',
    studioTelegramChatId: 'Telegram Group ID',
    studioTelegramTopicId: 'Forum Topic ID (optional)',
    studioRouteAgentLabel: 'Assigned analyst',
    studioRouteThreadLabel: 'Linked discussion',
    studioRouteAutoThread: '— Auto-create per topic —',
    studioTelegramAddRoute: 'Add group binding',
    companionToggleBtn: 'Ask Analyst about this article',
    companionToggleReportBtn: 'Ask Analyst about this brief',
    companionHeaderTitle: 'Reading Companion',
    companionContextBadge: 'Reading alongside current page',
    companionContextReadyTitle: 'Headline context ready',
    companionContextReadyLoaded: 'Article text attached',
    companionSelectedTextLabel: 'Selected passage:',
    companionArticleExcerptLabel: 'Article excerpt:',
    companionQuickExplain: 'Analyze macro & market impact',
    companionQuickSummarize: 'Summarize 3 core takeaways',
    companionQuickVerifyTax: 'Check related legal / tax deadlines',
    companionJumpArticle: 'Open article',
    surfaceSalon: '💬 Salon',
    surfaceAgents: '👥 Analysts & Studio',
    surfaceWorkspaces: '🗂 Workspaces',
    surfaceSkills: '🧩 Skill Hub',
    surfaceModels: '🔌 Models & Links',
    workspaceAll: 'All Workspaces',
    workspaceActiveLabel: 'Current workspace',
    workspaceDefaultBadge: 'Default',
    workspaceNewBtn: '+ New Workspace',
    workspaceSaveBtn: 'Save Workspace',
    workspaceDeleteBtn: 'Delete Workspace',
    workspaceNameLabel: 'Workspace name',
    workspaceDescLabel: 'Purpose & coverage',
    workspaceEmojiLabel: 'Icon',
    workspaceInstructionsLabel: 'Editorial guidelines for this workspace',
    workspaceDefaultSkillsLabel: 'Default attached skills',
    workspaceDefaultAgentsLabel: 'Default participating analysts',
    workspaceDefaultModeLabel: 'Default discussion format',
    workspaceRegionsLabel: 'Allowed regions (comma-separated)',
    workspaceTopicsLabel: 'Allowed topics (comma-separated)',
    workspaceIsDefaultLabel: 'Open this workspace by default on Agent Desk',
    workspaceTaxReminder: 'Compliance note: Analysts only read items permitted by both the Workspace and their own scope; tax deadline citations always preserve unverified status (verified = false) and statutory legal disclaimers.',
    studioTabSkills: '7. Skills & Topic Packs',
    studioAssignedWorkspacesLabel: 'Workspaces featuring this analyst',
    studioSkillGroupsLabel: 'Enabled Hub skill packs',
    studioGrantedSkillsLabel: 'Individually enabled skills',
    traceSummaryLabel: '🧠 Reasoning',
    traceToolsUnit: 'lookups',
    panelTabTrace: 'Reasoning',
    panelTabContext: 'Whiteboard & Scope',
    panelNoTraceYet: 'Click a "🧠 Reasoning" badge on any analyst reply to inspect its thinking and source lookups.',
    composerModelLabel: 'Model for this turn',
    composerModelDefault: 'Analyst default model',
    composerBudgetLabel: 'Prompt length',
    composerSkillAttachHint: 'Click @ to mention · / to attach a skill',
    actionCopy: 'Copy',
    actionQuote: 'Quote',
    actionAskPeer: 'Ask @',
    starterMacroToday: 'Macro brief for today',
    starterTaxReview: 'Review upcoming tax deadlines to verify',
    starterPolicyWatch: 'Highlight key regulatory filings this week',
    skillsSyncBtn: 'Sync with Hub',
    skillsSyncing: 'Syncing…',
    skillsAllGroups: 'All Packs',
    skillsSearchPlaceholder: 'Search skills or topic packs…',
    skillsEmpty: 'No skills match your current filter.',
    skillsGrantedBadge: 'Granted',
    skillsLockedBadge: 'Locked · Needs Approval',
    skillsEnableToggle: 'Enabled',
    skillsDisableToggle: 'Paused locally',
    skillsRequestAccessBtn: 'Request Skill Access',
    skillsRequestModalTitle: 'Request Skill Access from LNC Agent Hub',
    skillsRequestKindLabel: 'Access type',
    skillsRequestKindSkill: 'Single skill',
    skillsRequestKindGroup: 'Full skill pack',
    skillsRequestTargetLabel: 'Skill or pack identifier',
    skillsRequestReasonLabel: 'How the newsroom will use it',
    skillsRequestSubmitBtn: 'Send Request',
    skillsRequestsHistoryTitle: 'Access Request History',
    skillsNoRequests: 'No access requests submitted yet.',
    skillsPlaybookPlaceholder: 'Select a skill on the left to read its playbook guide.',
    targetKindSkillTag: 'Skill',
    targetKindGroupTag: 'Skill pack',
    modelsProbeBtn: 'Test Connections',
    modelsProbing: 'Testing…',
    modelsProbeLastRun: 'Last checked',
    modelsServiceHub: 'LNC Agent Hub',
    modelsServiceAstraea: 'Astraea Live Relay',
    modelsServiceLlm: 'Language Model Service',
    modelsConnectedOk: 'Connected',
    modelsDisconnected: 'Unreachable',
    modelsNotConfigured: 'Not set up',
    modelsCatalogTitle: 'Available Models',
    taskModelsTitle: 'Models by Background Task',
    taskModelsSaveBtn: 'Save Models & Connection Settings',
    taskModelThreadTitle: 'Automatic conversation titles',
    taskModelRoundSummary: 'Roundtable takeaway summaries',
    taskModelCompanionBrief: 'Quick reader companion briefs',
    taskModelTaxDocDigest: 'Regulatory & tax deadline digests',
    bridgeLlmBaseUrl: 'Model Server URL (OpenAI-compatible)',
    bridgeLlmApiKey: 'Model API Key (Write-only — never shown again)',
    bridgeAllowedHosts: 'Trusted internal hosts (comma-separated)',
    companionSkillChipLabel: 'Quick skills',
    companionWorkspaceBadge: 'Workspace:',
    skillsGroupPacksTitle: 'Topic Skill Packs from Hub',
    skillsGroupRequestBtn: 'Request Full Pack',
    skillsGroupGrantedBadge: '✓ Pack Granted',
    skillsAttachToSalonBtn: 'Use in Chat',
    skillsLockedPreviewTitle: 'Overview (Locked Skill)',
    skillsLockedPreviewEmpty:
      'No summary cached yet. Sync with Hub or request access to unlock the full skill guide.',
    skillsRequestSkillBtn: 'Request This Skill',
    skillsRequestByGroupBtn: 'Request Parent Pack',
    skillsInspectTaxonomyBtn: 'View Regions & Topics',
    skillsAttachedNotice: 'Attached skill to the current discussion.',
    skillsReqStatusApproved: 'Approved',
    skillsReqStatusPending: 'Pending review',
    skillsReqStatusRejected: 'Declined',
    skillsReqStatusLocal: 'Saved locally',
    skillsReqReviewedBy: 'Reviewed by',
    skillsReqApprovalId: 'Reference',
    salonScopeWorkspaceLabel: 'Active workspace',
    salonScopeEffectiveLabel: 'Active data coverage',
    salonScopeRegionsAll: 'All enabled regions',
    salonScopeTopicsAll: 'All topics',
    salonScopePinnedSkills: 'Active skills',
    salonScopeNoWorkspace: 'Entire newsroom',
    salonScopeDisjoint: 'No shared coverage',
    workspaceLaunchSalonBtn: 'Open Discussion in Workspace',
    connHubFirstSub:
      'Manual Connection Setup: Enter Hub URL + LNC Hub Token to route Astraea HTTP/WS, Model Fabric, Skills/MCP, and Cygnus Telegram through LNC Agent Hub (or expand Direct Overrides below to connect a specific leg directly).',
    connSyncAllBtn: '🔄 Sync Hub & Probe',
    connDirectOverridesTitle: 'Per-Leg Direct Overrides (Optional)',
    connDirectOverridesHint: 'Leave these fields blank to route everything cleanly via LNC Agent Hub. Fill only to bypass the Hub for a specific direct endpoint.',
    routeMatrixTitle: '5-Leg Star Topology Routing Matrix',
    routeLegAstraeaHttp: '1. Astraea HTTP (Turns)',
    routeLegAstraeaWs: '2. Astraea Relay (WSS)',
    routeLegModels: '3. Model Fabric (LLM)',
    routeLegSkillsMcp: '4. Bundles, Skills & MCP',
    routeLegTelegram: '5. Telegram Channel (Cygnus)',
    routeViaHub: 'Via LNC Hub',
    routeViaDirect: 'Direct Override',
    routeViaHubCygnus: 'Cygnus via Hub',
    routeViaDirectCygnus: 'Direct Cygnus',
    routeViaDirectBot: 'Direct Bot API',
    routeDisabled: 'Disabled',
    skillsBundlesTitle: 'Unified Capability Bundles',
    skillsBundleRequestBtn: 'Request Bundle',
    skillsBundleGrantedBadge: '✓ Bundle Granted',
    skillsMcpGroupsTitle: 'Granted Business MCP Groups',
    skillsMcpGroupRequestBtn: 'Request MCP Group',
    skillsMcpGrantedBadge: '✓ MCP Group Granted',
    skillsModelsGrantedTitle: 'Hub-Granted Model Aliases',
    skillsChannelsGrantedTitle: 'Granted Channels',
    skillsGrantedEmpty: 'No items synced from LNC Hub yet.',
    targetKindBundleTag: 'Bundle',
    targetKindMcpGroupTag: 'MCP Group',
    targetKindModelTag: 'Model',
    targetKindChannelTag: 'Channel',
    skillsRequestKindBundle: 'Unified Capability Bundle',
    skillsRequestKindMcpGroup: 'Business MCP Group',
    skillsRequestKindModel: 'Model Alias',
    skillsRequestKindChannel: 'Channel Grant',
    studioMcpToolsHintHub: 'Dynamically populated from Business MCP Groups granted via LNC Agent Hub.',
    studioTelegramRouteLabel: 'Active Telegram Route',
    studioTelegramHubBrokerNote: 'Leave Cygnus URL empty to route Telegram through the Hub Cygnus broker (fill only for a direct override).',
    connDiagTitle: 'Connection & Security Summary',
    connDiagSub: 'Keys are encrypted on the server and outbound requests pass through the application’s safe-network guard.',
    diagModePrefix: 'Status',
    diagSecretReady: 'Encrypted key store: Ready',
    diagSecretSeed: 'Encrypted key store: Using startup config',
    diagAgentsCount: 'Analysts',
    diagWorkspacesCount: 'Workspaces',
    diagSkillsCount: 'Granted skills',
    diagHostsCount: 'Trusted hosts',
    settingsAgentsCardTitle: 'Multi-Agent Salon, Task Models & Hub Connections',
    settingsAgentsCardSub: 'Configure LNC Agent Hub, Astraea live relay, task-specific models, and topic skill packs.',
    settingsAgentsCardBtn: 'Open Model & Connection Settings',
  },
} as const

export type AgentCopy = (typeof AGENT_COPY)[keyof typeof AGENT_COPY]

export function formatDiscussionModeShort(mode: DiscussionMode, copy: AgentCopy): string {
  switch (mode) {
    case 'parallel_panel':
      return copy.modeParallelShort
    case 'sequential_build':
      return copy.modeSequentialShort
    case 'cross_debate':
      return copy.modeDebateShort
    case 'mention_or_lead':
    default:
      return copy.modeLeadShort
  }
}

export function formatBridgeModeLabel(
  status: Pick<BridgeStatus, 'mode' | 'relay_connected'>,
  copy: AgentCopy,
): string {
  if (status.mode === 'astraea_relay' && status.relay_connected) {
    return copy.bridgeRelayOnline
  }
  if (status.mode === 'disconnected') {
    return copy.bridgeDisconnected
  }
  return copy.bridgeHubStandby
}

export const DEFAULT_SEED_AGENTS: readonly AppAgentConfig[] = [
  {
    slug: 'macro-argus',
    display_name: 'Chuyên gia Vĩ mô Argus',
    description: 'Theo dõi lãi suất, tỷ giá, chuỗi số liệu vĩ mô và dòng vốn liên thị trường.',
    emoji: '📈',
    color: '#236466',
    avatar_url: null,
    reply_language: 'vi',
    soul_md:
      'Bạn là Argus — chuyên gia phân tích vĩ mô điềm tĩnh, luôn đối chiếu tin tức với chuỗi số liệu kinh tế và dẫn nguồn bài báo cụ thể (#article-ID).',
    app_role_addendum_md:
      'Chỉ kết luận dựa trên dữ liệu đã lưu trong Newsman thuộc vùng và chủ đề được phép; không tự suy diễn số liệu ngoài nguồn.',
    sync_blueprint_to_hub: true,
    runtime_kind: 'partner_fast',
    primary_model: 'gpt-5.4-mini',
    backup_model: 'claude-3-7-sonnet',
    reasoning_effort: 'medium',
    remote_endpoint_cfg: {},
    max_tool_steps: 8,
    enabled_app_tools: [
      'latest_articles',
      'article_get',
      'series',
      'observations',
      'analyses',
      'notes',
      'brief_propose',
    ],
    enabled_mcp_tools: ['cortex__knowledge_search'],
    enabled_skills: ['macro-cross-impact-brief'],
    enabled_skill_groups: ['newsroom-core-pack'],
    default_resource_ctx: { regions: ['global', 'vn', 'us'], topics: ['macro', 'markets', 'rates'] },
    memory_mode: 'app_isolated',
  },
  {
    slug: 'tax-linh',
    display_name: 'Cố vấn Thuế & Pháp quy Linh',
    description: 'Rà soát lịch thuế khai báo, văn bản pháp quy chính thức và lưu ý tuân thủ theo vùng.',
    emoji: '⚖️',
    color: '#ba7044',
    avatar_url: null,
    reply_language: 'vi',
    soul_md:
      'Bạn là Linh — chuyên viên rà soát lịch thuế và văn bản pháp quy cẩn trọng, luôn phân biệt rõ quy tắc đã xác minh (verified = true) và chưa xác minh (verified = false).',
    app_role_addendum_md:
      'Luôn giữ nguyên cảnh báo miễn trừ tư vấn pháp lý và thuế của máy chủ khi trích dẫn lịch thuế hoặc văn bản pháp quy.',
    sync_blueprint_to_hub: true,
    runtime_kind: 'partner_fast',
    primary_model: 'claude-3-7-sonnet',
    backup_model: 'gpt-5.4-mini',
    reasoning_effort: 'high',
    remote_endpoint_cfg: {},
    max_tool_steps: 8,
    enabled_app_tools: ['latest_articles', 'article_get', 'documents', 'tax_calendar', 'notes'],
    enabled_mcp_tools: ['cortex__knowledge_search'],
    enabled_skills: ['statutory-tax-citation-guard'],
    enabled_skill_groups: ['newsroom-core-pack', 'compliance-pack'],
    default_resource_ctx: { regions: ['vn', 'global'], topics: ['tax', 'legal', 'policy'] },
    memory_mode: 'app_isolated',
  },
  {
    slug: 'editor-minh',
    display_name: 'Biên tập viên Minh',
    description: 'Chủ trì bàn tròn tòa soạn, tổng hợp đa góc nhìn và biên tập bản thảo Báo cáo đồng bộ Obsidian.',
    emoji: '🗞️',
    color: '#4a5470',
    avatar_url: null,
    reply_language: 'auto',
    soul_md:
      'Bạn là Minh — biên tập viên trưởng sắc sảo, giúp kết nối góc nhìn vĩ mô và pháp quy thành bản tin cô đọng, giàu dẫn chứng.',
    app_role_addendum_md:
      'Ưu tiên sử dụng ngữ cảnh bài báo hoặc báo cáo đang mở; khi đề xuất xuất bản Báo cáo mới phải gửi qua quy trình chờ biên tập viên duyệt.',
    sync_blueprint_to_hub: true,
    runtime_kind: 'partner_fast',
    primary_model: 'gpt-5.4-mini',
    backup_model: 'gemini-2.5-pro',
    reasoning_effort: 'medium',
    remote_endpoint_cfg: {},
    max_tool_steps: 8,
    enabled_app_tools: [
      'latest_articles',
      'article_get',
      'documents',
      'tax_calendar',
      'analyses',
      'notes',
      'brief_propose',
    ],
    enabled_mcp_tools: ['obsidian__vault_notes'],
    enabled_skills: ['editorial-factcheck-digest'],
    enabled_skill_groups: ['newsroom-core-pack'],
    default_resource_ctx: { regions: ['global', 'vn'] },
    memory_mode: 'app_isolated',
  },
] as const
