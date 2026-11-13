// Canonical TypeScript types & bilingual copy for App #2 (`lnc-newsman`):
// Astraea App-Scoped Conversational Agents, 4-Mode Editorial Group Chat,
// DeepTutor 6-Tab Studio, Article/Report Companion Drawer & Dual-Mode Telegram Bridge.

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

export interface SkillAccessRequest {
  id: string
  target_kind: 'skill' | 'skill_group' | string
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
    label_vi: 'Tìm & Liệt kê bài báo (FTS5)',
    label_en: 'Search & list articles (FTS5)',
    desc_vi: 'Truy vấn tin mới và tìm kiếm toàn văn FTS5 theo vùng/chủ đề được cấp quyền v4s.',
    desc_en: 'Query latest headlines and FTS5 full-text search within granted v4s regions and topics.',
  },
  {
    name: 'article_get',
    tier: 'read',
    scope: 'newsman:read',
    label_vi: 'Đọc toàn văn bài báo',
    label_en: 'Read article full text',
    desc_vi: 'Đọc nội dung sạch, tóm tắt RSS và thông tin ghi nguồn của một bài báo.',
    desc_en: 'Inspect extracted readable prose, RSS excerpt, and source attribution for an article.',
  },
  {
    name: 'documents',
    tier: 'read',
    scope: 'newsman:read',
    label_vi: 'Văn bản chính thức & Pháp quy',
    label_en: 'Official & regulatory documents',
    desc_vi: 'Tra cứu văn bản pháp quy, công보 kèm miễn trừ trách nhiệm pháp lý của hệ thống.',
    desc_en: 'Search official filings and regulatory documents with server-attached legal disclaimers.',
  },
  {
    name: 'tax_calendar',
    tier: 'read',
    scope: 'newsman:read',
    label_vi: 'Lịch thuế khai báo (Kèm miễn trừ pháp lý)',
    label_en: 'Declared tax calendar (With legal disclaimer)',
    desc_vi: 'Xem hạn nộp thuế theo region pack; giữ nguyên cờ verified và miễn trừ tư vấn thuế/pháp lý.',
    desc_en: 'Query declared tax deadlines; strictly preserves verified flags and tax/legal disclaimers.',
  },
  {
    name: 'series',
    tier: 'read',
    scope: 'newsman:read',
    label_vi: 'Danh mục chuỗi số liệu vĩ mô',
    label_en: 'Macroeconomic time series catalog',
    desc_vi: 'Liệt kê các chuỗi chỉ số kinh tế, lãi suất, tỷ giá theo vùng.',
    desc_en: 'List macroeconomic indicators, interest rates, and FX series by region.',
  },
  {
    name: 'observations',
    tier: 'read',
    scope: 'newsman:read',
    label_vi: 'Điểm dữ liệu chuỗi thời gian',
    label_en: 'Series observations & revisions',
    desc_vi: 'Đọc các điểm số liệu lịch sử và phiên bản hiệu chỉnh (revision) của chuỗi.',
    desc_en: 'Read historical observation points and vintage revisions for a macro series.',
  },
  {
    name: 'analyses',
    tier: 'read',
    scope: 'newsman:read',
    label_vi: 'Đọc Báo cáo & Brief đã lưu',
    label_en: 'Read stored briefs & analyses',
    desc_vi: 'Tra cứu các bản tin phân tích đã xuất bản cùng danh sách bài nguồn trích dẫn.',
    desc_en: 'Inspect stored analytical briefs and their cited source article IDs.',
  },
  {
    name: 'notes',
    tier: 'write_safe',
    scope: 'newsman:write',
    label_vi: 'Ghi chú nghiên cứu (Đồng bộ 2 chiều)',
    label_en: 'Research notes (Two-way sync)',
    desc_vi: 'Đọc hoặc thêm ghi chú nghiên cứu gắn nhãn trợ lý trên bài báo/báo cáo.',
    desc_en: 'Read or append agent-attributed research notes on articles and briefs.',
  },
  {
    name: 'brief_propose',
    tier: 'write_gated',
    scope: 'newsman:write',
    label_vi: 'Đề xuất xuất bản Báo cáo (Cần duyệt)',
    label_en: 'Propose brief publication (Gated)',
    desc_vi: 'Tạo đề xuất xuất bản Brief mới (approval_required: newsman.brief.publish) chờ chủ dự án duyệt.',
    desc_en: 'Queue a gated brief publication proposal (newsman.brief.publish) requiring owner approval.',
  },
] as const

export const MCP_BRIDGE_TOOLS: readonly { name: string; label_vi: string; label_en: string }[] = [
  { name: 'cortex__memory_search', label_vi: 'Tìm bộ nhớ Cortex', label_en: 'Cortex memory search' },
  { name: 'cortex__knowledge_search', label_vi: 'Tra cứu tri thức Cortex', label_en: 'Cortex knowledge search' },
  { name: 'obsidian__vault_notes', label_vi: 'Đồng bộ ghi chú Obsidian (_agents/)', label_en: 'Obsidian vault sync (_agents/)' },
] as const

export const AGENT_COPY = {
  vi: {
    agents: 'Bàn Trợ lý',
    navAgents: 'Bàn Trợ lý',
    agentsEyebrow: 'Tòa soạn Phân tích & Bàn tròn Trợ lý',
    agentsWorkspaceTitle: 'Bàn Trợ lý & Hội thảo Tin tức',
    agentsWorkspaceSub: 'Đối thoại 1:1 hoặc mở bàn tròn nhiều trợ lý phân tích vĩ mô, lịch thuế và văn bản pháp quy — cách ly dữ liệu v4s theo vùng & chủ đề.',
    agentsTabWorkspace: 'Phòng họp Tòa soạn',
    agentsTabStudio: 'Agent Studio (DeepTutor)',
    bridgeRelayOnline: 'Astraea Relay · Trực tuyến',
    bridgeHubStandby: 'Hub Standby · Tự chủ tại chỗ',
    bridgeDisconnected: 'Chưa kết nối Relay',
    bridgeConfigureBtn: 'Cấu hình Cầu nối',
    bridgeAstraeaUrl: 'Địa chỉ Astraea API',
    bridgeHubUrl: 'Địa chỉ LNC Agent Hub',
    bridgeDefaultModel: 'Model mặc định',
    bridgeAstraeaToken: 'Khóa cầu nối Astraea (Chỉ ghi)',
    bridgeHubToken: 'Khóa LNC Hub (Chỉ ghi)',
    bridgeTokenSaved: 'Đã mã hóa qua SecretBox trên máy chủ (không bao giờ hiển thị lại)',
    directThreadsTitle: 'Trao đổi 1:1',
    groupRoomsTitle: 'Bàn tròn Phân tích',
    newDirectBtn: '+ 1:1 mới',
    newGroupBtn: '+ Bàn tròn',
    threadTitleInput: 'Tiêu đề phiên phân tích…',
    pickMembersLabel: 'Chọn trợ lý tham gia',
    pickLeadLabel: 'Biên tập viên chủ trì (Lead)',
    discussionModeLabel: 'Chế độ thảo luận bàn tròn',
    modeParallelPanel: 'Hội đồng song song (Parallel Panel)',
    modeParallelPanelDesc: 'Các trợ lý độc lập phân tích cùng lúc từ một tập dữ liệu vĩ mô/tin tức.',
    modeSequentialBuild: 'Tiếp sức bồi đắp (Sequential Build)',
    modeSequentialBuildDesc: 'Trợ lý sau đọc phân tích của trợ lý trước để bổ sung góc nhìn pháp lý/số liệu.',
    modeCrossDebate: 'Tranh biện 2 vòng (Cross Debate)',
    modeCrossDebateDesc: 'Vòng 1 nêu nhận định độc lập → Vòng 2 phản biện chéo các giả định rủi ro.',
    modeMentionOrLead: 'Gọi tên @slug hoặc Chủ trì (Mention / Lead)',
    modeMentionOrLeadDesc: 'Định tuyến tới @slug được nhắc hoặc biên tập viên chủ trì (tối đa 3 bước hỏi đồng nghiệp).',
    createRoomAction: 'Mở phiên thảo luận',
    noThreadsYet: 'Chưa có phiên thảo luận nào. Hãy tạo luồng 1:1 hoặc mở bàn tròn phân tích.',
    selectThreadPrompt: 'Chọn một phiên thảo luận ở cột trái hoặc mở bàn tròn mới cùng các trợ lý tòa soạn.',
    composerPlaceholder: 'Đặt câu hỏi phân tích, gõ @slug để gọi trợ lý… (Enter để gửi, Shift+Enter xuống dòng)',
    sendTurnBtn: 'Gửi',
    sendingTurnBtn: 'Đang phân tích…',
    mentionAutocompleteHint: 'Gợi ý gọi nhanh trợ lý:',
    reasoningTraceSummary: 'Mạch suy luận & kiểm chứng nguồn',
    toolCallRunning: 'Đang truy vấn dữ liệu',
    toolCallDone: 'Đã truy vấn dữ liệu',
    toolCallError: 'Lỗi công cụ',
    toolApprovalRequired: 'Cần phê duyệt (write_gated)',
    v4sScopedBadge: 'v4s cách ly vùng/chủ đề',
    seatThinking: 'đang phân tích…',
    seatSpeaking: 'đang soạn…',
    seatToolCalling: 'đang tra cứu tin/số liệu…',
    seatDone: 'hoàn tất',
    seatError: 'gián đoạn',
    whiteboardTitle: 'Bảng ghim Kết luận',
    whiteboardSub: 'Các luận điểm ghim tại đây sẽ tự động đưa vào ngữ cảnh của vòng thảo luận kế tiếp.',
    whiteboardPlaceholder: 'Ghi chú nhận định vĩ mô, mốc lịch thuế hoặc kết luận để ghim…',
    whiteboardAddBtn: 'Ghim kết luận',
    whiteboardEmpty: 'Chưa có kết luận nào được ghim trong phiên này.',
    whiteboardSummarizeBtn: 'Tóm tắt vòng này',
    whiteboardSummarizing: 'Đang tóm tắt…',
    whiteboardPinnedBadge: 'Đã ghim',
    whiteboardToggleDrawer: 'Bảng ghim',
    studioHeaderTitle: 'DeepTutor Agent Studio',
    studioHeaderSub: 'Thiết kế nhân cách trợ lý tòa soạn, mô hình LNC Hub, phạm vi vùng/chủ đề v4s, lịch trình điểm tin và cầu nối Telegram.',
    studioNewAgentBtn: '+ Tạo trợ lý mới',
    studioSaveAgentBtn: 'Lưu cấu hình trợ lý',
    studioDeleteAgentBtn: 'Xoá trợ lý',
    studioTabIdentity: '1. Định danh & Giọng',
    studioTabPersona: '2. Tâm thế & Ngữ cảnh',
    studioTabRuntime: '3. Mô hình & Runtime',
    studioTabScope: '4. Phạm vi & Công cụ',
    studioTabSchedules: '5. Lịch trình & Cron',
    studioTabTelegram: '6. Cầu nối Telegram',
    studioSlugLabel: 'Mã định danh (slug)',
    studioDisplayNameLabel: 'Tên hiển thị',
    studioDescriptionLabel: 'Mô tả chuyên môn',
    studioEmojiLabel: 'Biểu tượng (Emoji)',
    studioColorLabel: 'Màu sắc nhận diện',
    studioReplyLangLabel: 'Ngôn ngữ phản hồi',
    studioLangVi: 'Tiếng Việt (Mặc định)',
    studioLangEn: 'Tiếng Anh (English)',
    studioLangAuto: 'Tự động theo người đọc',
    studioSoulMdLabel: 'Bản sắc cốt lõi (SOUL.md)',
    studioSoulMdHint: 'Định hình nguyên tắc kiểm chứng nguồn, trích dẫn bài báo và văn phong phân tích.',
    studioRoleAddendumLabel: 'Phụ lục vai trò Tòa soạn (app_role_addendum_md)',
    studioRolePreviewLabel: 'Bản xem trước tổng hợp Prompt',
    studioSyncHubLabel: 'Đồng bộ Blueprint lên LNC Agent Hub',
    studioRuntimeKindLabel: 'Bộ máy thực thi (Runtime Kind)',
    studioPrimaryModelLabel: 'Mô hình chính (LNC Hub Primary Model)',
    studioBackupModelLabel: 'Mô hình dự phòng (Backup Model)',
    studioReasoningEffortLabel: 'Mức độ suy luận (Reasoning Effort)',
    studioMaxToolStepsLabel: 'Số bước gọi công cụ tối đa (Max Tool Steps)',
    studioRemoteUrlLabel: 'Địa chỉ Remote Agent Endpoint',
    studioSecretRefLabel: 'Tham chiếu khóa Hub Vault (Chỉ nhận vault://…)',
    studioSecretRefHint: 'Tuyệt đối không nhập khóa thô tại đây. Chỉ dùng đường dẫn vault:// trên LNC-Agent-Hub.',
    studioSecretRefInvalid: 'Tham chiếu bí mật phải bắt đầu bằng vault:// (không nhập khóa thô).',
    studioAppToolsTitle: 'Công cụ Tòa soạn nội bộ (Newsroom Domain Tools)',
    studioMcpToolsTitle: 'Công cụ MCP mở rộng (Liên thông Hub)',
    studioResourceCtxTitle: 'Giới hạn phạm vi dữ liệu v4s (Row-Level Region & Topic Isolation)',
    studioAllowedRegionsLabel: 'Mã vùng được phép truy cập (cách nhau bởi dấu phẩy, vd: vn, us, global — để trống = mọi vùng đang bật)',
    studioAllowedTopicsLabel: 'Mã chủ đề được phép truy cập (cách nhau bởi dấu phẩy, vd: macro, tax, tech — để trống = mọi chủ đề)',
    studioTaxDisclaimerNotice: 'Quy tắc bắt buộc: Công cụ tax_calendar & documents luôn giữ nguyên cờ verified=false (nếu chưa xác minh với cơ quan thuế) và đính kèm miễn trừ trách nhiệm pháp lý/thuế. Trợ lý không bao giờ trình bày hạn nộp tự khai báo như tư vấn pháp lý chính thức.',
    studioMemoryModeLabel: 'Chế độ bộ nhớ trợ lý',
    studioMemoryIsolated: 'Cách ly trong ứng dụng (app_isolated — Khuyên dùng)',
    studioMemoryShared: 'Chia sẻ liên ứng dụng qua Cortex (cross_app_shared)',
    studioSchedulesTitle: 'Lịch trình tự động (Điểm tin sáng / Rà soát lịch thuế)',
    studioScheduleNameLabel: 'Tên lịch trình',
    studioScheduleCronLabel: 'Biểu thức Cron hoặc chu kỳ (giây)',
    studioSchedulePromptLabel: 'Yêu cầu giao cho trợ lý',
    studioScheduleRunNow: 'Chạy ngay',
    studioScheduleRunning: 'Đang chạy…',
    studioScheduleLastStatus: 'Trạng thái gần nhất',
    studioTelegramModeTitle: 'Chế độ kết nối Telegram 2 chiều (Conversational Bridge)',
    studioTelegramCoexistenceNotice: 'Lưu ý vận hành song song: Kênh cảnh báo tự động một chiều (alert_telegram_outbox tại Cài đặt → Kênh Telegram) vẫn giữ nguyên độc lập. Cầu nối dưới đây dành riêng cho hội thoại 2 chiều với trợ lý theo từng Topic.',
    studioTelegramDirectMode: 'Direct Bot Token (Poll / Webhook)',
    studioTelegramCygnusMode: 'Cygnus Gateway v0.8',
    studioTelegramBotName: 'Tên Bot (@bot_username)',
    studioTelegramBotToken: 'Telegram Bot Token (Chỉ ghi — không bao giờ hiển thị lại)',
    studioTelegramWebhookSecret: 'Webhook Secret Token (Chỉ ghi)',
    studioTelegramCygnusUrl: 'Địa chỉ Cygnus Gateway',
    studioTelegramCygnusKey: 'Khóa x-gateway-key Cygnus (Chỉ ghi — không bao giờ hiển thị lại)',
    studioTelegramPolicyLabel: 'Chính sách phản hồi trong nhóm (TelegramChatPolicy)',
    studioTelegramPolicyOpen: 'Mở (open — trả lời mọi tin nhắn)',
    studioTelegramPolicyMention: 'Chỉ khi được @nhắc tên (mention)',
    studioTelegramPolicyTopics: 'Chỉ trong đúng Topic đã gắn (topics_only)',
    studioTelegramRoutesTitle: 'Ánh xạ (chat_id, message_thread_id) ↔ (agent_slug, thread_id)',
    studioTelegramChatId: 'Chat ID nhóm Telegram',
    studioTelegramTopicId: 'Topic ID (message_thread_id)',
    studioTelegramAddRoute: 'Thêm ánh xạ Topic',
    companionToggleBtn: 'Hỏi trợ lý về bài này',
    companionToggleReportBtn: 'Hỏi trợ lý về báo cáo này',
    companionHeaderTitle: 'Trợ lý Đọc & Phân tích tin',
    companionContextBadge: 'Zero-RTT Ngữ cảnh bài đang đọc',
    companionSelectedTextLabel: 'Đoạn đang chọn:',
    companionArticleExcerptLabel: 'Trích đoạn nội dung bài báo:',
    companionQuickExplain: 'Phân tích tác động & bối cảnh',
    companionQuickSummarize: 'Tóm tắt 3 ý cốt lõi',
    companionQuickVerifyTax: 'Kiểm tra lưu ý pháp lý / lịch thuế liên quan',
    companionJumpArticle: 'Mở bài báo',
    surfaceSalon: '💬 Thảo luận',
    surfaceAgents: '👥 Trợ lý & Studio',
    surfaceWorkspaces: '🗂 Không gian',
    surfaceSkills: '🧩 Kho Kỹ năng',
    surfaceModels: '🔌 Mô hình & Kết nối',
    workspaceAll: 'Tất cả không gian',
    workspaceActiveLabel: 'Không gian hiện hành',
    workspaceNewBtn: '+ Không gian mới',
    workspaceSaveBtn: 'Lưu Không gian',
    workspaceDeleteBtn: 'Xoá Không gian',
    workspaceNameLabel: 'Tên không gian biên tập',
    workspaceDescLabel: 'Mô tả vai trò & phạm vi',
    workspaceEmojiLabel: 'Biểu tượng',
    workspaceInstructionsLabel: 'Chỉ dẫn chuyên biệt (custom_instructions_md)',
    workspaceDefaultSkillsLabel: 'Kỹ năng mặc định (Granted Skills)',
    workspaceDefaultAgentsLabel: 'Trợ lý tham gia mặc định',
    workspaceDefaultModeLabel: 'Chế độ thảo luận mặc định',
    workspaceRegionsLabel: 'Giới hạn vùng dữ liệu (regions, cách nhau dấu phẩy)',
    workspaceTopicsLabel: 'Giới hạn chủ đề (topics, cách nhau dấu phẩy)',
    workspaceIsDefaultLabel: 'Đặt làm Không gian mặc định khi mở Bàn Trợ lý',
    workspaceTaxReminder: 'Nhắc nhở tuân thủ: giao điểm vùng/chủ đề giữa Không gian và Trợ lý là fail-closed; trích dẫn lịch thuế luôn bảo toàn cờ verified và miễn trừ trách nhiệm.',
    studioTabSkills: '7. Kỹ năng & Gói Hub',
    studioAssignedWorkspacesLabel: 'Không gian có gắn trợ lý này',
    studioSkillGroupsLabel: 'Nhóm gói Kỹ năng Hub (enabled_skill_groups)',
    studioGrantedSkillsLabel: 'Kỹ năng lẻ được bật (enabled_skills)',
    traceSummaryLabel: '🧠 Suy luận',
    traceToolsUnit: 'công cụ',
    panelTabTrace: 'Nhật ký Suy luận',
    panelTabContext: 'Bảng ghim & Ngữ cảnh',
    panelNoTraceYet: 'Chọn nhãn "🧠 Suy luận" trên một lượt trả lời để xem suy luận và chi tiết gọi công cụ.',
    composerModelLabel: 'Mô hình lượt này',
    composerModelDefault: 'Mặc định trợ lý / không gian',
    composerBudgetLabel: 'Ngữ cảnh bài đang đọc',
    composerSkillAttachHint: 'Gõ @ để gọi trợ lý, / để gắn kỹ năng được cấp quyền',
    actionCopy: 'Sao chép',
    actionQuote: 'Trích dẫn',
    actionAskPeer: 'Hỏi tiếp @',
    starterMacroToday: 'Điểm tin vĩ mô hôm nay',
    starterTaxReview: 'Rà soát hạn thuế chưa verified',
    starterPolicyWatch: 'Tổng hợp văn bản pháp quy nổi bật tuần này',
    skillsSyncBtn: 'Đồng bộ từ Hub',
    skillsSyncing: 'Đang đồng bộ…',
    skillsAllGroups: 'Tất cả nhóm',
    skillsSearchPlaceholder: 'Tìm kỹ năng hoặc gói chuyên đề…',
    skillsEmpty: 'Không có kỹ năng nào phù hợp bộ lọc.',
    skillsGrantedBadge: 'Đã cấp quyền',
    skillsLockedBadge: 'Đã khóa · Chưa cấp Hub',
    skillsEnableToggle: 'Đang bật',
    skillsDisableToggle: 'Đang tắt cục bộ',
    skillsRequestAccessBtn: 'Xin cấp quyền Skill / Nhóm Skill',
    skillsRequestModalTitle: 'Gửi yêu cầu cấp quyền từ LNC Agent Hub',
    skillsRequestKindLabel: 'Loại đối tượng',
    skillsRequestKindSkill: 'Kỹ năng lẻ (skill)',
    skillsRequestKindGroup: 'Nhóm gói kỹ năng (skill_group)',
    skillsRequestTargetLabel: 'Mã định danh (slug)',
    skillsRequestReasonLabel: 'Lý do biên tập cần sử dụng',
    skillsRequestSubmitBtn: 'Gửi yêu cầu cấp quyền',
    skillsRequestsHistoryTitle: 'Lịch sử yêu cầu cấp quyền',
    skillsNoRequests: 'Chưa có yêu cầu cấp quyền nào.',
    skillsPlaybookPlaceholder: 'Chọn một kỹ năng để xem hướng dẫn Playbook chi tiết.',
    modelsProbeBtn: 'Kiểm tra kết nối',
    modelsProbing: 'Đang kiểm tra…',
    modelsProbeLastRun: 'Kiểm tra gần nhất',
    modelsServiceHub: 'LNC Agent Hub',
    modelsServiceAstraea: 'Astraea Reverse Relay',
    modelsServiceLlm: 'Điểm cuối LLM (/v1/models)',
    modelsConnectedOk: 'Sẵn sàng',
    modelsDisconnected: 'Không kết nối được',
    modelsNotConfigured: 'Chưa cấu hình',
    modelsCatalogTitle: 'Danh mục Mô hình khả dụng',
    taskModelsTitle: 'Định tuyến Mô hình theo Tác vụ (Task Models)',
    taskModelsSaveBtn: 'Lưu cấu hình Mô hình & Kết nối',
    taskModelThreadTitle: 'Đặt tiêu đề luồng (thread_title)',
    taskModelRoundSummary: 'Tóm tắt vòng thảo luận (round_summary)',
    taskModelCompanionBrief: 'Phân tích nhanh bài đang đọc (companion_brief)',
    taskModelTaxDocDigest: 'Đối chiếu văn bản & lịch thuế (tax_doc_digest)',
    bridgeLlmBaseUrl: 'LLM Base URL (OpenAI-compatible)',
    bridgeLlmApiKey: 'LLM API Key (Chỉ ghi — không bao giờ hiển thị lại)',
    bridgeAllowedHosts: 'Allowed Private Hosts (phân tách bằng dấu phẩy)',
    companionSkillChipLabel: 'Kỹ năng nhanh (/skill)',
    companionWorkspaceBadge: 'Không gian:',
    skillsGroupPacksTitle: 'Gói kỹ năng theo nhóm từ Hub',
    skillsGroupRequestBtn: 'Xin quyền cả nhóm',
    skillsGroupGrantedBadge: '✓ Đã cấp nhóm',
    skillsAttachToSalonBtn: 'Gắn vào Bàn tròn',
    skillsLockedPreviewTitle: 'Xem trước an toàn (Kỹ năng đang khóa)',
    skillsLockedPreviewEmpty:
      'Chưa có bản xem trước cho kỹ năng này. Bấm Đồng bộ lại từ Hub hoặc gửi yêu cầu cấp quyền để mở toàn văn SKILL.md.',
    skillsRequestSkillBtn: 'Xin quyền kỹ năng',
    skillsRequestByGroupBtn: 'Xin theo nhóm',
    skillsInspectTaxonomyBtn: 'Xem phân loại Vùng & Chủ đề',
    skillsAttachedNotice: 'Đã gắn kỹ năng vào Bàn tròn hiện tại.',
    skillsReqStatusApproved: 'Đã duyệt',
    skillsReqStatusPending: 'Đang chờ duyệt',
    skillsReqStatusRejected: 'Từ chối',
    skillsReqStatusLocal: 'Lưu cục bộ',
    skillsReqReviewedBy: 'Người duyệt',
    skillsReqApprovalId: 'Mã duyệt',
    salonScopeWorkspaceLabel: 'Không gian',
    salonScopeEffectiveLabel: 'Phạm vi hiệu lực',
    salonScopeRegionsAll: 'Mọi vùng',
    salonScopeTopicsAll: 'Mọi chủ đề',
    salonScopePinnedSkills: 'Kỹ năng ghim',
    salonScopeNoWorkspace: 'Toàn tòa soạn (chưa giới hạn)',
    salonScopeDisjoint: 'Rỗng (không giao nhau)',
    workspaceLaunchSalonBtn: 'Mở Bàn tròn với Không gian này',
    connFleetPresetBtn: '⚡ Điền nhanh chuẩn LNC Fleet',
    connFleetPresetApplied: 'Đã điền URL chuẩn LNC Fleet & danh sách host nội bộ an toàn — dán khóa mới nếu cần rồi bấm Lưu.',
    connDiagTitle: 'Chẩn đoán Runtime & Tầng Cấu hình',
    connDiagSub: 'Cấu hình UI runtime (mã hóa LNC_MASTER_KEY) ghi đè biến môi trường khởi tạo; mọi kết nối đi qua bộ kiểm duyệt SSRF fetch::Fetcher.',
    settingsAgentsCardTitle: 'Đa Trợ lý Tòa soạn, Model theo Tác vụ & Cầu nối Hub/Astraea',
    settingsAgentsCardSub: 'Cấu hình điểm cuối LNC Agent Hub, Astraea Relay, model mặc định & phụ trợ theo tác vụ, gói kỹ năng.',
    settingsAgentsCardBtn: 'Mở Cấu hình Model & Kết nối',
  },
  en: {
    agents: 'Agent Desk',
    navAgents: 'Agent Desk',
    agentsEyebrow: 'Editorial Newsroom & Analyst Roundtables',
    agentsWorkspaceTitle: 'Agent Desk & Group Briefings',
    agentsWorkspaceSub: '1:1 newsroom analysts and multi-agent macro/tax/policy panels grounded in stored articles — v4s scoped by region and topic.',
    agentsTabWorkspace: 'Editorial Salon',
    agentsTabStudio: 'Agent Studio (DeepTutor)',
    bridgeRelayOnline: 'Astraea Relay · Connected',
    bridgeHubStandby: 'Hub Standby · Local Engine',
    bridgeDisconnected: 'Relay Disconnected',
    bridgeConfigureBtn: 'Bridge Settings',
    bridgeAstraeaUrl: 'Astraea API URL',
    bridgeHubUrl: 'LNC Agent Hub URL',
    bridgeDefaultModel: 'Default LLM Model',
    bridgeAstraeaToken: 'Astraea Bridge Token (Write-only)',
    bridgeHubToken: 'LNC Hub Token (Write-only)',
    bridgeTokenSaved: 'Sealed via SecretBox on server (never echoed back)',
    directThreadsTitle: 'Direct 1:1 Briefings',
    groupRoomsTitle: 'Group Analyst Rooms',
    newDirectBtn: '+ New 1:1',
    newGroupBtn: '+ Group Room',
    threadTitleInput: 'Briefing or discussion title…',
    pickMembersLabel: 'Select participating analysts',
    pickLeadLabel: 'Lead managing editor',
    discussionModeLabel: 'Roundtable discussion mode',
    modeParallelPanel: 'Parallel Panel',
    modeParallelPanelDesc: 'Concurrent independent perspectives from the same news and macro snapshot.',
    modeSequentialBuild: 'Sequential Build',
    modeSequentialBuildDesc: 'Ordered responses where each analyst builds on prior macro/legal points without repeating.',
    modeCrossDebate: 'Cross Debate (2 Rounds)',
    modeCrossDebateDesc: 'Round 1 independent theses followed by Round 2 cross-rebuttals of risk assumptions.',
    modeMentionOrLead: 'Mention or Lead (@slug)',
    modeMentionOrLeadDesc: 'Routes to @slug mentions or the lead editor (up to 3 peer consult hops).',
    createRoomAction: 'Start briefing',
    noThreadsYet: 'No discussions yet. Start a 1:1 thread or a multi-agent editorial room.',
    selectThreadPrompt: 'Select a briefing thread on the left or open a new editorial roundtable.',
    composerPlaceholder: 'Ask an analytical question, type @slug to mention an agent… (Enter sends, Shift+Enter newline)',
    sendTurnBtn: 'Send',
    sendingTurnBtn: 'Analysing…',
    mentionAutocompleteHint: 'Quick mention:',
    reasoningTraceSummary: 'Reasoning & source verification trace',
    toolCallRunning: 'Querying newsroom data',
    toolCallDone: 'Query completed',
    toolCallError: 'Tool error',
    toolApprovalRequired: 'Approval required (write_gated)',
    v4sScopedBadge: 'v4s region/topic-scoped',
    seatThinking: 'analysing…',
    seatSpeaking: 'drafting…',
    seatToolCalling: 'querying articles/series…',
    seatDone: 'done',
    seatError: 'error',
    whiteboardTitle: 'Pinned Whiteboard',
    whiteboardSub: 'Pinned takeaways are automatically injected into subsequent group discussion rounds.',
    whiteboardPlaceholder: 'Write a key macro takeaway, tax deadline note, or conclusion to pin…',
    whiteboardAddBtn: 'Pin takeaway',
    whiteboardEmpty: 'No pinned notes on this thread whiteboard yet.',
    whiteboardSummarizeBtn: 'Summarize Round',
    whiteboardSummarizing: 'Summarizing…',
    whiteboardPinnedBadge: 'Pinned',
    whiteboardToggleDrawer: 'Whiteboard',
    studioHeaderTitle: 'DeepTutor Agent Studio',
    studioHeaderSub: 'Configure newsroom identity, editorial voice, LNC Hub models, v4s region/topic scopes, cron schedules, and Telegram bridge.',
    studioNewAgentBtn: '+ New Agent',
    studioSaveAgentBtn: 'Save Agent Config',
    studioDeleteAgentBtn: 'Delete Agent',
    studioTabIdentity: '1. Identity & Voice',
    studioTabPersona: '2. Persona & Context',
    studioTabRuntime: '3. Model & Runtime',
    studioTabScope: '4. Newsroom Scope & Tools',
    studioTabSchedules: '5. Schedules & Cron',
    studioTabTelegram: '6. Telegram Bridge',
    studioSlugLabel: 'Agent slug',
    studioDisplayNameLabel: 'Display name',
    studioDescriptionLabel: 'Specialty description',
    studioEmojiLabel: 'Avatar emoji',
    studioColorLabel: 'Accent color',
    studioReplyLangLabel: 'Reply language',
    studioLangVi: 'Vietnamese (Default)',
    studioLangEn: 'English',
    studioLangAuto: 'Auto-detect from reader',
    studioSoulMdLabel: 'Core Persona (SOUL.md)',
    studioSoulMdHint: 'Defines source attribution rigor, analytical tone, and citation discipline.',
    studioRoleAddendumLabel: 'Newsroom Role Addendum (app_role_addendum_md)',
    studioRolePreviewLabel: 'Combined System Prompt Preview',
    studioSyncHubLabel: 'Sync Blueprint to LNC Agent Hub',
    studioRuntimeKindLabel: 'Execution Runtime (runtime_kind)',
    studioPrimaryModelLabel: 'Primary Model (LNC Hub Alias)',
    studioBackupModelLabel: 'Backup Model (Failover)',
    studioReasoningEffortLabel: 'Reasoning Effort',
    studioMaxToolStepsLabel: 'Max Tool Steps per Turn',
    studioRemoteUrlLabel: 'Remote Agent Endpoint URL',
    studioSecretRefLabel: 'Hub Vault Secret Reference (vault://… only)',
    studioSecretRefHint: 'Never enter raw API keys here. Use a vault:// reference managed by LNC-Agent-Hub.',
    studioSecretRefInvalid: 'Secret reference must start with vault:// (raw keys are prohibited).',
    studioAppToolsTitle: 'Newsroom Domain Tools (enabled_app_tools)',
    studioMcpToolsTitle: 'External MCP Tools (enabled_mcp_tools)',
    studioResourceCtxTitle: 'v4s Row-Level Scope Isolation (default_resource_ctx)',
    studioAllowedRegionsLabel: 'Allowed region codes (comma-separated, e.g. vn, us, global — blank = all enabled regions)',
    studioAllowedTopicsLabel: 'Allowed topic slugs (comma-separated, e.g. macro, tax, tech — blank = all enabled topics)',
    studioTaxDisclaimerNotice: 'Mandatory guardrail: `tax_calendar` and `documents` always preserve `verified = false` on unverified pack rules and attach the statutory legal/tax disclaimer. Agents must never present unverified deadlines as formal legal advice.',
    studioMemoryModeLabel: 'Agent Memory Isolation Mode',
    studioMemoryIsolated: 'App-isolated memory (app_isolated — Recommended)',
    studioMemoryShared: 'Cross-app shared via Cortex (cross_app_shared)',
    studioSchedulesTitle: 'Automated Schedules (Morning Briefs / Tax Calendar Sweeps)',
    studioScheduleNameLabel: 'Schedule name',
    studioScheduleCronLabel: 'Cron expression or interval (seconds)',
    studioSchedulePromptLabel: 'Scheduled prompt',
    studioScheduleRunNow: 'Run Now',
    studioScheduleRunning: 'Running…',
    studioScheduleLastStatus: 'Last run status',
    studioTelegramModeTitle: 'Dual-Mode Bidirectional Telegram Bridge',
    studioTelegramCoexistenceNotice: 'Coexistence guardrail: Newsman’s one-way urgent alert pipeline (`alert_telegram_outbox` in Settings → Telegram channel) remains completely separate. This bridge handles two-way conversational agent threads mapped to dedicated topics.',
    studioTelegramDirectMode: 'Direct Bot Token (Poll / Webhook)',
    studioTelegramCygnusMode: 'Cygnus Gateway v0.8',
    studioTelegramBotName: 'Bot username (@bot_name)',
    studioTelegramBotToken: 'Telegram Bot Token (Write-only — never echoed)',
    studioTelegramWebhookSecret: 'Webhook Secret Token (Write-only)',
    studioTelegramCygnusUrl: 'Cygnus Gateway URL',
    studioTelegramCygnusKey: 'Cygnus x-gateway-key (Write-only — never echoed)',
    studioTelegramPolicyLabel: 'Group Chat Policy (TelegramChatPolicy)',
    studioTelegramPolicyOpen: 'Open (reply to all messages in bound chat)',
    studioTelegramPolicyMention: 'Mention only (require @bot mention)',
    studioTelegramPolicyTopics: 'Topics only (strict forum topic binding)',
    studioTelegramRoutesTitle: 'Topic Routing Map: (chat_id, message_thread_id) ↔ (agent_slug, thread_id)',
    studioTelegramChatId: 'Telegram Group Chat ID',
    studioTelegramTopicId: 'Forum Topic ID (message_thread_id)',
    studioTelegramAddRoute: 'Add Topic Route',
    companionToggleBtn: 'Ask Analyst about this article',
    companionToggleReportBtn: 'Ask Analyst about this brief',
    companionHeaderTitle: 'Newsroom Companion',
    companionContextBadge: 'Zero-RTT Active Article Context',
    companionSelectedTextLabel: 'Selected passage:',
    companionArticleExcerptLabel: 'Stored article text / excerpt:',
    companionQuickExplain: 'Analyze macro & market impact',
    companionQuickSummarize: 'Summarize 3 core takeaways',
    companionQuickVerifyTax: 'Check related legal / tax deadlines',
    companionJumpArticle: 'Open article',
    surfaceSalon: '💬 Salon',
    surfaceAgents: '👥 Agents & Studio',
    surfaceWorkspaces: '🗂 Workspaces',
    surfaceSkills: '🧩 Skill Hub',
    surfaceModels: '🔌 Models & Links',
    workspaceAll: 'All Workspaces',
    workspaceActiveLabel: 'Active Workspace',
    workspaceNewBtn: '+ New Workspace',
    workspaceSaveBtn: 'Save Workspace',
    workspaceDeleteBtn: 'Delete Workspace',
    workspaceNameLabel: 'Workspace name',
    workspaceDescLabel: 'Role & scope description',
    workspaceEmojiLabel: 'Emoji',
    workspaceInstructionsLabel: 'Custom Playbook Instructions (custom_instructions_md)',
    workspaceDefaultSkillsLabel: 'Default Granted Skills',
    workspaceDefaultAgentsLabel: 'Default Participating Analysts',
    workspaceDefaultModeLabel: 'Default Discussion Mode',
    workspaceRegionsLabel: 'Allowed Regions (comma-separated)',
    workspaceTopicsLabel: 'Allowed Topics (comma-separated)',
    workspaceIsDefaultLabel: 'Use as default Workspace on Agent Desk load',
    workspaceTaxReminder: 'Compliance reminder: workspace × agent region/topic intersection is fail-closed; tax_calendar citations preserve verified flags & legal disclaimers.',
    studioTabSkills: '7. Hub Skills & Packs',
    studioAssignedWorkspacesLabel: 'Workspaces assigning this analyst',
    studioSkillGroupsLabel: 'Enabled Hub Skill Packs (enabled_skill_groups)',
    studioGrantedSkillsLabel: 'Individually Enabled Skills (enabled_skills)',
    traceSummaryLabel: '🧠 Reasoning',
    traceToolsUnit: 'tools',
    panelTabTrace: 'Reasoning Trace',
    panelTabContext: 'Whiteboard & Context',
    panelNoTraceYet: 'Click a "🧠 Reasoning" badge on any analyst reply to inspect its thinking and tool trace.',
    composerModelLabel: 'Turn model override',
    composerModelDefault: 'Agent / thread default',
    composerBudgetLabel: 'Active view budget',
    composerSkillAttachHint: 'Type @ to mention an analyst, / to attach a granted skill',
    actionCopy: 'Copy',
    actionQuote: 'Quote',
    actionAskPeer: 'Ask @',
    starterMacroToday: 'Macro brief for today',
    starterTaxReview: 'Audit unverified tax deadlines',
    starterPolicyWatch: 'Highlight key regulatory filings this week',
    skillsSyncBtn: 'Sync from Hub',
    skillsSyncing: 'Syncing…',
    skillsAllGroups: 'All Packs',
    skillsSearchPlaceholder: 'Search skills or Hub packs…',
    skillsEmpty: 'No skills match your current filter.',
    skillsGrantedBadge: 'Granted',
    skillsLockedBadge: 'Locked · Hub Grant Required',
    skillsEnableToggle: 'Enabled locally',
    skillsDisableToggle: 'Disabled locally',
    skillsRequestAccessBtn: 'Request Skill / Pack Access',
    skillsRequestModalTitle: 'Submit Skill Access Request to LNC Agent Hub',
    skillsRequestKindLabel: 'Target kind',
    skillsRequestKindSkill: 'Single Skill (skill)',
    skillsRequestKindGroup: 'Skill Pack (skill_group)',
    skillsRequestTargetLabel: 'Target slug',
    skillsRequestReasonLabel: 'Editorial justification',
    skillsRequestSubmitBtn: 'Submit Access Request',
    skillsRequestsHistoryTitle: 'Access Request History',
    skillsNoRequests: 'No skill access requests submitted yet.',
    skillsPlaybookPlaceholder: 'Select a skill to inspect its playbook instructions.',
    modelsProbeBtn: 'Probe Connections',
    modelsProbing: 'Probing…',
    modelsProbeLastRun: 'Last probed',
    modelsServiceHub: 'LNC Agent Hub',
    modelsServiceAstraea: 'Astraea Reverse Relay',
    modelsServiceLlm: 'LLM Endpoint (/v1/models)',
    modelsConnectedOk: 'Healthy',
    modelsDisconnected: 'Unreachable',
    modelsNotConfigured: 'Not configured',
    modelsCatalogTitle: 'Available Model Catalog',
    taskModelsTitle: 'Task-Specific Model Routing (Task Models)',
    taskModelsSaveBtn: 'Save Models & Bridge Settings',
    taskModelThreadTitle: 'Thread titling (thread_title)',
    taskModelRoundSummary: 'Round synthesis (round_summary)',
    taskModelCompanionBrief: 'Reader companion brief (companion_brief)',
    taskModelTaxDocDigest: 'Tax & filing digest (tax_doc_digest)',
    bridgeLlmBaseUrl: 'LLM Base URL (OpenAI-compatible)',
    bridgeLlmApiKey: 'LLM API Key (Write-only — never echoed)',
    bridgeAllowedHosts: 'Allowed Private Hosts (comma-separated)',
    companionSkillChipLabel: 'Quick Skill (/skill)',
    companionWorkspaceBadge: 'Workspace:',
    skillsGroupPacksTitle: 'Hub Skill Group Packs',
    skillsGroupRequestBtn: 'Request Group Pack',
    skillsGroupGrantedBadge: '✓ Group Granted',
    skillsAttachToSalonBtn: 'Attach to active chat',
    skillsLockedPreviewTitle: 'Safe Preview (Locked Skill)',
    skillsLockedPreviewEmpty:
      'No safe preview cached yet. Sync with Hub or request access to unlock the full SKILL.md playbook.',
    skillsRequestSkillBtn: 'Request Skill Access',
    skillsRequestByGroupBtn: 'Request via Group Pack',
    skillsInspectTaxonomyBtn: 'Inspect Regions & Topics Scope',
    skillsAttachedNotice: 'Attached skill to active Salon chat.',
    skillsReqStatusApproved: 'Approved',
    skillsReqStatusPending: 'Pending review',
    skillsReqStatusRejected: 'Rejected',
    skillsReqStatusLocal: 'Saved locally',
    skillsReqReviewedBy: 'Reviewed by',
    skillsReqApprovalId: 'Approval ID',
    salonScopeWorkspaceLabel: 'Workspace',
    salonScopeEffectiveLabel: 'Effective Scope',
    salonScopeRegionsAll: 'All regions',
    salonScopeTopicsAll: 'All topics',
    salonScopePinnedSkills: 'Pinned /skills',
    salonScopeNoWorkspace: 'Newsroom-wide (unscoped)',
    salonScopeDisjoint: 'Empty (disjoint)',
    workspaceLaunchSalonBtn: 'Launch Salon with Workspace',
    connFleetPresetBtn: '⚡ Fill LNC Fleet Defaults',
    connFleetPresetApplied: 'Applied LNC Fleet URLs & LAN allowlist hosts — paste tokens if needed, then click Save.',
    connDiagTitle: 'Runtime & Config Layering Diagnostics',
    connDiagSub: 'Encrypted UI runtime config (LNC_MASTER_KEY) overrides seed env vars; all outbound calls pass through SSRF-safe fetch::Fetcher.',
    settingsAgentsCardTitle: 'Multi-Agent Salon, Task Models & Hub/Astraea Connections',
    settingsAgentsCardSub: 'Configure LNC Agent Hub, Astraea Relay, default & task-specific models, and skill group packs.',
    settingsAgentsCardBtn: 'Open Model & Connection Settings',
  },
} as const

export type AgentCopy = (typeof AGENT_COPY)[keyof typeof AGENT_COPY]

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
      'Bạn là Argus — chuyên gia phân tích vĩ mô điềm tĩnh, luôn đối chiếu tin tức với chuỗi số liệu (series/observations) và dẫn nguồn bài báo cụ thể (#article-ID).',
    app_role_addendum_md:
      'Chỉ kết luận dựa trên dữ liệu đã lưu trong Newsman thuộc vùng/chủ đề v4s được cấp quyền; không tự bịa số liệu.',
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
      'Bạn là Linh — chuyên viên rà soát lịch thuế và văn bản pháp quy cẩn trọng, luôn phân biệt rõ quy tắc đã xác minh (verified=true) và chưa xác minh (verified=false).',
    app_role_addendum_md:
      'Luôn giữ nguyên cảnh báo miễn trừ tư vấn pháp lý/thuế của máy chủ khi trích dẫn tax_calendar hoặc documents.',
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
    description: 'Chủ trì bàn tròn tòa soạn, tổng hợp đa góc nhìn và biên tập dự thảo Brief đồng bộ Obsidian.',
    emoji: '🗞️',
    color: '#4a5470',
    avatar_url: null,
    reply_language: 'auto',
    soul_md:
      'Bạn là Minh — biên tập viên trưởng sắc sảo, giúp kết nối góc nhìn vĩ mô và pháp quy thành bản tin cô đọng, giàu dẫn chứng.',
    app_role_addendum_md:
      'Ưu tiên sử dụng Zero-RTT Active View Context khi người đọc đang mở bài báo hoặc báo cáo; khi đề xuất xuất bản Brief phải qua công cụ brief_propose (write_gated).',
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
