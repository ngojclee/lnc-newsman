import { FormEvent, useEffect, useRef, useState } from 'react'
import { createRoot } from 'react-dom/client'
import { AgentsWorkspace } from './AgentsWorkspace.tsx'
import { AGENT_COPY } from './agentTypes.ts'
import { CatalogManager } from './CatalogManager'
import { CompanionDrawer } from './CompanionDrawer.tsx'
import { Select } from './Select'
import { appLinks } from './appLinks'
import { THEMES, ReaderMode, applyMode, isMode } from './themes'
import './style.css'
import './agent-kit/agent-kit-hygiene.css'

type Lang = 'vi' | 'en'
type Page = 'today' | 'topics' | 'regions' | 'market' | 'stories' | 'reports' | 'agents' | 'alerts' | 'timeline' | 'saved' | 'settings'
type Topic = { slug: string; label_vi: string; label_en: string; parent_slug: string | null; enabled: boolean }
type SourceFacet = { slug: string; name: string; tier: string; region: string; articles: number }
type Stats = { collector_enabled: boolean; fulltext_auto_enabled?: boolean; articles_total: number; articles_24h: number; articles_with_content?: number; articles_pending_content?: number; sources_enabled: number; sources_paused: number; topic_counts: Record<string, number> }
type Dashboard = { topics: Topic[]; sources: SourceFacet[]; stats: Stats }
type Settings = {
  version: string; language: Lang; default_reader_mode: ReaderMode; collector_enabled: boolean; fulltext_auto_enabled: boolean; connectors_auto_enabled: boolean;
  focus_profile: string; brief_morning_time: string; brief_evening_time: string; brief_timezone: string;
  telegram_mode: 'relay' | 'direct' | 'off'; telegram_mode_stored: 'relay' | 'direct' | 'off'; telegram_env_override: boolean;
  telegram_ready: boolean; telegram_reason?: string; telegram_chat_id: number | null; telegram_alerts_topic_id: number | null;
  telegram_has_relay_token: boolean; telegram_has_bot_token: boolean
}
type TelegramStatus = { ready: boolean; reason?: string; queue: { pending: number; sending: number; sent: number; failed: number } }
type Labels = { topics: string[]; entities: string[] }
type Article = { id: number; title: string; url: string; summary?: string | null; content_source?: string; word_count?: number; published_at: string | null; fetched_at: string | null; topic_slug: string | null; region_code: string | null; source_name: string; source_tier: string; labels?: Labels }
type ArticleDetail = Article & { content_text: string | null; content_fetched_at: string | null; content_error: string | null; source_slug: string; source_attribution: string }
type ArticleDetailPayload = { data: ArticleDetail; notes: NoteItem[] }
type AlertRule = { id: number; slug: string; label_vi: string; label_en: string; enabled: boolean; match_kind: string; match_value: string; level: string; cooldown_seconds: number; quiet_from: string; quiet_to: string; timezone: string; channels: string; last_fired_at: number | null }
type AlertDelivery = { id: number; rule_slug: string; rule_label_vi: string; rule_label_en: string; article_id: number; article_title: string; article_url: string; level: string; channel: string; title: string; body: string; read: boolean; created_at: number }
type Region = { code: string; parent_code: string | null; label_vi: string; label_en: string; currency: string; timezone: string; language: string; enabled: boolean; builtin: boolean; managed: boolean; source_count: number }
type PackIssue = { path: string; code: string; detail: string | null }
type PackStatus = { last_loaded_at: number | null; issues: PackIssue[]; regions_registered: number; sources_registered: number }
type RegionsPayload = { data: Region[]; effective: string[]; pack?: PackStatus }
type SeriesRow = { code: string; provider: string; region: string; label_vi: string; label_en: string; unit: string; frequency: string; attribution: string }
type Observation = { observed_at: number; as_of: number; revision: number; value: number; quality: string; source_url: string | null }
type SeriesPayload = { data: SeriesRow[]; effective: string[] }
type ObservationsPayload = { data: Observation[]; series: SeriesRow }
type SecretMeta = { name: string; key_version: number; updated_at: string }
type DocRow = { provider: string; region: string; doc_type: string; agency: string; number: string; title: string; effective_on: string | null; published_on: string | null; url: string; summary: string | null; topic: string; disclaimer: string }
type DocsPayload = { data: DocRow[]; effective: string[] }
type TaxDeadline = { code: string; region: string; name_vi: string; name_en: string; cadence: string; source_code: string; url: string; note: string; verified: boolean; lead_days: number; due_on: string; days_until: number; due_soon: boolean; nominal_on: string; weekend_rule: string }
type TaxPayload = { data: TaxDeadline[]; disclaimer_vi: string; disclaimer_en: string; holidays_modelled: boolean }
type AgentWrite = { id: number; at: number; client_id: string | null; app_id: string | null; tool: string; record_kind: string; record_id: number | null; outcome: string; note: string | null }
type AgentAnalysis = { id: number; analysis_id?: number; stable_key?: string; kind: string; title: string; body?: string; topic_slug: string | null; region_code: string | null; author: string | null; created_at?: number; updated_at: number; source_article_ids: number[] }
type CitedArticle = { id: number; title: string; url: string; published_at: string | null; source_name: string; source_tier: string }
type NoteItem = { id: number; article_id: number | null; document_id: number | null; analysis_id: number | null; body: string; author: string | null; created_at: number; updated_at: number }
type AnalysisDetailPayload = { data: AgentAnalysis; cited_articles: CitedArticle[]; notes: NoteItem[] }
type ResearchResponse = { answer: string; model: string; tokens: number; cached: boolean; source_article_ids: number[]; saved_analysis_id: number | null; stable_key: string | null }
type ConnectorSource = 'env' | 'admin' | 'unset'
type ConnectorCard = { enabled: boolean; audience: string; max_scopes: string[]; bearer: ConnectorSource; delegation_secret: ConnectorSource; audience_source: ConnectorSource; secret_store_open: boolean }
type ProviderStatus = { configured: boolean; base_host: string | null; allowed_hosts: string[]; models: [string, string, string]; agent: string; timeout_seconds: number; tokens_used_today: number; daily_budget: number; calls_today: number; has_secret: boolean }
type LegalSource = { code: string; region: string; name_vi: string; name_en: string; access_kind: string; endpoint: string; access_note: string; licence: string; attribution: string; terms_verdict: string; terms_checked_on: string; terms_url: string; terms_quote: string; terms_note: string; rate_limit_per_day: number; enabled: boolean; pollable: boolean; block_reason: string | null; evidence_age_days: number | null; evidence_stale: boolean }
type ConnectorStatus = { id: string; provider: string; region: string; enabled: boolean; health: string; last_error: string | null; last_run_at: number | null; next_run_at: number | null; requests_today: number; daily_quota: number; needs_secret: boolean }
/** The reader filters a link or the sidebar can set. */
type Filter = { topic: string | null; source: string | null; q?: string }

const copy = {
  vi: {
    today: 'Hôm nay', topics: 'Chủ đề', stories: 'Câu chuyện', reports: 'Báo cáo', alerts: 'Cảnh báo', timeline: 'Dòng thời gian', saved: 'Đã lưu', settings: 'Cài đặt',
    subtitle: 'Không gian đọc và nghiên cứu tin', signIn: 'Đăng nhập quản trị', key: 'Khóa quản trị', keyHint: 'Dùng khóa MANAGEMENT_KEY được cấu hình cho ứng dụng.', enter: 'Vào Newsman', invalid: 'Không đăng nhập được. Hãy kiểm tra khóa và thử lại.', limited: 'Tạm khóa đăng nhập do thử quá nhiều lần. Hãy thử lại sau 5 phút.', missing: 'Chưa cấu hình MANAGEMENT_KEY trên máy chủ.', welcome: 'Chào mừng trở lại.', checking: 'Đang kiểm tra…',
    briefing: 'Một nơi để đọc sâu hơn', briefingSub: 'Tin mới từ các nguồn đang bật — lưu sẵn tóm tắt và toàn văn sạch để đọc ngay trong ứng dụng, tìm kiếm toàn văn hoặc cho Agent phân tích sâu.', noStories: 'Chưa có tin', noStoriesSub: 'Hãy thêm nguồn RSS/Atom, bật nguồn và bật thu thập tự động trong Cài đặt, hoặc bấm “Quét ngay” ở từng nguồn.', nextPhase: 'Thu thập & Lưu trữ nội dung',
    latest: 'Tin mới nhất', articles24h: 'Tin trong 24 giờ', articlesTotal: 'Tổng số tin', sourcesEnabled: 'Nguồn đang bật', sourcesPaused: 'Nguồn tạm dừng do lỗi', allTopics: 'Tất cả', loadMore: 'Xem thêm', loading: 'Đang tải…', loadFailed: 'Không tải thêm được — thử lại', fetchedAt: 'Lấy về', noDate: 'Không rõ ngày đăng',
    collectorOn: 'Đang bật thu thập tự động', collectorOff: 'Thu thập tự động đang tắt', collectorHelp: 'Khi bật, hệ thống quét các nguồn đang bật theo khoảng quét đã đặt, cách nhau ít nhất 5 giây cho mỗi máy chủ. Nguồn lỗi 5 lần liên tiếp sẽ tự tạm dừng.', collectorToggle: 'Bật hoặc tắt thu thập tự động', topicArticles: 'tin',
    alertsTitle: 'Cảnh báo', alertsSub: 'Thông báo trong ứng dụng khi có tin mới khớp luật; luật có thể gửi thêm vào Telegram nếu kênh đó đang bật.', alertsEmpty: 'Chưa có cảnh báo nào', alertsEmptySub: 'Bật một luật cảnh báo trong Cài đặt; thông báo sẽ xuất hiện khi có tin mới khớp.', alertsUnread: 'chưa đọc', markRead: 'Đã đọc', markAllRead: 'Đánh dấu đã đọc', alertsSweep: 'Chạy quét ngay', alertsSwept: (n: number) => `Đã tạo ${n} thông báo`, alertsRules: 'Luật cảnh báo', alertsRulesSub: 'Mỗi luật khớp một chủ đề, vùng, thực thể hoặc từ khóa. Một sự kiện chỉ báo đúng một lần; cooldown và giờ yên lặng có hiệu lực.', alertCooldown: 'Cooldown (giây)', alertQuiet: 'Giờ yên lặng', alertLevel: 'Mức', alertMatch: 'Khớp', alertOff: 'Đang tắt', alertChannelGap: 'Kênh trong ứng dụng luôn hoạt động. Telegram chỉ nhận thông báo khi đã bật và cấu hình xong ở khối bên dưới; email/webhook thuộc bước sau.', entities: 'Thực thể',
    changeTheme: 'Đổi nền đọc', refresh: 'Làm mới', topicsTracked: 'Chủ đề đang bật', pendingAlerts: 'Cảnh báo', upcoming: 'Chưa triển khai',
    settingsTitle: 'Cài đặt hiển thị', settingsSub: 'Đặt mặc định cho lần đầu mở ứng dụng. Thiết lập trên trình duyệt hiện tại sẽ được ưu tiên.', language: 'Ngôn ngữ mặc định', readerMode: 'Nền đọc mặc định', save: 'Lưu mặc định', saving: 'Đang lưu…', savedToast: 'Đã lưu cài đặt', admin: 'Quản trị', signOut: 'Đăng xuất',
    emptyTitle: 'Tính năng đang được chuẩn bị', emptySub: 'Báo cáo, cảnh báo, dòng thời gian và mục đã lưu sẽ có ở các giai đoạn sau.', sampleTopics: 'Các chủ đề đang bật', sampleTopicsSub: 'Chọn một chủ đề để xem tin của chủ đề đó và các chủ đề con. Quản lý chủ đề trong phần Cài đặt.', tgTitle: 'Kênh Telegram', tgSub: 'Nhận cảnh báo qua Telegram của fleet hoặc bot riêng. App không lưu token hiển thị lại được — chỉ ghi đè.', tgMode: 'Đường gửi', tgRelay: 'Qua relay telegram.lengoc.me', tgDirect: 'Bot API trực tiếp', tgOff: 'Tắt', tgChatId: 'Chat / group id', tgTopic: 'Topic riêng cho cảnh báo (tuỳ chọn)', tgSave: 'Lưu cấu hình', tgReady: 'Sẵn sàng gửi', tgNotReady: 'Chưa gửi được', tgEnvOverride: 'Đang bị ghi đè bởi biến môi trường TELEGRAM_MODE', tgRelayToken: 'Đã có relay token', tgBotToken: 'Đã có bot token', tgQueue: 'Hàng đợi', tgPending: 'chờ', tgSent: 'đã gửi', tgFailed: 'lỗi', tgFlushNow: 'Gửi ngay', tgFlushed: (n: number) => `Đã gửi ${n} cảnh báo`, settingsError: 'Không tải hoặc lưu được cài đặt. Hãy thử lại.', loadError: 'Không tải được dữ liệu. Hãy thử làm mới.', dismiss: 'Đóng', failedLogout: 'Không đăng xuất được; hãy thử lại.', changeLanguage: 'Chuyển ngôn ngữ',
    region: 'Vùng', allRegions: 'Mọi vùng đang bật', regionsTitle: 'Vùng và region pack', regionsSub: 'Bật vùng để job quét và trang đọc dùng vùng đó cùng các vùng cha. Vùng mới phát hiện từ thư mục pack luôn ở trạng thái tắt.',
    reloadRegions: 'Nạp lại vùng', reloading: 'Đang nạp…', packIssues: 'Lỗi pack', packClean: 'Không có lỗi pack.', packSummary: (r: number, s: number) => `${r} vùng, ${s} nguồn từ pack`, packNever: 'Chưa nạp pack', packLast: 'Lần nạp gần nhất',
    regionTree: 'Cây vùng', regionEnable: 'Bật hoặc tắt vùng', regionInherit: 'gồm vùng cha', noRegions: 'Chưa có vùng nào.', regionError: 'Không đổi được vùng. Kiểm tra vùng cha trước.',
    regions: 'Vùng',
    regionsPage: 'Vùng', regionsPageSub: 'Tin, văn bản chính thức và lịch thuế theo vùng đang bật (gồm vùng cha).', regionNews: 'Tin theo vùng', regionDocs: 'Văn bản chính thức', regionTax: 'Lịch thuế', noDocs: 'Chưa có văn bản nào cho vùng này.', noTax: 'Chưa có hạn nộp nào được khai báo cho vùng này.',
    dueIn: (n: number) => n === 0 ? 'hôm nay' : `còn ${n} ngày`, dueSoon: 'Sắp tới hạn', unverified: 'Chưa xác minh với cơ quan thuế', verified: 'Đã xác minh', taxNotice: 'Không phải tư vấn pháp lý/thuế. Hạn nộp do region pack khai báo, chưa gọi mạng; hãy kiểm tra lại với cơ quan thuế.', holidaysNote: 'Chưa tính ngày lễ công cộng; chỉ dời theo cuối tuần khi pack khai báo.', docSummary: 'Tóm tắt tự viết', docNoSummary: 'Chưa có tóm tắt tự viết cho văn bản này.', effectiveOn: 'Hiệu lực', publishedOn: 'Ban hành', searchDocs: 'Tìm văn bản', searchHint: 'Gõ không dấu cũng tìm được (vd: thue).', allTopicsDoc: 'Tất cả chủ đề',
    market: 'Thị trường', marketTitle: 'Chuỗi số liệu', marketSub: 'Dữ liệu số theo vùng. Chưa kết nối nhà cung cấp nào: danh sách này chỉ có khi một connector thật được duyệt và chạy.', noSeries: 'Chưa có chuỗi số liệu', noSeriesSub: 'P2 mới có kho chuỗi và khung connector. Dữ liệu thật cần chọn nhà cung cấp (plan mục 8).',
    seriesPoints: (n: number) => `${n} điểm`, observing: 'Lấy dữ liệu', latestValue: 'Giá trị mới nhất', asOf: 'Ghi nhận lúc', observedAt: 'Kỳ dữ liệu', revision: (n: number) => `bản ${n}`, noAttribution: 'Chưa có dòng ghi nguồn cho chuỗi này.',
    hubTitle: 'LNC Agent Hub', hubSub: 'Kết nối Hub cho phân loại, tóm tắt và nghiên cứu. Khóa chỉ ghi, không hiện lại. Cấu hình địa chỉ do máy chủ quyết định (biến môi trường), không nhập ở đây.', hubKeyName: 'hub-api-key', hubKeyHint: 'Khóa riêng của Newsman trên Hub. Dán vào đây; giá trị không bao giờ được trả lại.', hubCheck: 'Kiểm tra kết nối', hubChecking: 'Đang kiểm tra…', hubCheckOk: (n: number) => `Kết nối được, thấy ${n} model. Chỉ liệt kê model, chưa sinh nội dung, chưa tốn token.`, hubCheckFail: 'Không kết nối được Hub. Kiểm tra địa chỉ, allowlist host và khóa.', hubHost: 'Host', hubAllowed: 'Allowlist', hubModels: 'Model theo tác vụ', hubClassify: 'Phân loại', hubSummarise: 'Tóm tắt', hubAnalyse: 'Phân tích', hubAgent: 'Agent', hubBudget: 'Token hôm nay', hubSecret: 'Khóa', hubSaved: 'Đã lưu khóa Hub.', hubHasKey: 'đã có khóa', hubNoKey: 'chưa có khóa', hubNotConfigured: 'Máy chủ chưa đặt HUB_BASE_URL hoặc host chưa nằm trong HUB_ALLOWED_HOSTS.',
    secretsTitle: 'Khóa nhà cung cấp', secretsSub: 'Chỉ ghi, không bao giờ hiển thị lại. Cần LNC_MASTER_KEY hợp lệ trên máy chủ.', secretName: 'Tên khóa', secretValue: 'Giá trị', saveSecret: 'Lưu khóa', deleting: 'Đang xóa…', removeSecret: 'Xóa', secretSaved: 'Đã lưu khóa.', notConfigured: 'Máy chủ chưa cấu hình LNC_MASTER_KEY nên kho khóa đang đóng.', mcpWrites: 'Ghi qua connector', mcpWritesSub: 'Mỗi lần agent ghi đều có nhãn nguồn. Nhãn là client id do Hub cấp, không phải thứ agent tự đặt.', mcpNoWrites: 'Chưa có agent nào ghi.', mcpAnalyses: 'Brief đã lưu', mcpSources: 'bài nguồn', mcpNoAnalyses: 'Chưa có brief nào.', mcpOwner: 'chủ dự án', mcpTitle: 'Kết nối Business MCP Hub', mcpSub: 'Hub gọi vào /mcp bằng khóa riêng của connector kèm chữ ký ủy quyền từng agent (newsman:read để truy vấn, newsman:write để lưu brief, ghi chú phản hồi & đồng bộ hai chiều với Obsidian).', mcpReady: 'Đã ghép nối', mcpNotReady: 'Chưa ghép nối', mcpBearer: 'Khóa connector', mcpSecret: 'Khóa ủy quyền', mcpAudience: 'Audience', mcpScopes: 'Phạm vi tối đa', mcpFromEnv: 'biến môi trường', mcpFromAdmin: 'nhập ở đây', mcpUnset: 'chưa đặt', mcpSet: 'đã đặt', mcpSave: 'Lưu', mcpSaved: 'Đã lưu cấu hình connector.', mcpStoreClosed: 'Máy chủ chưa có LNC_MASTER_KEY nên chưa lưu được khóa connector.', mcpHint: 'Hai khóa nằm ở mục "Khóa nhà cung cấp" bên dưới, tên `connector-bearer` và `delegation-secret`. Chúng là bí mật nên chỉ ghi, không hiện lại.', legalTitle: 'Nguồn văn bản pháp luật', legalSub: 'Mỗi nguồn kèm điều khoản sử dụng đã đọc và ngày kiểm. Nguồn chưa đọc được điều khoản thì không bật được.', legalBlocked: 'Chưa được bật', legalBlockedUnverified: 'Chưa kiểm điều khoản', legalBlockedProhibited: 'Không được phép', legalTurnsOn: 'Bật theo dõi', legalTurnsOff: 'Tắt theo dõi', legalTerms: 'Điều khoản', legalChecked: 'Kiểm ngày', legalStale: 'Điều khoản đã kiểm quá lâu, cần đọc lại', legalNoChannel: 'Chưa có kênh truy cập chính thức', legalReasonUnverified: 'Chưa đọc được điều khoản sử dụng của trang nên chưa theo dõi.', legalReasonProhibited: 'Điều khoản của trang không cho phép lấy tự động.', valid_terms_unverified: 'Chưa đọc được điều khoản sử dụng nên không bật được nguồn này.', valid_terms_prohibit_polling: 'Điều khoản của trang không cho phép lấy tự động.', valid_no_documented_channel: 'Nguồn chưa có kênh truy cập chính thức.', valid_region_disabled: 'Vùng của nguồn này đang tắt.', connectors: 'Connector', connectorHealth: 'Sức khỏe', quota: 'Hạn mức hôm nay', needsSecret: 'cần khóa', neverRan: 'Chưa chạy',
    // --- browse sidebar, rails and the filter chip (W1/W2 in the shared UI plan)
    browse: 'Duyệt', browseAll: 'Mọi tin đang đọc', byTopic: 'Chủ đề', byRegion: 'Vùng', bySource: 'Nguồn', quickFilter: 'Lọc nhanh', filterPlaceholder: 'Tên chủ đề hoặc nguồn…', clearFilter: 'Bỏ lọc', showing: 'Đang xem', noMatch: 'Không có mục nào khớp', railLatest: 'Tin mới nhất', railAlerts: 'Cảnh báo gần đây', railWatch: 'Theo dõi', seeAll: 'Xem tất cả', emptyRail: 'Chưa có gì', itemUnit: 'tin', tabbarLabel: 'Điều hướng một tay', themeLabel: 'Nền đọc',
    railBriefs: 'Bản tin phân tích mới nhất', openReports: 'Mở Báo cáo', noBriefsRail: 'Chưa có bản tin phân tích — mở mục Báo cáo để yêu cầu Agent tổng hợp hoặc nhận đồng bộ từ MCP / Obsidian.',
    reportsTitle: 'Báo cáo & Trợ lý Phân tích', reportsSub: 'Đọc bản tin do Agent tổng hợp, đối chiếu bài nguồn trích dẫn, ghi ý kiến phản hồi hai chiều (đồng bộ Obsidian) hoặc yêu cầu phân tích trực tiếp.',
    askTitle: 'Hỏi & Yêu cầu Agent phân tích', askSub: 'Agent tổng hợp dựa trên toàn văn & tóm tắt bài báo đã lưu (tìm kiếm FTS5), chuỗi số liệu, lịch thuế và Hồ sơ trọng tâm của anh — không tự bịa nguồn.',
    askPlaceholder: 'Ví dụ: Tóm tắt các diễn biến lãi suất, tỷ giá và chính sách đáng chú ý hôm nay…',
    askChip1: 'Điểm nóng 24h qua', askChip1Prompt: 'Tóm tắt các tin tức và diễn biến quan trọng nhất đang lưu trong Newsman, nhóm theo chủ đề và nêu điểm cần theo dõi.',
    askChip2: 'Vĩ mô & Lãi suất', askChip2Prompt: 'Phân tích bức tranh vĩ mô, lãi suất và chuỗi số liệu mới nhất đang có; nêu rủi ro và cơ hội chính.',
    askChip3: 'Thuế & Pháp lý', askChip3Prompt: 'Rà soát các mốc lịch thuế sắp tới và văn bản chính thức đang lưu, nhắc những việc cần kiểm tra sớm.',
    askMode: 'Chế độ phân tích', askModeAnalyse: 'Phân tích nhanh (Model)', askModeAgent: 'Nghiên cứu sâu (Hub Agent)',
    askSaveBrief: 'Lưu kết quả thành Báo cáo (đồng bộ Obsidian)', askBriefTitle: 'Tiêu đề báo cáo (tuỳ chọn)',
    askSubmit: 'Phân tích ngay', askRunning: 'Agent đang phân tích…', askSavedAs: 'Đã lưu báo cáo:',
    askHubHint: 'Chưa gọi được LNC Agent Hub (hãy kiểm tra HUB_BASE_URL và khóa hub-api-key trong Cài đặt → LNC Agent Hub). Các báo cáo gửi từ Business MCP Hub vẫn đọc và phản hồi bình thường.',
    noBriefs: 'Chưa có báo cáo phân tích', noBriefsSub: 'Bấm “Phân tích ngay” ở trên (bật Lưu thành Báo cáo) hoặc để Agent định kỳ gửi brief qua cổng /mcp.',
    citedArticles: 'Bài nguồn trích dẫn', noCited: 'Báo cáo này chưa gắn mã bài nguồn cụ thể.',
    feedbackTitle: 'Ý kiến của anh & Phản hồi hai chiều', feedbackSub: 'Ghi chú của anh trên web được bảo vệ (Agent không thể sửa/xóa); ý kiến anh viết trong Obsidian cũng tự gắn về đúng mã báo cáo này.',
    feedbackPlaceholder: 'Viết nhận định, câu hỏi đào sâu hoặc hướng điều chỉnh cho kỳ báo cáo tới…', feedbackSubmit: 'Gửi ý kiến', feedbackEmpty: 'Chưa có ghi chú phản hồi nào trên báo cáo này.',
    focusTitle: 'Hồ sơ trọng tâm & Lịch nhận tin', focusSub: 'Giúp cả Trợ lý nội bộ lẫn các Agent đặt lịch qua MCP (`reader_preferences`) bám đúng ưu tiên và khung giờ đọc của anh.',
    focusLabel: 'Chủ đề, chỉ số & câu hỏi trọng tâm', focusHint: 'Tối đa 1.000 ký tự. Ví dụ: Lãi suất FED/ECB, tỷ giá USD/VND, BTC, thuế doanh nghiệp Việt Nam & xuất nhập khẩu.',
    morningTime: 'Giờ bản tin sáng (HH:MM)', eveningTime: 'Giờ bản tin chiều (HH:MM)', briefTz: 'Múi giờ (IANA)',
    connectorAutoOn: 'Đang bật tự động chạy Connector định kỳ', connectorAutoOff: 'Tự động chạy Connector đang tắt', connectorAutoHelp: 'Tách riêng với RSS để anh bấm “Lấy ngay” kiểm thử từng nhà cung cấp trước khi bật chạy nền tự động.',
    connectorRunNow: 'Lấy ngay', connectorRunning: 'Đang lấy…', connectorEnable: 'Bật', connectorDisable: 'Tắt',
    connectorRanOk: (obs: number, docs: number, ev: number) => `Đã lấy dữ liệu: ${obs} điểm số liệu, ${docs} văn bản, ${ev} sự kiện.`,
    connectorErrRegion: 'Vùng của connector này đang tắt — hãy bật vùng ở mục Vùng trước.', connectorErrSecret: 'Thiếu khóa API của nhà cung cấp — hãy lưu khóa ở mục Khóa nhà cung cấp trước.', connectorErrConfig: 'Thiếu cấu hình máy chủ (ví dụ SEC_USER_AGENT).', connectorErrCache: 'Dữ liệu vừa lấy vẫn còn mới trong cửa sổ cache.', connectorErrQuota: 'Đã hết hạn mức truy vấn hôm nay của connector này.',
    searchArticles: 'Tìm toàn văn bài báo', searchArticlesPlaceholder: 'Tìm tiêu đề hoặc nội dung toàn văn đã lưu (không dấu cũng khớp, vd: lai suat, thue)…', searchGo: 'Tìm',
    readInApp: 'Đọc ngay', openOriginal: 'Mở bài gốc ↗', closeReader: 'Đóng',
    extractNow: 'Tải toàn văn sạch', reExtract: 'Tải lại toàn văn', extracting: 'Đang bóc toàn văn sạch từ trang gốc…',
    contentWeb: 'Đã lưu toàn văn', contentFeed: 'Nội dung RSS', contentSummary: 'Tóm tắt', wordsUnit: 'từ',
    readMinutes: (m: number) => `${m} phút đọc`,
    readerNoBody: 'Trang gốc không cung cấp văn bản bài viết tĩnh (có thể là cổng tải tệp PDF/ZIP hoặc yêu cầu JavaScript). Anh có thể mở trực tiếp đường dẫn gốc bên dưới.',
    articleNotesTitle: 'Ghi chú riêng cho bài báo này', articleNotesPlaceholder: 'Ghi lại điểm đáng chú ý, số liệu hoặc ý tưởng liên hệ từ bài báo…',
    fulltextAutoOn: 'Đang bật tự động tải & lưu toàn văn sạch', fulltextAutoOff: 'Tự động tải toàn văn đang tắt (chỉ lưu RSS)',
    fulltextAutoHelp: 'Tự động truy xuất bài gốc ở nền (qua cổng chống SSRF, giãn cách tối thiểu 5 giây/máy chủ), lọc bỏ quảng cáo để lưu văn bản sạch vào SQLite và chỉ mục FTS5.',
    articlesArchived: 'Bài đã có nội dung', articlesPending: 'Chờ tải toàn văn',
    backfillBtn: 'Tải toàn văn cho bài cũ (tối đa 10 bài/lượt)', backfilling: 'Đang bóc toàn văn…',
    backfillDone: (ok: number, attempted: number) => `Đã lưu toàn văn sạch cho ${ok}/${attempted} bài báo.`,
    reportsSalonTitle: 'Hội thảo Đa Trợ lý & Không gian Tòa soạn',
    reportsSalonSub: 'Mở Bàn tròn đa trợ lý có trí nhớ vùng/chủ đề, Bảng trắng tóm tắt vòng thảo luận và kỹ năng được cấp từ Hub.',
    reportsSalonBtn: 'Mở Hội thảo Đa Trợ lý',
    ...AGENT_COPY.vi,
  },
  en: {
    today: 'Today', topics: 'Topics', stories: 'Stories', reports: 'Reports', alerts: 'Alerts', timeline: 'Timeline', saved: 'Saved', settings: 'Settings',
    subtitle: 'A place to read and research', signIn: 'Management sign in', key: 'Management key', keyHint: 'Use the MANAGEMENT_KEY configured for this application.', enter: 'Enter Newsman', invalid: 'Could not sign in. Check the key and try again.', limited: 'Too many sign-in attempts. Please try again in 5 minutes.', missing: 'MANAGEMENT_KEY is not configured on the server.', welcome: 'Welcome back.', checking: 'Checking…',
    briefing: 'A quieter place to read', briefingSub: 'New items from enabled sources with stored excerpts and clean extracted full text for distraction-free reading, full-text search and grounded AI analysis.', noStories: 'No news yet', noStoriesSub: 'Add an RSS/Atom source, enable it and turn on automatic collection in Settings, or press “Fetch now” on a source.', nextPhase: 'Collection & Content Archive',
    latest: 'Latest news', articles24h: 'Items in 24 hours', articlesTotal: 'Total items', sourcesEnabled: 'Enabled sources', sourcesPaused: 'Sources paused by errors', allTopics: 'All', loadMore: 'Load more', loading: 'Loading…', loadFailed: 'Could not load more — try again', fetchedAt: 'Fetched', noDate: 'Publication date unknown',
    collectorOn: 'Automatic collection is on', collectorOff: 'Automatic collection is off', collectorHelp: 'When on, enabled sources are checked at their configured interval, at least 5 seconds apart per host. A source that fails 5 times in a row is paused automatically.', collectorToggle: 'Turn automatic collection on or off', topicArticles: 'items',
    alertsTitle: 'Alerts', alertsSub: 'In-app notifications when new items match a rule; a rule can also post to Telegram while that channel is on.', alertsEmpty: 'No alerts yet', alertsEmptySub: 'Enable an alert rule in Settings; notifications appear when a new item matches.', alertsUnread: 'unread', markRead: 'Mark read', markAllRead: 'Mark all read', alertsSweep: 'Run sweep now', alertsSwept: (n: number) => `Created ${n} notifications`, alertsRules: 'Alert rules', alertsRulesSub: 'Each rule matches a topic, region, entity or keyword. One event notifies exactly once; cooldown and quiet hours apply.', alertCooldown: 'Cooldown (seconds)', alertQuiet: 'Quiet hours', alertLevel: 'Level', alertMatch: 'Match', alertOff: 'Off', alertChannelGap: 'The in-app channel always works. Telegram receives alerts only once it is switched on and fully configured below; email and webhooks come later.', entities: 'Entities',
    changeTheme: 'Change reading mode', refresh: 'Refresh', topicsTracked: 'Enabled topics', pendingAlerts: 'Alerts', upcoming: 'Not implemented yet',
    settingsTitle: 'Display settings', settingsSub: 'Set defaults for first-time visitors. This browser’s own preferences take precedence.', language: 'Default language', readerMode: 'Default reading mode', save: 'Save defaults', saving: 'Saving…', savedToast: 'Settings saved', admin: 'Admin', signOut: 'Sign out',
    emptyTitle: 'This feature is being prepared', emptySub: 'Reports, alerts, timeline and saved items arrive in later phases.', sampleTopics: 'Enabled topics', sampleTopicsSub: 'Pick a topic to read its items, including child topics. Manage topics in Settings.', tgTitle: 'Telegram channel', tgSub: 'Deliver alerts through the fleet Telegram relay or a bot of our own. Tokens are write-only: they can be replaced, never read back.', tgMode: 'Delivery path', tgRelay: 'Via relay telegram.lengoc.me', tgDirect: 'Direct Bot API', tgOff: 'Off', tgChatId: 'Chat / group id', tgTopic: 'Topic reserved for alerts (optional)', tgSave: 'Save channel', tgReady: 'Ready to send', tgNotReady: 'Not sending yet', tgEnvOverride: 'Overridden by the TELEGRAM_MODE environment variable', tgRelayToken: 'Relay token stored', tgBotToken: 'Bot token stored', tgQueue: 'Queue', tgPending: 'pending', tgSent: 'sent', tgFailed: 'failed', tgFlushNow: 'Send now', tgFlushed: (n: number) => `Sent ${n} alerts`, settingsError: 'Could not load or save settings. Please try again.', loadError: 'Could not load data. Please refresh.', dismiss: 'Dismiss', failedLogout: 'Could not sign out. Please try again.', changeLanguage: 'Change language',
    region: 'Region', allRegions: 'All enabled regions', regionsTitle: 'Regions and region packs', regionsSub: 'Enable a region so jobs and the reader use it together with its parent regions. Regions discovered from the pack directory always start disabled.',
    reloadRegions: 'Reload regions', reloading: 'Reloading…', packIssues: 'Pack issues', packClean: 'No pack issues.', packSummary: (r: number, s: number) => `${r} regions, ${s} sources from packs`, packNever: 'Pack not loaded yet', packLast: 'Last load',
    regionTree: 'Region tree', regionEnable: 'Turn the region on or off', regionInherit: 'includes parents', noRegions: 'No regions yet.', regionError: 'Could not change the region. Check the parent region first.',
    regions: 'Regions',
    regionsPage: 'Regions', regionsPageSub: 'News, official documents and tax deadlines for the enabled regions (parents included).', regionNews: 'News by region', regionDocs: 'Official documents', regionTax: 'Tax calendar', noDocs: 'No documents for this region yet.', noTax: 'No deadline is declared for this region.',
    dueIn: (n: number) => n === 0 ? 'today' : `in ${n} days`, dueSoon: 'Due soon', unverified: 'Not checked with the tax authority', verified: 'Verified', taxNotice: 'Not legal or tax advice. Deadlines come from a declared region pack rule and no network call was made; verify them with the tax authority.', holidaysNote: 'Public holidays are not modelled; a date is only shifted by a declared weekend rule.', docSummary: 'Written summary', docNoSummary: 'No written summary for this document yet.', effectiveOn: 'Effective', publishedOn: 'Published', searchDocs: 'Search documents', searchHint: 'Accents are optional (e.g. thue).', allTopicsDoc: 'All topics',
    market: 'Markets', marketTitle: 'Time series', marketSub: 'Numeric data by region. No provider is connected yet: this list only fills once a real connector is approved and running.', noSeries: 'No series yet', noSeriesSub: 'P2 provides only the series store and the connector framework. Real data needs a chosen provider (plan section 8).',
    seriesPoints: (n: number) => `${n} points`, observing: 'Fetch data', latestValue: 'Latest value', asOf: 'Known as of', observedAt: 'Period', revision: (n: number) => `revision ${n}`, noAttribution: 'This series has no credit line yet.',
    hubTitle: 'LNC Agent Hub', hubSub: 'Connect the Hub for classification, summaries and research. The key is write-only and never shown again. The address is server configuration (environment), not entered here.', hubKeyName: 'hub-api-key', hubKeyHint: 'Newsman’s own Hub key. Paste it here; the value is never returned.', hubCheck: 'Check connection', hubChecking: 'Checking…', hubCheckOk: (n: number) => `Connected, ${n} models visible. Model listing only — no content generated, no tokens billed.`, hubCheckFail: 'Could not reach the Hub. Check the address, the host allowlist and the key.', hubHost: 'Host', hubAllowed: 'Allowlist', hubModels: 'Model per task', hubClassify: 'Classify', hubSummarise: 'Summarise', hubAnalyse: 'Analyse', hubAgent: 'Agent', hubBudget: 'Tokens today', hubSecret: 'Key', hubSaved: 'Hub key saved.', hubHasKey: 'key present', hubNoKey: 'no key', hubNotConfigured: 'The server has no HUB_BASE_URL, or the host is not in HUB_ALLOWED_HOSTS.',
    secretsTitle: 'Provider keys', secretsSub: 'Write-only; never shown again. Requires a valid LNC_MASTER_KEY on the server.', secretName: 'Key name', secretValue: 'Value', saveSecret: 'Save key', deleting: 'Deleting…', removeSecret: 'Delete', secretSaved: 'Key saved.', notConfigured: 'The server has no LNC_MASTER_KEY, so the secret store is closed.', mcpWrites: 'Writes through the connector', mcpWritesSub: 'Every agent write carries a source label. The label is the client id the Hub assigned, not something the agent chose.', mcpNoWrites: 'No agent has written anything yet.', mcpAnalyses: 'Stored briefs', mcpSources: 'sources', mcpNoAnalyses: 'No briefs yet.', mcpOwner: 'owner', mcpTitle: 'Business MCP Hub connector', mcpSub: 'The Hub calls /mcp with its own connector key plus a signed delegation per agent (newsman:read for queries, newsman:write for briefs, feedback notes & two-way Obsidian sync).', mcpReady: 'Paired', mcpNotReady: 'Not paired', mcpBearer: 'Connector key', mcpSecret: 'Delegation secret', mcpAudience: 'Audience', mcpScopes: 'Maximum scopes', mcpFromEnv: 'environment', mcpFromAdmin: 'set here', mcpUnset: 'not set', mcpSet: 'set', mcpSave: 'Save', mcpSaved: 'Connector settings saved.', mcpStoreClosed: 'The server has no LNC_MASTER_KEY, so the two connector secrets cannot be stored yet.', mcpHint: 'Both secrets live under "Provider keys" below, named `connector-bearer` and `delegation-secret`. They are secrets: write-only, never shown again.', legalTitle: 'Legal-text sources', legalSub: 'Each source lists the terms we read and the date we checked them. A source whose terms were not read cannot be switched on.', legalBlocked: 'Not available', legalBlockedUnverified: 'Terms not checked', legalBlockedProhibited: 'Not permitted', legalTurnsOn: 'Follow this source', legalTurnsOff: 'Stop following', legalTerms: 'Terms', legalChecked: 'Checked', legalStale: 'The terms were checked a long time ago — read them again', legalNoChannel: 'No official access channel yet', legalReasonUnverified: 'The site’s terms of use could not be read, so this source is not followed.', legalReasonProhibited: 'The site’s terms do not allow automated retrieval.', valid_terms_unverified: 'The terms of use could not be read, so this source cannot be switched on.', valid_terms_prohibit_polling: 'The site’s terms do not allow automated retrieval.', valid_no_documented_channel: 'This source has no official access channel yet.', valid_region_disabled: 'This source’s region is off.', connectors: 'Connectors', connectorHealth: 'Health', quota: 'Today’s quota', needsSecret: 'needs a key', neverRan: 'Never ran',
    browse: 'Browse', browseAll: 'Everything visible', byTopic: 'Topics', byRegion: 'Regions', bySource: 'Sources', quickFilter: 'Quick filter', filterPlaceholder: 'Topic or source name…', clearFilter: 'Clear filter', showing: 'Showing', noMatch: 'Nothing matches', railLatest: 'Latest items', railAlerts: 'Recent alerts', railWatch: 'Following', seeAll: 'See all', emptyRail: 'Nothing yet', itemUnit: 'items', tabbarLabel: 'One-handed navigation', themeLabel: 'Reading theme',
    railBriefs: 'Latest briefs', openReports: 'Open Reports', noBriefsRail: 'No briefs stored yet — open Reports to ask the Analyst or sync via MCP / Obsidian.',
    reportsTitle: 'Briefs & Research Analyst', reportsSub: 'Read agent-authored briefs, check cited source articles, leave two-way feedback notes (synced with Obsidian) or run a grounded analysis.',
    askTitle: 'Ask Newsman Analyst', askSub: 'Grounded strictly in stored full-text articles & excerpts (FTS5 search), numeric series, declared tax deadlines and your Focus Profile — never invented sources.',
    askPlaceholder: 'Example: Summarise today’s notable rate, FX and policy developments…',
    askChip1: '24h highlights', askChip1Prompt: 'Summarise the most important stored headlines in Newsman grouped by topic and highlight what to watch next.',
    askChip2: 'Macro & Rates', askChip2Prompt: 'Analyse the macro and interest-rate picture from stored headlines and latest series; note key risks and opportunities.',
    askChip3: 'Tax & Official docs', askChip3Prompt: 'Review upcoming declared tax deadlines and stored official documents, noting items to verify early.',
    askMode: 'Analysis mode', askModeAnalyse: 'Quick brief (Model)', askModeAgent: 'Deep research (Hub Agent)',
    askSaveBrief: 'Save result as a Brief (syncs to Obsidian)', askBriefTitle: 'Brief title (optional)',
    askSubmit: 'Run analysis', askRunning: 'Analysing…', askSavedAs: 'Saved brief:',
    askHubHint: 'Could not reach LNC Agent Hub (check HUB_BASE_URL and hub-api-key in Settings → LNC Agent Hub). Briefs pushed via Business MCP Hub still read and accept feedback normally.',
    noBriefs: 'No briefs stored yet', noBriefsSub: 'Click “Run analysis” above (with Save as a Brief checked) or let a scheduled MCP agent publish briefs via /mcp.',
    citedArticles: 'Cited source articles', noCited: 'No specific stored articles were cited.',
    feedbackTitle: 'Your feedback & two-way notes', feedbackSub: 'Notes you write here are protected (agents cannot edit or delete them); feedback written in Obsidian also attaches to this brief.',
    feedbackPlaceholder: 'Write your takeaway, follow-up question or guidance for the next brief…', feedbackSubmit: 'Save note', feedbackEmpty: 'No feedback notes on this brief yet.',
    focusTitle: 'Focus profile & brief schedule', focusSub: 'Guides both the built-in analyst and scheduled MCP agents (`reader_preferences`) on what you prioritise and when you read.',
    focusLabel: 'Priorities, sectors & recurring questions', focusHint: 'Up to 1,000 characters. E.g. Fed/ECB rates, USD/VND, BTC, Vietnam corporate tax & trade.',
    morningTime: 'Morning brief (HH:MM)', eveningTime: 'Evening brief (HH:MM)', briefTz: 'Timezone (IANA)',
    connectorAutoOn: 'Automatic scheduled connector polling is on', connectorAutoOff: 'Automatic connector polling is off', connectorAutoHelp: 'Kept separate from RSS so you can verify each provider with “Run now” before enabling scheduled background runs.',
    connectorRunNow: 'Run now', connectorRunning: 'Running…', connectorEnable: 'Enable', connectorDisable: 'Disable',
    connectorRanOk: (obs: number, docs: number, ev: number) => `Fetched: ${obs} observations, ${docs} documents, ${ev} events.`,
    connectorErrRegion: 'This connector’s region is off — enable the region above first.', connectorErrSecret: 'Required provider API key is missing — save it under Provider keys first.', connectorErrConfig: 'Required server configuration is missing (e.g. SEC_USER_AGENT).', connectorErrCache: 'Recent data is still fresh inside the cache window.', connectorErrQuota: 'Today’s request quota for this connector is exhausted.',
    searchArticles: 'Search stored articles', searchArticlesPlaceholder: 'Search headlines & stored full text (accents optional, e.g. fed rates)…', searchGo: 'Search',
    readInApp: 'Read', openOriginal: 'Original ↗', closeReader: 'Close',
    extractNow: 'Extract full text', reExtract: 'Re-extract full text', extracting: 'Extracting clean article text from source…',
    contentWeb: 'Full text saved', contentFeed: 'RSS content', contentSummary: 'Summary', wordsUnit: 'words',
    readMinutes: (m: number) => `${m} min read`,
    readerNoBody: 'This page does not expose static article text (it may be a PDF/ZIP download portal or require JavaScript). You can open the original link below.',
    articleNotesTitle: 'Personal notes on this article', articleNotesPlaceholder: 'Jot down a takeaway, figure or follow-up from this article…',
    fulltextAutoOn: 'Automatic clean full-text archiving is on', fulltextAutoOff: 'Automatic full-text archiving is off (RSS only)',
    fulltextAutoHelp: 'Fetches article pages in the background (SSRF-safe, at least 5s per-host spacing), strips clutter and stores clean readable text in SQLite + FTS5.',
    articlesArchived: 'Articles with body', articlesPending: 'Pending extraction',
    backfillBtn: 'Extract full text for existing items (up to 10)', backfilling: 'Extracting batch…',
    backfillDone: (ok: number, attempted: number) => `Saved clean full text for ${ok}/${attempted} articles.`,
    reportsSalonTitle: 'Multi-Agent Salon & Newsroom Workspaces',
    reportsSalonSub: 'Open the multi-agent roundtable with region/topic scoping, whiteboard synthesis, and Hub-granted skills.',
    reportsSalonBtn: 'Open Multi-Agent Salon',
    ...AGENT_COPY.en,
  },
} as const
type Copy = (typeof copy)[Lang]

class ApiError extends Error { constructor(readonly status: number, message: string) { super(message) } }
async function api<T>(path: string, init: RequestInit = {}): Promise<T> {
  const headers = new Headers(init.headers)
  headers.set('Content-Type', 'application/json')
  headers.set('X-Requested-With', 'XMLHttpRequest')
  const response = await fetch(path, { ...init, credentials: 'same-origin', headers })
  if (!response.ok) {
    if (response.status === 401 && path !== '/api/auth/login') window.dispatchEvent(new Event('newsman:session-expired'))
    const body = await response.json().catch(() => null)
    throw new ApiError(response.status, body?.reason ? `${body.error}:${body.reason}` : body?.error ?? `Request failed (${response.status})`)
  }
  return response.json() as Promise<T>
}
function storageGet(key: string): string | null { try { return localStorage.getItem(key) } catch { return null } }
function storageSet(key: string, value: string) { try { localStorage.setItem(key, value) } catch { /* preference storage is optional */ } }
const validLang = (value: string | null): value is Lang => value === 'vi' || value === 'en'
const isLocal = () => location.hostname === '127.0.0.1' || location.hostname === 'localhost'
/**
 * Pages the app can actually route to. Everything else would render an empty
 * placeholder, so the phone tab bar offers only real destinations and the rest stay
 * in the desktop sidebar until their phase lands.
 */
const ROUTABLE_PAGES: Page[] = ['today', 'topics', 'regions', 'market', 'reports', 'agents', 'alerts', 'settings']
const pages: Page[] = ['today', 'topics', 'regions', 'market', 'stories', 'reports', 'agents', 'alerts', 'timeline', 'saved', 'settings']
function currentPage(): Page { const hash = location.hash.slice(1) as Page; return pages.includes(hash) ? hash : 'today' }

function App() {
  const storedLang = storageGet('lnc-newsman-language')
  const storedMode = storageGet('lnc-newsman-reader-mode')
  const [authenticated, setAuthenticated] = useState<boolean | null>(null)
  const [managementConfigured, setManagementConfigured] = useState(false)
  const [lang, setLangState] = useState<Lang>(validLang(storedLang) ? storedLang : 'vi')
  const [mode, setModeState] = useState<ReaderMode>(isMode(storedMode) ? storedMode : 'paper')
  const [page, setPage] = useState<Page>(currentPage)
  const [dashboard, setDashboard] = useState<Dashboard | null>(null)
  const [regions, setRegions] = useState<RegionsPayload | null>(null)
  const [region, setRegionState] = useState<string>(() => storageGet('lnc-newsman-region') ?? '')
  const [error, setError] = useState('')
  const setLang = (value: Lang) => { setLangState(value); storageSet('lnc-newsman-language', value) }
  const setMode = (value: ReaderMode) => { setModeState(value); storageSet('lnc-newsman-reader-mode', value) }
  const setRegion = (value: string) => { setRegionState(value); storageSet('lnc-newsman-region', value) }
  // Dashboard counters follow the same region as the article list, so a card can
  // never advertise items the reader is hiding.
  const refreshDashboard = (forRegion: string) => { api<Dashboard>(forRegion ? `/api/dashboard?region=${encodeURIComponent(forRegion)}` : '/api/dashboard').then(setDashboard).catch(() => setError('load')) }
  const refreshRegions = () => { api<RegionsPayload>('/api/admin/regions').then(setRegions).catch(() => setError('load')) }

  useEffect(() => { document.documentElement.lang = lang }, [lang])
  // The reading mode lives on <html data-mode>, so it covers the page behind the
  // shell and the overscroll area too; the status-bar colour follows it.
  useEffect(() => { applyMode(mode) }, [mode])
  // On a phone the keyboard opens as a visual-viewport change; the fixed tab bar
  // would sit on top of the field being typed into, so hide it while typing.
  useEffect(() => {
    const vv = window.visualViewport
    if (!vv) return
    const sync = () => document.body.classList.toggle('keyboard-open', window.innerHeight - vv.height > 140)
    vv.addEventListener('resize', sync)
    return () => vv.removeEventListener('resize', sync)
  }, [])
  useEffect(() => {
    api<{ authenticated: boolean; management_configured: boolean }>('/api/auth/status')
      .then((status) => { setAuthenticated(status.authenticated); setManagementConfigured(status.management_configured) })
      .catch(() => { setAuthenticated(false); setError('offline') })
    const expired = () => { setDashboard(null); setAuthenticated(false); history.replaceState(null, '', '/login.html') }
    const hashChanged = () => setPage(currentPage())
    window.addEventListener('newsman:session-expired', expired)
    window.addEventListener('hashchange', hashChanged)
    return () => { window.removeEventListener('newsman:session-expired', expired); window.removeEventListener('hashchange', hashChanged) }
  }, [])
  useEffect(() => {
    if (!authenticated) return
    refreshRegions()
    api<Settings>('/api/settings').then((settings) => {
      if (!validLang(storageGet('lnc-newsman-language')) && validLang(settings.language)) setLangState(settings.language)
      if (!isMode(storageGet('lnc-newsman-reader-mode')) && isMode(settings.default_reader_mode)) setModeState(settings.default_reader_mode)
    }).catch(() => setError('load'))
  }, [authenticated])

  const activeRegion = region && regions?.data.some((item) => item.code === region && item.enabled) ? region : ''
  useEffect(() => {
    if (!authenticated || !regions) return
    refreshDashboard(activeRegion)
    // activeRegion is derived from the stored preference and the region list; the
    // other dependencies are stable callbacks.
  }, [authenticated, regions, activeRegion])
  if (authenticated === null) return <div className="loading-screen"><img src="/favicon.svg" alt="" /><span>Loading Newsman…</span></div>
  if (!authenticated) return <Login lang={lang} setLang={setLang} mode={mode} configured={managementConfigured} offline={error === 'offline'} />
  return <Shell lang={lang} setLang={setLang} mode={mode} setMode={setMode} page={page} setPage={setPage} dashboard={dashboard} regions={regions} region={activeRegion} setRegion={setRegion} onRegionsChanged={refreshRegions} error={error} setError={setError} onCatalogChanged={() => refreshDashboard(activeRegion)} onSignOut={() => { setDashboard(null); setAuthenticated(false); history.replaceState(null, '', '/login.html') }} />
}

function Login({ lang, setLang, mode, configured, offline }: { lang: Lang; setLang: (v: Lang) => void; mode: ReaderMode; configured: boolean; offline: boolean }) {
  const t = copy[lang]
  const [key, setKey] = useState('')
  const [error, setError] = useState('')
  const [pending, setPending] = useState(false)
  const submit = async (event: FormEvent) => {
    event.preventDefault(); setPending(true); setError('')
    try { await api('/api/auth/login', { method: 'POST', body: JSON.stringify({ key }) }); setKey(''); window.location.assign('/admin') }
    catch (err) { setError(err instanceof ApiError && err.status === 429 ? t.limited : t.invalid) }
    finally { setPending(false) }
  }
  return <main className="login-page"><section className="login-card">
    <div className="login-brand"><img src="/favicon.svg" alt="" /><div><strong>LNC Newsman</strong><span>{t.subtitle}</span></div></div>
    <div className="login-copy"><p className="eyebrow">{t.signIn}</p><h1>{t.welcome}</h1><p>{offline ? t.loadError : configured ? t.keyHint : t.missing}</p></div>
    <form onSubmit={submit} className="login-form"><label htmlFor="management-key">{t.key}</label><input id="management-key" type="password" autoComplete="current-password" autoFocus value={key} onChange={(e) => setKey(e.target.value)} placeholder="••••••••••••" /><button className="btn primary" disabled={!key || pending || !configured}>{pending ? t.checking : t.enter}</button>{error && <div className="form-error" role="alert">{error}</div>}</form>
    <div className="login-footer"><span>Private internal workspace</span><button type="button" className="language-link" onClick={() => setLang(lang === 'vi' ? 'en' : 'vi')}>{lang === 'vi' ? 'English' : 'Tiếng Việt'}</button></div>
  </section></main>
}

const icons: Record<Page, string> = { today: '⌂', topics: '◌', regions: '⌖', market: '◭', stories: '▤', reports: '◫', agents: '✦', alerts: '◈', timeline: '⌁', saved: '♡', settings: '⚙' }

function Shell({ lang, setLang, mode, setMode, page, setPage, dashboard, regions, region, setRegion, onRegionsChanged, error, setError, onCatalogChanged, onSignOut }: { lang: Lang; setLang: (v: Lang) => void; mode: ReaderMode; setMode: (v: ReaderMode) => void; page: Page; setPage: (v: Page) => void; dashboard: Dashboard | null; regions: RegionsPayload | null; region: string; setRegion: (v: string) => void; onRegionsChanged: () => void; error: string; setError: (v: string) => void; onCatalogChanged: () => void; onSignOut: () => void }) {
  const t = copy[lang]
  // The browse panel narrows the reader: a topic from the tree, or one enabled source.
  // Both are plain filters on /api/articles, so nothing here needs new server state.
  const [filter, setFilter] = useState<Filter>({ topic: null, source: null })
  const [railFilter, setRailFilter] = useState('')
  const [readingId, setReadingId] = useState<number | null>(null)
  const cycleMode = () => setMode(THEMES[(THEMES.findIndex((theme) => theme.id === mode) + 1) % THEMES.length].id)
  const signOut = async () => { try { await api('/api/auth/logout', { method: 'POST' }); onSignOut() } catch { setError('logout') } }
  const openFilter = (next: Filter) => { setFilter(next); if (page !== 'topics') setRoute('topics') }
  return <div className={`app-shell page-${page}`}>
    <aside className="sidebar">
      <div className="brand"><img src="/favicon.svg" alt="" /><div><strong>LNC Newsman</strong><span>INTELLIGENCE READER</span></div></div>
      <div className="app-switch" aria-label="Chuyển ứng dụng / Switch app">{appLinks('newsman', isLocal()).map((link) => <a key={link.id} className={link.id === 'newsman' ? 'app-switch-link active' : 'app-switch-link'} href={link.href} aria-current={link.id === 'newsman' ? 'page' : undefined}><span aria-hidden="true">{link.icon}</span>{link.label}</a>)}</div>
      <nav className="nav" aria-label="Primary navigation">{ROUTABLE_PAGES.map((id) => <button key={id} aria-label={t[id]} aria-current={page === id ? 'page' : undefined} className={page === id ? 'nav-item active' : 'nav-item'} onClick={() => setRoute(id)}><span className="nav-icon" aria-hidden="true">{icons[id]}</span><span>{t[id]}</span></button>)}</nav>
      <BrowseSidebar t={t} lang={lang} dashboard={dashboard} regions={regions} region={region} filter={filter} query={railFilter} setQuery={setRailFilter} onPick={openFilter} />
      <div className="side-divider" />
      <div className="sidebar-footer">
        <div className="version">v{__APP_VERSION__} <span>· Reader + MCP</span></div>
        <button className="footer-action" onClick={cycleMode} aria-label={`${t.changeTheme}: ${themeName(mode, lang)}`}><span aria-hidden="true">{THEMES.find((theme) => theme.id === mode)?.dark ? '☾' : '☼'}</span>{themeName(mode, lang)}</button>
        <button className="footer-action signout" onClick={signOut}><span aria-hidden="true">↪</span>{t.signOut}</button>
      </div>
    </aside>
    <main className="page-shell">
      <header className="page-header">
        <div><div className="breadcrumb">LNC / NEWSMAN</div><h1>{t[page]}</h1></div>
        <div className="header-actions">
          <div className="region-picker">{regions && <Select label={t.region} value={region} options={[{ value: '', label: t.allRegions }, ...regions.data.filter((item) => item.enabled).map((item) => ({ value: item.code, label: lang === 'vi' ? item.label_vi : item.label_en, hint: item.code }))]} onChange={setRegion} />}</div>
          <button className="language-toggle" aria-label={t.changeLanguage} onClick={() => setLang(lang === 'vi' ? 'en' : 'vi')}>{lang.toUpperCase()}</button>
          <button className="icon-button" aria-label={t.refresh} onClick={() => location.reload()}>↻</button>
        </div>
      </header>
      {regions && <div className="mobile-region-bar"><Select label={t.region} value={region} options={[{ value: '', label: t.allRegions }, ...regions.data.filter((item) => item.enabled).map((item) => ({ value: item.code, label: lang === 'vi' ? item.label_vi : item.label_en, hint: item.code }))]} onChange={setRegion} /></div>}
      {error && <div className="alert error" role="alert"><strong>{error === 'logout' ? t.failedLogout : t.loadError}</strong><button onClick={() => setError('')}>{t.dismiss}</button></div>}
      <div className="page-content">
        {page === 'today' && <Today t={t} lang={lang} dashboard={dashboard} region={region} setPage={setRoute} onPick={openFilter} onOpenArticle={setReadingId} />}
        {page === 'topics' && <Topics t={t} lang={lang} dashboard={dashboard} region={region} filter={filter} setFilter={setFilter} onOpenArticle={setReadingId} />}
        {page === 'regions' && <RegionsPage t={t} lang={lang} region={region} onOpenArticle={setReadingId} />}
        {page === 'market' && <Market t={t} lang={lang} region={region} />}
        {page === 'reports' && <ReportsPage t={t} lang={lang} region={region} dashboard={dashboard} onOpenArticle={setReadingId} onOpenAgents={() => setRoute('agents')} />}
        {page === 'agents' && <AgentsWorkspace lang={lang} onOpenArticle={setReadingId} onOpenRegions={() => setRoute('regions')} />}
        {page === 'alerts' && <AlertsPage t={t} lang={lang} />}
        {page === 'settings' && <SettingsPage t={t} lang={lang} mode={mode} setMode={setMode} setLang={setLang} onCatalogChanged={onCatalogChanged} onRegionsChanged={onRegionsChanged} onOpenAgentsModels={() => { try { sessionStorage.setItem('lnc-newsman-agents-surface', 'settings') } catch { /* ignore */ } setRoute('agents') }} regions={regions} dashboard={dashboard} />}
        {!ROUTABLE_PAGES.includes(page) && <EmptyPage t={t} />}
      </div>
    </main>
    <nav className="tabbar" aria-label={t.tabbarLabel}>{ROUTABLE_PAGES.filter((id) => id !== 'market' && id !== 'regions').map((id) => <button key={id} type="button" className={page === id ? 'tab active' : 'tab'} aria-current={page === id ? 'page' : undefined} onClick={() => setRoute(id)}><span className="tab-icon" aria-hidden="true">{icons[id]}</span><span>{t[id]}</span></button>)}</nav>
    {readingId !== null && <ArticleReaderModal t={t} lang={lang} articleId={readingId} topics={dashboard?.topics} onClose={() => setReadingId(null)} onUpdated={onCatalogChanged} />}
  </div>
  function setRoute(next: Page) { location.hash = next; setPage(next) }
}

function themeName(mode: ReaderMode, lang: Lang): string {
  const spec = THEMES.find((theme) => theme.id === mode) ?? THEMES[0]
  return lang === 'vi' ? spec.vi : spec.en
}

/**
 * Desktop browse list (W1 of the shared UI plan): the same filter the reader uses,
 * expressed as counts. Phones get the tab bar instead and never see this panel.
 */
function BrowseSidebar({ t, lang, dashboard, regions, region, filter, query, setQuery, onPick }: { t: Copy; lang: Lang; dashboard: Dashboard | null; regions: RegionsPayload | null; region: string; filter: Filter; query: string; setQuery: (v: string) => void; onPick: (v: Filter) => void }) {
  const topics = dashboard?.topics ?? []
  const sources = dashboard?.sources ?? []
  const counts = dashboard?.stats.topic_counts ?? {}
  const needle = query.trim().toLowerCase()
  const matches = (label: string) => !needle || label.toLowerCase().includes(needle)
  const topicLabel = (topic: Topic) => lang === 'vi' ? topic.label_vi : topic.label_en
  const total = (slug: string, seen = new Set<string>()): number => {
    if (seen.has(slug)) return 0
    seen.add(slug)
    return (counts[slug] ?? 0) + topics.filter((item) => item.parent_slug === slug).reduce((sum, child) => sum + total(child.slug, seen), 0)
  }
  const shownTopics = topics.filter((topic) => matches(topicLabel(topic)))
  const shownSources = sources.filter((source) => matches(source.name))
  const shownRegions = (regions?.data ?? []).filter((item) => item.enabled && matches(lang === 'vi' ? item.label_vi : item.label_en))
  return <div className="side-shelves">
    <label className="side-filter field"><span className="field-label">{t.quickFilter}</span><input type="search" value={query} placeholder={t.filterPlaceholder} onChange={(event) => setQuery(event.target.value)} /></label>
    <button type="button" className={filter.topic === null && filter.source === null ? 'side-all active' : 'side-all'} onClick={() => onPick({ topic: null, source: null })}><span>{t.browseAll}</span><em>{dashboard?.stats.articles_total ?? '—'}</em></button>
    <div className="side-group">
      <div className="side-group-head" role="presentation"><span>{t.byRegion}</span><i>{lang === 'vi' ? 'đang bật' : 'on'}</i></div>
      <div className="side-items">
        {shownRegions.length === 0 && <p className="index-empty">{t.noMatch}</p>}
        {shownRegions.map((item) => <button key={item.code} type="button" className={region === item.code ? 'side-item active' : 'side-item'} onClick={() => onPick({ topic: null, source: null })} title={item.code}><span>{lang === 'vi' ? item.label_vi : item.label_en}</span><em>{item.source_count}</em></button>)}
      </div>
    </div>
    <div className="side-group">
      <div className="side-group-head" role="presentation"><span>{t.byTopic}</span><i>{topics.length}</i></div>
      <div className="side-items">
        {shownTopics.length === 0 && <p className="index-empty">{t.noMatch}</p>}
        {shownTopics.map((topic) => <button key={topic.slug} type="button" className={filter.topic === topic.slug ? 'side-item active' : 'side-item'} onClick={() => onPick({ topic: topic.slug, source: null })} title={topic.slug}><span>{topicLabel(topic)}</span><em>{total(topic.slug)}</em></button>)}
      </div>
    </div>
    <div className="side-group">
      <div className="side-group-head" role="presentation"><span>{t.bySource}</span><i>{sources.length}</i></div>
      <div className="side-items">
        {shownSources.length === 0 && <p className="index-empty">{t.noMatch}</p>}
        {shownSources.slice(0, 40).map((source) => <button key={source.slug} type="button" className={filter.source === source.slug ? 'side-item active' : 'side-item'} onClick={() => onPick({ topic: null, source: source.slug })} title={`${source.slug} · ${source.region}`}><span>{source.name}</span><em>{source.articles}</em></button>)}
      </div>
    </div>
  </div>
}

function formatDate(lang: Lang, value: string | null) {
  if (!value) return null
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return null
  return new Intl.DateTimeFormat(lang === 'vi' ? 'vi-VN' : 'en-GB', { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' }).format(date)
}
function topicLabel(lang: Lang, topics: Topic[] | undefined, slug: string | null) {
  const topic = topics?.find((item) => item.slug === slug)
  return topic ? (lang === 'vi' ? topic.label_vi : topic.label_en) : null
}

/**
 * Load the next page when a sentinel below the list scrolls into view.
 *
 * Two rules that matter more than the mechanism:
 * - **One attempt per rendered page.** The observer disconnects when it fires, and the
 *   effect rebuilds it only after the list grew, `hasMore` changed or a load finished.
 *   A sentinel still on screen after a *failed* load (the list did not grow) is not
 *   retried, so a broken network does not spin.
 * - **A failure falls back to the button.** `failed` is set when a load added nothing;
 *   the caller then shows the old "Load more" control instead of an invisible loop.
 *
 * Kept as a hook so every paginated list (articles, documents, alerts) behaves the same,
 * and so the fallback is one implementation rather than three.
 */
function useInfiniteScroll({ hasMore, busy, count, load }: { hasMore: boolean; busy: boolean; count: number; load: () => Promise<boolean> }) {
  const sentinel = useRef<HTMLDivElement | null>(null)
  const [failed, setFailed] = useState(false)
  // The gating flag is a ref, not state: the observer callback would otherwise close
  // over a stale `busy` and fire twice for one page.
  const inFlight = useRef(false)
  useEffect(() => { setFailed(false) }, [count === 0])
  useEffect(() => {
    if (!hasMore || busy || failed) return
    const node = sentinel.current
    if (!node || typeof IntersectionObserver === 'undefined') return
    const observer = new IntersectionObserver((entries) => {
      if (!entries.some((entry) => entry.isIntersecting) || inFlight.current) return
      // Disconnect first: the callback must run the load exactly once, even if the
      // sentinel stays in view while the request is in flight.
      observer.disconnect()
      inFlight.current = true
      void load().then((grew) => {
        inFlight.current = false
        // Nothing new means the page is exhausted or the request failed; either way
        // retrying automatically would loop, so hand control back to the button.
        if (!grew) setFailed(true)
      })
    }, { rootMargin: '600px 0px' })
    observer.observe(node)
    return () => observer.disconnect()
  }, [hasMore, busy, count, failed, load])
  return { sentinel, failed }
}

function useArticles(topic: string | null, region: string, source: string | null, query: string, pageSize = 30) {
  const [items, setItems] = useState<Article[] | null>(null)
  const [error, setError] = useState(false)
  const [more, setMore] = useState(false)
  const [busy, setBusy] = useState(false)
  const load = async (offset: number): Promise<boolean> => {
    setBusy(true); setError(false)
    try {
      const params = new URLSearchParams({ limit: String(pageSize), offset: String(offset) })
      if (topic) params.set('topic', topic)
      if (source) params.set('source', source)
      if (region) params.set('region', region)
      if (query.trim()) params.set('q', query.trim())
      const result = await api<{ data: Article[] }>(`/api/articles?${params}`)
      setItems((current) => offset === 0 || !current ? result.data : [...current, ...result.data])
      setMore(result.data.length === pageSize)
      return result.data.length > 0
    } catch { setError(true); return false } finally { setBusy(false) }
  }
  useEffect(() => { setItems(null); void load(0) }, [topic, region, source, query])
  return { items, error, more, busy, load }
}
function ArticleList({ t, lang, topic, topics, region, source, query, limit, onOpenArticle }: { t: Copy; lang: Lang; topic: string | null; topics?: Topic[]; region?: string; source?: string | null; query?: string; limit?: number; onOpenArticle?: (id: number) => void }) {
  const { items, error, more, busy, load } = useArticles(topic, region ?? '', source ?? null, query ?? '', limit)
  // A capped list (a home rail) has nothing more to load, so it never gets a sentinel.
  const paged = !limit
  const { sentinel, failed } = useInfiniteScroll({
    hasMore: paged && more,
    busy,
    count: items?.length ?? 0,
    load: () => load(items?.length ?? 0),
  })
  if (error && !items) return <p className="form-error" role="alert">{t.loadError}</p>
  if (!items) return <p className="muted" role="status">{t.loading}</p>
  if (items.length === 0) return <div className="empty-card"><div className="empty-art">✦</div><h3>{query?.trim() ? t.noMatch : t.noStories}</h3><p>{query?.trim() ? t.searchArticlesPlaceholder : t.noStoriesSub}</p></div>
  return <div><div className="article-list">{items.map((item) => {
    const published = formatDate(lang, item.published_at)
    const label = topicLabel(lang, topics, item.topic_slug)
    const extraTopics = (item.labels?.topics ?? []).filter((slug) => slug !== item.topic_slug)
    const entities = item.labels?.entities ?? []
    const hasFull = item.content_source === 'web' || (item.content_source === 'feed' && (item.word_count ?? 0) > 0)
    return <article className="article-row" key={item.id}>
      <div className="article-row-top">
        {onOpenArticle
          ? <button type="button" className="article-open-title" onClick={() => onOpenArticle(item.id)}>{item.title}</button>
          : <a href={item.url} target="_blank" rel="noopener noreferrer">{item.title}</a>}
        <div className="article-row-actions">
          {onOpenArticle && <button type="button" className="reader-pill" onClick={() => onOpenArticle(item.id)}>{t.readInApp}</button>}
          <a className="ext-pill" href={item.url} target="_blank" rel="noopener noreferrer">{t.openOriginal}</a>
        </div>
      </div>
      {item.summary && <p className="article-excerpt" onClick={() => onOpenArticle?.(item.id)}>{item.summary}</p>}
      <div className="article-meta"><span className="tier-badge">{item.source_tier}</span><span>{item.source_name}</span>{hasFull && <span className="content-badge">{item.content_source === 'web' ? t.contentWeb : t.contentFeed}{item.word_count ? ` · ${item.word_count} ${t.wordsUnit}` : ''}</span>}{label && <span>· {label}</span>}{extraTopics.map((slug) => <span key={slug} className="label-chip">{topicLabel(lang, topics, slug) ?? slug}</span>)}{entities.length > 0 && <span className="label-chips" aria-label={t.entities}>{entities.slice(0, 5).map((value) => <span key={value} className="label-chip entity">{value}</span>)}</span>}<span>· {published ?? `${t.fetchedAt} ${formatDate(lang, item.fetched_at) ?? ''}`}</span></div>
    </article>
  })}</div>{error && <p className="form-error" role="alert">{t.loadError}</p>}
    {paged && more && !failed && <div ref={sentinel} className="scroll-sentinel" aria-hidden="true">{busy ? t.loading : ''}</div>}
    {paged && more && (failed || typeof IntersectionObserver === 'undefined') && <button className="btn secondary load-more" disabled={busy} onClick={() => void load(items.length)}>{busy ? t.loading : failed ? t.loadFailed : t.loadMore}</button>}
  </div>
}

/**
 * A home rail: a heading, an optional "see all" and a responsive card grid.
 */
function Rail({ title, action, children }: { title: string; action?: React.ReactNode; children: React.ReactNode }) {
  return <section className="rail"><div className="rail-head"><h2 className="section-title">{title}</h2>{action}</div><div className="rail-strip">{children}</div></section>
}

function MiniArticle({ t, lang, item, topics, onOpen }: { t: Copy; lang: Lang; item: Article; topics?: Topic[]; onOpen: (id: number) => void }) {
  const hasFull = item.content_source === 'web' || (item.content_source === 'feed' && (item.word_count ?? 0) > 0)
  return <article className="mini-card">
    <div className="mini-head"><span className="mini-tier">{item.source_tier}</span>{hasFull && <span className="content-badge">{item.content_source === 'web' ? t.contentWeb : t.contentFeed}</span>}</div>
    <button type="button" className="mini-open-title" onClick={() => onOpen(item.id)}>{item.title}</button>
    {item.summary && <p className="mini-excerpt" onClick={() => onOpen(item.id)}>{item.summary}</p>}
    <div className="article-meta"><span>{item.source_name}</span>{topicLabel(lang, topics, item.topic_slug) && <span>· {topicLabel(lang, topics, item.topic_slug)}</span>}<span>· {formatDate(lang, item.published_at) ?? formatDate(lang, item.fetched_at) ?? t_none(lang)}</span><a className="mini-ext" href={item.url} target="_blank" rel="noopener noreferrer">{t.openOriginal}</a></div>
  </article>
}
function t_none(lang: Lang) { return lang === 'vi' ? 'không rõ ngày' : 'no date' }

function HomeRails({ t, lang, dashboard, region, onPick, onOpenArticle }: { t: Copy; lang: Lang; dashboard: Dashboard | null; region: string; onPick: (v: Filter) => void; onOpenArticle: (id: number) => void }) {
  const topics = dashboard?.topics ?? []
  const sources = dashboard?.sources ?? []
  const [latest, setLatest] = useState<Article[] | null>(null)
  const [watched, setWatched] = useState<Article[] | null>(null)
  const [alerts, setAlerts] = useState<AlertDelivery[] | null>(null)
  const [briefs, setBriefs] = useState<AgentAnalysis[] | null>(null)
  useEffect(() => {
    const suffix = region ? `&region=${encodeURIComponent(region)}` : ''
    api<{ data: Article[] }>(`/api/articles?limit=6${suffix}`).then((payload) => setLatest(payload.data)).catch(() => setLatest([]))
    api<{ data: AlertDelivery[] }>('/api/alerts?limit=6').then((payload) => setAlerts(payload.data)).catch(() => setAlerts([]))
    api<{ data: AgentAnalysis[] }>(`/api/v1/analyses?limit=6${suffix}`).then((payload) => setBriefs(payload.data)).catch(() => setBriefs([]))
  }, [region])
  useEffect(() => {
    if (!sources.length) { setWatched([]); return }
    const top = sources[0]
    const suffix = region ? `&region=${encodeURIComponent(region)}` : ''
    api<{ data: Article[] }>(`/api/articles?limit=6&source=${encodeURIComponent(top.slug)}${suffix}`).then((payload) => setWatched(payload.data)).catch(() => setWatched([]))
  }, [sources, region])
  return <>
    <Rail title={t.railBriefs} action={<button type="button" className="text-button" onClick={() => { location.hash = 'reports' }}>{t.openReports} →</button>}>
      {!briefs ? <p className="muted">{t.loading}</p> : briefs.length === 0 ? <p className="rail-empty">{t.noBriefsRail}</p> : briefs.map((item) => <article className="mini-card" key={item.id}><span className="mini-tier">{item.stable_key ?? `analysis-${item.id}`}</span><a href="#reports">{item.title}</a><div className="article-meta"><span>{item.author ?? t.mcpOwner}</span>{item.region_code && <span>· {item.region_code}</span>}<span>· {formatDate(lang, new Date(item.updated_at * 1000).toISOString())}</span></div></article>)}
    </Rail>
    <Rail title={t.railLatest} action={<button type="button" className="text-button" onClick={() => onPick({ topic: null, source: null })}>{t.seeAll}</button>}>
      {!latest ? <p className="muted">{t.loading}</p> : latest.length === 0 ? <div className="empty-card compact"><div className="empty-art">✦</div><h3>{t.noStories}</h3><p>{t.noStoriesSub}</p></div> : latest.map((item) => <MiniArticle key={item.id} t={t} lang={lang} item={item} topics={topics} onOpen={onOpenArticle} />)}
    </Rail>
    <Rail title={t.railAlerts} action={<button type="button" className="text-button" onClick={() => { location.hash = 'alerts' }}>{t.seeAll}</button>}>
      {!alerts ? <p className="muted">{t.loading}</p> : alerts.length === 0 ? <p className="rail-empty">{t.alertsEmpty} — {t.alertsEmptySub}</p> : alerts.map((item) => <article className="mini-card" key={item.id}><span className={`alert-level ${item.level}`}>{item.level}</span><button type="button" className="mini-open-title" onClick={() => onOpenArticle(item.article_id)}>{item.article_title}</button><div className="article-meta"><span>{lang === 'vi' ? item.rule_label_vi : item.rule_label_en}</span><span>· {formatDate(lang, new Date(item.created_at * 1000).toISOString())}</span></div></article>)}
    </Rail>
    <Rail title={t.railWatch} action={sources.length > 0 ? <button type="button" className="text-button" onClick={() => onPick({ topic: null, source: sources[0].slug })}>{sources[0].name}</button> : undefined}>
      {!watched ? <p className="muted">{t.loading}</p> : watched.length === 0 ? <p className="rail-empty">{t.emptyRail}</p> : watched.map((item) => <MiniArticle key={item.id} t={t} lang={lang} item={item} topics={topics} onOpen={onOpenArticle} />)}
    </Rail>
  </>
}

function Today({ t, lang, dashboard, region, setPage, onPick, onOpenArticle }: { t: Copy; lang: Lang; dashboard: Dashboard | null; region: string; setPage: (v: Page) => void; onPick: (v: Filter) => void; onOpenArticle: (id: number) => void }) {
  const stats = dashboard?.stats
  const [quickSearch, setQuickSearch] = useState('')
  const onSearch = (event: FormEvent) => {
    event.preventDefault()
    if (!quickSearch.trim()) return
    onPick({ topic: null, source: null, q: quickSearch.trim() })
  }
  return <div className="content-stack">
    <section className="welcome-grid"><div className="welcome-card"><div className="eyebrow">{new Intl.DateTimeFormat(lang === 'vi' ? 'vi-VN' : 'en-US', { weekday: 'long', day: 'numeric', month: 'long' }).format(new Date())}</div><h2>{t.briefing}</h2><p>{t.briefingSub}</p>
      <form className="quick-search-bar" onSubmit={onSearch}>
        <input type="search" value={quickSearch} placeholder={t.searchArticlesPlaceholder} onChange={(e) => setQuickSearch(e.target.value)} aria-label={t.searchArticles} />
        <button className="btn secondary" disabled={!quickSearch.trim()}>{t.searchGo}</button>
      </form>
      <div className="welcome-actions"><button className="btn primary" onClick={() => setPage('topics')}>{t.topics} <span>→</span></button><span className="quiet-note">{stats ? (stats.collector_enabled ? t.collectorOn : t.collectorOff) : ''}</span></div></div><div className="signal-card"><span className="signal-ring">◎</span><div><strong>{stats ? stats.articles_24h : '—'}</strong><span>{t.articles24h}</span></div><div className="signal-line" /><span className="muted">{stats ? `${stats.articles_with_content ?? 0}/${stats.articles_total} ${t.articlesArchived.toLowerCase()} · ${stats.sources_enabled} ${t.sourcesEnabled.toLowerCase()}` : '—'}</span></div></section>
    <HomeRails t={t} lang={lang} dashboard={dashboard} region={region} onPick={onPick} onOpenArticle={onOpenArticle} />
  </div>
}

function Topics({ t, lang, dashboard, region, filter, setFilter, onOpenArticle }: { t: Copy; lang: Lang; dashboard: Dashboard | null; region: string; filter: Filter; setFilter: (v: Filter) => void; onOpenArticle: (id: number) => void }) {
  const topics = dashboard?.topics ?? []
  const sources = dashboard?.sources ?? []
  const [searchInput, setSearchInput] = useState(filter.q ?? '')
  useEffect(() => { setSearchInput(filter.q ?? '') }, [filter.q])
  useEffect(() => {
    const timer = window.setTimeout(() => {
      if ((filter.q ?? '') !== searchInput.trim()) setFilter({ ...filter, q: searchInput.trim() || undefined })
    }, 260)
    return () => window.clearTimeout(timer)
  }, [searchInput])
  const parentTrail = (topic: Topic): string => {
    const ancestors: string[] = []
    const seen = new Set([topic.slug])
    let parentSlug = topic.parent_slug
    while (parentSlug) {
      const parent: Topic | undefined = topics.find((item) => item.slug === parentSlug)
      if (!parent || seen.has(parent.slug)) break
      seen.add(parent.slug)
      ancestors.unshift(lang === 'vi' ? parent.label_vi : parent.label_en)
      parentSlug = parent.parent_slug
    }
    return ancestors.join(' / ')
  }
  const total = (slug: string, seen = new Set<string>()): number => {
    if (seen.has(slug)) return 0
    seen.add(slug)
    return (dashboard?.stats.topic_counts[slug] ?? 0) + topics.filter((item) => item.parent_slug === slug).reduce((sum, child) => sum + total(child.slug, seen), 0)
  }
  if (!dashboard) return <p className="muted">{t.loadError}</p>
  const activeSource = sources.find((source) => source.slug === filter.source)
  const chipLabel = filter.source ? activeSource?.name ?? filter.source : filter.topic ? topicLabel(lang, topics, filter.topic) ?? filter.topic : null
  return <div className="content-stack">
    <div className="chip-row">
      <button type="button" className={filter.topic === null && filter.source === null && !filter.q ? 'chip active' : 'chip'} onClick={() => { setSearchInput(''); setFilter({ topic: null, source: null }) }}>{t.allTopics}</button>
      {chipLabel && <button type="button" className="chip active filter-chip" onClick={() => setFilter({ topic: null, source: null, q: filter.q })}>{t.showing}: {chipLabel}<span aria-hidden="true"> ×</span></button>}
      {filter.q && <button type="button" className="chip active filter-chip" onClick={() => { setSearchInput(''); setFilter({ ...filter, q: undefined }) }}>“{filter.q}”<span aria-hidden="true"> ×</span></button>}
    </div>
    <label className="field article-search-field"><span>{t.searchArticles}</span><input type="search" value={searchInput} placeholder={t.searchArticlesPlaceholder} onChange={(e) => setSearchInput(e.target.value)} /></label>
    {!filter.source && !filter.q && <div className="topic-grid">{topics.map((topic) => <button type="button" className="topic-card" key={topic.slug} aria-pressed={filter.topic === topic.slug} onClick={() => setFilter({ topic: filter.topic === topic.slug ? null : topic.slug, source: null, q: filter.q })}><span className="topic-dot" />{parentTrail(topic) && <p>{parentTrail(topic)} →</p>}<h3>{lang === 'vi' ? topic.label_vi : topic.label_en}</h3><p>{lang === 'vi' ? topic.label_en : topic.label_vi}</p><span className="topic-count">{total(topic.slug)} {t.topicArticles}</span></button>)}</div>}
    <section className="section-heading"><div><span className="eyebrow">{t.stories}</span><h2>{filter.q ? `${t.searchArticles}: “${filter.q}”` : filter.source ? activeSource?.name ?? t.bySource : filter.topic ? topicLabel(lang, topics, filter.topic) : t.latest}</h2></div></section>
    <ArticleList t={t} lang={lang} topic={filter.topic} topics={topics} region={region} source={filter.source} query={filter.q ?? ''} onOpenArticle={onOpenArticle} />
  </div>
}

function AlertsPage({ t, lang }: { t: Copy; lang: Lang }) {
  const [items, setItems] = useState<AlertDelivery[] | null>(null)
  const [unread, setUnread] = useState(0)
  const [error, setError] = useState(false)
  const [busy, setBusy] = useState(false)
  const [notice, setNotice] = useState('')
  const load = async () => {
    try {
      const payload = await api<{ data: AlertDelivery[]; unread: number }>('/api/alerts?limit=100')
      setItems(payload.data); setUnread(payload.unread); setError(false)
    } catch { setError(true) }
  }
  useEffect(() => { void load() }, [])
  const markRead = async (id: number) => {
    try { await api(`/api/alerts/${id}/read`, { method: 'POST' }); await load() } catch { setError(true) }
  }
  const markAll = async () => {
    setBusy(true)
    try { await api('/api/alerts/read-all', { method: 'POST' }); await load() } catch { setError(true) } finally { setBusy(false) }
  }
  const sweep = async () => {
    setBusy(true); setNotice('')
    try {
      const result = await api<{ created: number }>('/api/admin/alerts/sweep', { method: 'POST' })
      setNotice(t.alertsSwept(result.created)); await load()
    } catch { setError(true) } finally { setBusy(false) }
  }
  if (error && !items) return <p className="form-error" role="alert">{t.loadError}</p>
  if (!items) return <p className="muted" role="status">{t.loading}</p>
  return <div className="content-stack">
    <section className="section-heading"><div><span className="eyebrow">{t.alertsTitle}</span><h2>{unread > 0 ? `${unread} ${t.alertsUnread}` : t.alertsTitle}</h2><p>{t.alertsSub}</p></div>
      <div className="btn-row section-actions"><button type="button" className="btn secondary" disabled={busy} onClick={() => void sweep()}>{busy ? t.loading : t.alertsSweep}</button>
        <button type="button" className="btn secondary" disabled={busy || unread === 0} onClick={() => void markAll()}>{t.markAllRead}</button></div></section>
    {notice && <p className="muted" role="status">{notice}</p>}
    {error && <p className="form-error" role="alert">{t.loadError}</p>}
    {items.length === 0
      ? <div className="empty-card"><div className="empty-art">◈</div><h3>{t.alertsEmpty}</h3><p>{t.alertsEmptySub}</p></div>
      : <div className="alert-list">{items.map((item) => <article className={item.read ? 'alert-row read' : 'alert-row'} key={item.id}>
          <div className="alert-main">
            <span className={`alert-level ${item.level}`}>{item.level}</span>
            <a href={item.article_url} target="_blank" rel="noopener noreferrer">{item.article_title}</a>
            <div className="article-meta"><span>{lang === 'vi' ? item.rule_label_vi : item.rule_label_en}</span><span>· {formatDate(lang, new Date(item.created_at * 1000).toISOString())}</span><span>· {item.channel}</span></div>
          </div>
          {!item.read && <button type="button" className="btn secondary" onClick={() => void markRead(item.id)}>{t.markRead}</button>}
        </article>)}</div>}
  </div>
}
function AlertRulesManager({ t, lang }: { t: Copy; lang: Lang }) {
  const [rules, setRules] = useState<AlertRule[] | null>(null)
  const [error, setError] = useState(false)
  const [busy, setBusy] = useState('')
  const load = async () => {
    try { const payload = await api<{ data: AlertRule[] }>('/api/admin/alerts/rules'); setRules(payload.data); setError(false) }
    catch { setError(true) }
  }
  useEffect(() => { void load() }, [])
  const toggle = async (rule: AlertRule) => {
    setBusy(rule.slug)
    try { await api(`/api/admin/alerts/rules/${encodeURIComponent(rule.slug)}`, { method: 'PUT', body: JSON.stringify({ enabled: !rule.enabled }) }); await load() }
    catch { setError(true) } finally { setBusy('') }
  }
  const quiet = (rule: AlertRule) => rule.quiet_from && rule.quiet_to ? `${rule.quiet_from}–${rule.quiet_to} ${rule.timezone}` : '—'
  return <section className="regions-admin" aria-labelledby="alert-rules-title">
    <div className="section-heading"><div><span className="eyebrow">{t.admin}</span><h2 id="alert-rules-title">{t.alertsRules}</h2><p>{t.alertsRulesSub}</p></div></div>
    {error && <div className="form-error" role="alert">{t.loadError}</div>}
    {!rules ? <p className="muted" role="status">{t.loading}</p> : rules.length === 0
      ? <p className="muted">{t.alertsEmptySub}</p>
      : <div className="regions-tree">{rules.map((rule) => <div className="region-row" key={rule.slug}>
          <div className="region-main"><strong>{lang === 'vi' ? rule.label_vi : rule.label_en}</strong>
            <span>{rule.slug} · {t.alertMatch}: {rule.match_kind}={rule.match_value} · {t.alertCooldown}: {rule.cooldown_seconds} · {t.alertQuiet}: {quiet(rule)} · {rule.enabled ? rule.channels : t.alertOff}</span></div>
          <button type="button" role="switch" className="switch" aria-checked={rule.enabled} aria-label={`${rule.slug}: ${rule.enabled ? t.alertOff : t.alertsTitle}`} disabled={busy !== ''} onClick={() => void toggle(rule)} />
        </div>)}</div>}
    <p className="muted">{t.alertChannelGap}</p>
  </section>
}
function EmptyPage({ t }: { t: Copy }) { return <div className="content-stack"><div className="empty-card"><div className="empty-art">✦</div><h3>{t.emptyTitle}</h3><p>{t.emptySub}</p></div></div> }
function SettingsPage({ t, lang, mode, setMode, setLang, onCatalogChanged, onRegionsChanged, onOpenAgentsModels, regions, dashboard }: { t: Copy; lang: Lang; mode: ReaderMode; setMode: (v: ReaderMode) => void; setLang: (v: Lang) => void; onCatalogChanged: () => void; onRegionsChanged: () => void; onOpenAgentsModels: () => void; regions: RegionsPayload | null; dashboard: Dashboard | null }) {
  const [settings, setSettings] = useState<Settings | null>(null)
  const [error, setError] = useState(false)
  const [saved, setSaved] = useState(false)
  const [busy, setBusy] = useState(false)
  const [backfillBusy, setBackfillBusy] = useState(false)
  const [backfillNote, setBackfillNote] = useState('')
  useEffect(() => { api<Settings>('/api/settings').then(setSettings).catch(() => setError(true)) }, [])
  const save = async () => {
    if (!settings) return
    setBusy(true); setError(false); setSaved(false)
    try {
      const result = await api<Settings>('/api/settings', {
        method: 'PATCH',
        body: JSON.stringify({
          language: settings.language,
          default_reader_mode: settings.default_reader_mode,
          focus_profile: settings.focus_profile,
          brief_morning_time: settings.brief_morning_time,
          brief_evening_time: settings.brief_evening_time,
          brief_timezone: settings.brief_timezone,
        }),
      })
      setSettings(result); setLang(result.language); setMode(result.default_reader_mode); setSaved(true)
    } catch { setError(true) } finally { setBusy(false) }
  }
  const toggleCollector = async () => {
    if (!settings) return
    setBusy(true); setError(false)
    try {
      const result = await api<Settings>('/api/settings', { method: 'PATCH', body: JSON.stringify({ collector_enabled: !settings.collector_enabled }) })
      setSettings((current) => current ? { ...current, collector_enabled: result.collector_enabled } : result); onCatalogChanged()
    } catch { setError(true) } finally { setBusy(false) }
  }
  const toggleFulltext = async () => {
    if (!settings) return
    setBusy(true); setError(false)
    try {
      const result = await api<Settings>('/api/settings', { method: 'PATCH', body: JSON.stringify({ fulltext_auto_enabled: !settings.fulltext_auto_enabled }) })
      setSettings((current) => current ? { ...current, fulltext_auto_enabled: result.fulltext_auto_enabled } : result); onCatalogChanged()
    } catch { setError(true) } finally { setBusy(false) }
  }
  const runBackfill = async () => {
    setBackfillBusy(true); setBackfillNote(''); setError(false)
    try {
      const { data } = await api<{ data: { attempted: number; saved: number } }>('/api/admin/articles/backfill?limit=10', { method: 'POST' })
      setBackfillNote(t.backfillDone(data.saved, data.attempted))
      onCatalogChanged()
    } catch { setError(true) } finally { setBackfillBusy(false) }
  }
  if (!settings) return <div role={error ? 'alert' : 'status'} className="loading-inline">{error ? t.settingsError : t.loading}</div>
  const stats = dashboard?.stats
  const themeOptions = THEMES.map((theme) => ({ value: theme.id, label: lang === 'vi' ? theme.vi : theme.en }))
  return <div className="content-stack settings-page"><section className="section-heading"><div><span className="eyebrow">{t.admin}</span><h2>{t.settingsTitle}</h2><p>{t.settingsSub}</p></div>{saved && <span className="save-state" role="status">✓ {t.savedToast}</span>}</section>
    {error && <div className="form-error" role="alert">{t.settingsError}</div>}
    <div className="settings-grid"><section className="settings-card"><div className="card-heading"><span className="card-icon">Aa</span><div><h3>{t.settingsTitle}</h3><p>{t.settingsSub}</p></div></div>
      <Select label={t.language} value={settings.language} options={[{ value: 'vi', label: 'Tiếng Việt' }, { value: 'en', label: 'English' }]} onChange={(value) => { setSaved(false); setSettings({ ...settings, language: value as Lang }) }} />
      <Select label={t.readerMode} value={settings.default_reader_mode} options={themeOptions} onChange={(value) => { setSaved(false); setSettings({ ...settings, default_reader_mode: value as ReaderMode }) }} />
      <div className="theme-grid" role="group" aria-label={t.themeLabel}>{THEMES.map((theme) => <button key={theme.id} type="button" className={mode === theme.id ? 'theme-swatch active' : 'theme-swatch'} style={{ background: theme.bg, color: theme.dark ? '#e8e0d4' : '#332f2a' }} aria-pressed={mode === theme.id} onClick={() => setMode(theme.id)}><span className="swatch-sample" aria-hidden="true">Aa</span><span className="swatch-name">{lang === 'vi' ? theme.vi : theme.en}</span></button>)}</div>
    </section><section className="settings-card"><div className="card-heading"><span className="card-icon">◷</span><div><h3>{t.nextPhase}</h3><p>{t.collectorHelp}</p></div></div>
      <div className="switch-row"><div><strong>{settings.collector_enabled ? t.collectorOn : t.collectorOff}</strong></div><button type="button" role="switch" className="switch" aria-checked={settings.collector_enabled} aria-label={t.collectorToggle} disabled={busy} onClick={() => void toggleCollector()} /></div>
      <div className="switch-row"><div><strong>{settings.fulltext_auto_enabled ? t.fulltextAutoOn : t.fulltextAutoOff}</strong><span>{t.fulltextAutoHelp}</span></div><button type="button" role="switch" className="switch" aria-checked={settings.fulltext_auto_enabled} disabled={busy} onClick={() => void toggleFulltext()} /></div>
      {stats && <dl className="stat-pairs"><div><dt>{t.sourcesEnabled}</dt><dd>{stats.sources_enabled}</dd></div><div><dt>{t.sourcesPaused}</dt><dd>{stats.sources_paused}</dd></div><div><dt>{t.articles24h}</dt><dd>{stats.articles_24h}</dd></div><div><dt>{t.articlesTotal}</dt><dd>{stats.articles_total}</dd></div><div><dt>{t.articlesArchived}</dt><dd>{stats.articles_with_content ?? 0}</dd></div><div><dt>{t.articlesPending}</dt><dd>{stats.articles_pending_content ?? 0}</dd></div></dl>}
      <div className="btn-row"><button type="button" className="btn secondary" disabled={backfillBusy} onClick={() => void runBackfill()}>{backfillBusy ? t.backfilling : t.backfillBtn}</button></div>
      {backfillNote && <div className="save-state" role="status">✓ {backfillNote}</div>}
    </section></div>
    <section className="settings-card">
      <div className="card-heading"><span className="card-icon">✦</span><div><h3>{t.focusTitle}</h3><p>{t.focusSub}</p></div></div>
      <label className="field"><span>{t.focusLabel}</span><textarea rows={3} maxLength={1000} value={settings.focus_profile} placeholder={t.focusHint} onChange={(e) => { setSaved(false); setSettings({ ...settings, focus_profile: e.target.value }) }} /><small>{t.focusHint}</small></label>
      <div className="research-controls">
        <label className="field"><span>{t.morningTime}</span><input maxLength={5} placeholder="07:30" value={settings.brief_morning_time} onChange={(e) => { setSaved(false); setSettings({ ...settings, brief_morning_time: e.target.value }) }} /></label>
        <label className="field"><span>{t.eveningTime}</span><input maxLength={5} placeholder="18:00" value={settings.brief_evening_time} onChange={(e) => { setSaved(false); setSettings({ ...settings, brief_evening_time: e.target.value }) }} /></label>
        <label className="field"><span>{t.briefTz}</span><input maxLength={64} placeholder="Asia/Ho_Chi_Minh" value={settings.brief_timezone} onChange={(e) => { setSaved(false); setSettings({ ...settings, brief_timezone: e.target.value }) }} /></label>
      </div>
    </section>
    <section className="settings-card">
      <div className="card-heading">
        <span className="card-icon">✦</span>
        <div>
          <h3>{t.settingsAgentsCardTitle}</h3>
          <p>{t.settingsAgentsCardSub}</p>
        </div>
      </div>
      <div className="btn-row">
        <button type="button" className="btn secondary" onClick={onOpenAgentsModels}>
          ✦ {t.settingsAgentsCardBtn}
        </button>
      </div>
    </section>
    <div className="settings-actions"><span className="muted">Version {settings.version} · MCP + Obsidian two-way sync</span><div className="btn-row"><button className="btn primary" onClick={save} disabled={busy}>{busy ? t.saving : t.save}</button></div></div><RegionsManager t={t} lang={lang} regions={regions} onChanged={onRegionsChanged} /><HubManager t={t} /><ConnectorHubPanel t={t} /><LegalSourcesPanel t={t} lang={lang} /><TelegramPanel t={t} /><SecretsManager t={t} /><ConnectorsPanel t={t} /><AlertRulesManager t={t} lang={lang} /><CatalogManager lang={lang} api={api} regions={regions} onChanged={onCatalogChanged} /></div>
}

function RegionsManager({ t, lang, regions, onChanged }: { t: Copy; lang: Lang; regions: RegionsPayload | null; onChanged: () => void }) {
  const [busy, setBusy] = useState('')
  const [error, setError] = useState(false)
  const toggle = async (region: Region) => {
    setBusy(region.code); setError(false)
    try { await api(`/api/admin/regions/${encodeURIComponent(region.code)}`, { method: 'PUT', body: JSON.stringify({ enabled: !region.enabled }) }); onChanged() }
    catch { setError(true) } finally { setBusy('') }
  }
  const reload = async () => {
    setBusy('reload'); setError(false)
    try { await api('/api/admin/regions/reload', { method: 'POST' }); onChanged() }
    catch { setError(true) } finally { setBusy('') }
  }
  const pack = regions?.pack
  const depth = (region: Region): number => {
    let levels = 0
    let parent = region.parent_code
    const seen = new Set([region.code])
    while (parent && levels < 8) {
      if (seen.has(parent)) break
      seen.add(parent)
      levels += 1
      parent = regions?.data.find((item) => item.code === parent)?.parent_code ?? null
    }
    return levels
  }
  return <section className="regions-admin" aria-labelledby="regions-title">
    <div className="section-heading"><div><span className="eyebrow">{t.admin}</span><h2 id="regions-title">{t.regionsTitle}</h2><p>{t.regionsSub}</p></div>
      <div className="btn-row section-actions"><span className="muted">{pack?.last_loaded_at ? `${t.packLast}: ${new Intl.DateTimeFormat(lang === 'vi' ? 'vi-VN' : 'en-GB', { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' }).format(new Date(pack.last_loaded_at * 1000))}` : t.packNever}</span>
        <button type="button" className="btn secondary" disabled={busy !== ''} onClick={() => void reload()}>{busy === 'reload' ? t.reloading : t.reloadRegions}</button></div></div>
    {error && <div className="form-error" role="alert">{t.regionError}</div>}
    {!regions ? <p className="muted" role="status">{t.loading}</p> : <div className="regions-layout">
      <div className="regions-tree" aria-label={t.regionTree}>
        {regions.data.length === 0 && <p className="muted">{t.noRegions}</p>}
        {regions.data.map((region) => <div className="region-row" key={region.code} style={{ paddingLeft: `${depth(region) * 14}px` }}>
          <div className="region-main"><strong>{lang === 'vi' ? region.label_vi : region.label_en}</strong><span>{region.code}{region.enabled && region.code !== 'global' && regions.effective.includes(region.code) ? ` · ${t.regionInherit}` : ''} · {region.source_count} {lang === 'vi' ? 'nguồn' : 'sources'}</span></div>
          <button type="button" role="switch" className="switch" aria-checked={region.enabled} aria-label={`${t.regionEnable}: ${region.code}`} disabled={busy !== '' || region.code === 'global'} onClick={() => void toggle(region)} />
        </div>)}
      </div>
      <div className="pack-panel">
        <h3>{t.packIssues}{pack && pack.issues.length > 0 ? ` (${pack.issues.length})` : ''}</h3>
        {pack && <p className="muted">{t.packSummary(pack.regions_registered, pack.sources_registered)}</p>}
        {!pack || pack.issues.length === 0 ? <p className="muted">{t.packClean}</p> : <ul className="pack-issues">{pack.issues.map((issue, index) => <li key={`${issue.path}-${issue.code}-${index}`}><code>{issue.code}</code> <span>{issue.path || '/'}</span>{issue.detail && <small>{issue.detail}</small>}</li>)}</ul>}
      </div>
    </div>}
  </section>
}

function formatPoint(lang: Lang, seconds: number) {
  return new Intl.DateTimeFormat(lang === 'vi' ? 'vi-VN' : 'en-GB', { day: 'numeric', month: 'short', year: 'numeric' }).format(new Date(seconds * 1000))
}

function Market({ t, lang, region }: { t: Copy; lang: Lang; region: string }) {
  const [series, setSeries] = useState<SeriesRow[] | null>(null)
  const [selected, setSelected] = useState<string | null>(null)
  const [points, setPoints] = useState<Observation[] | null>(null)
  const [error, setError] = useState(false)
  useEffect(() => {
    setSeries(null); setSelected(null); setPoints(null); setError(false)
    const query = region ? `?region=${encodeURIComponent(region)}` : ''
    api<SeriesPayload>(`/api/v1/series${query}`).then((payload) => setSeries(payload.data)).catch(() => setError(true))
  }, [region])
  useEffect(() => {
    if (!selected) { setPoints(null); return }
    api<ObservationsPayload>(`/api/v1/series/${encodeURIComponent(selected)}/observations?limit=30`).then((payload) => setPoints(payload.data)).catch(() => setError(true))
  }, [selected])
  if (error && !series) return <p className="form-error" role="alert">{t.loadError}</p>
  if (!series) return <p className="muted" role="status">{t.loading}</p>
  if (series.length === 0) return <div className="content-stack"><section className="section-heading"><div><span className="eyebrow">{t.market}</span><h2>{t.marketTitle}</h2><p>{t.marketSub}</p></div></section><div className="empty-card"><div className="empty-art">◭</div><h3>{t.noSeries}</h3><p>{t.noSeriesSub}</p></div></div>
  const latest = points?.[0]
  return <div className="content-stack"><section className="section-heading"><div><span className="eyebrow">{t.market}</span><h2>{t.marketTitle}</h2><p>{t.marketSub}</p></div></section>
    <div className="series-grid">{series.map((row) => <button type="button" key={row.code} className="series-card" aria-pressed={selected === row.code} onClick={() => setSelected(selected === row.code ? null : row.code)}>
      <span className="eyebrow">{row.region} · {row.frequency}</span><h3>{lang === 'vi' ? row.label_vi : row.label_en}</h3><p>{row.code}{row.unit ? ` · ${row.unit}` : ''}</p><small className="series-attribution">{row.attribution || t.noAttribution}</small>
    </button>)}</div>
    {selected && <section className="series-detail"><h3>{series.find((row) => row.code === selected)?.code}</h3><p className="series-attribution">{series.find((row) => row.code === selected)?.attribution || t.noAttribution}</p>
      {!points ? <p className="muted">{t.observing}…</p> : points.length === 0 ? <p className="muted">{t.noSeries}</p> : <dl className="stat-pairs"><div><dt>{t.latestValue}</dt><dd>{latest?.value}</dd></div><div><dt>{t.observedAt}</dt><dd>{latest ? formatPoint(lang, latest.observed_at) : '—'}</dd></div><div><dt>{t.asOf}</dt><dd>{latest ? formatPoint(lang, latest.as_of) : '—'}</dd></div><div><dt>{t.revision(latest?.revision ?? 1)}</dt><dd>{t.seriesPoints(points.length)}</dd></div></dl>}
    </section>}
  </div>
}

function HubManager({ t }: { t: Copy }) {
  const [status, setStatus] = useState<ProviderStatus | null>(null)
  const [key, setKey] = useState('')
  const [busy, setBusy] = useState(false)
  const [check, setCheck] = useState<{ ok: boolean; text: string } | null>(null)
  const [notice, setNotice] = useState('')
  const [error, setError] = useState(false)
  const load = () => api<ProviderStatus>('/api/admin/provider').then(setStatus).catch(() => setStatus(null))
  useEffect(() => { void load() }, [])
  const saveKey = async (event: FormEvent) => {
    event.preventDefault(); setBusy(true); setError(false); setNotice('')
    try {
      await api(`/api/admin/secrets/${encodeURIComponent(t.hubKeyName)}`, { method: 'PUT', body: JSON.stringify({ value: key }) })
      // The field is cleared at once; nothing reads the value back.
      setKey(''); setNotice(t.hubSaved); await load()
    } catch { setError(true) } finally { setBusy(false) }
  }
  const runCheck = async () => {
    setBusy(true); setError(false); setCheck(null)
    try {
      const result = await api<{ models: string[] }>('/api/admin/provider/check', { method: 'POST' })
      setCheck({ ok: true, text: t.hubCheckOk(result.models.length) })
    } catch {
      setCheck({ ok: false, text: t.hubCheckFail })
    } finally { setBusy(false); await load() }
  }
  return <section className="secrets-admin" aria-labelledby="hub-title">
    <div className="section-heading"><div><span className="eyebrow">{t.admin}</span><h2 id="hub-title">{t.hubTitle}</h2><p>{t.hubSub}</p></div>
      {status && <span className={status.configured ? 'catalog-status' : 'catalog-status inactive'}>{status.configured ? t.hubHasKey : t.hubNotConfigured}</span>}</div>
    {error && <div className="form-error" role="alert">{t.settingsError}</div>}
    {notice && <div className="save-state" role="status">✓ {notice}</div>}
    {check && <div className={check.ok ? 'catalog-notice' : 'form-error'} role="status">{check.text}</div>}
    {status && <dl className="stat-pairs">
      <div><dt>{t.hubHost}</dt><dd>{status.base_host ?? '—'}</dd></div>
      <div><dt>{t.hubAllowed}</dt><dd>{status.allowed_hosts.join(', ') || '—'}</dd></div>
      <div><dt>{t.hubClassify}</dt><dd>{status.models[0]}</dd></div>
      <div><dt>{t.hubSummarise}</dt><dd>{status.models[1]}</dd></div>
      <div><dt>{t.hubAnalyse}</dt><dd>{status.models[2]}</dd></div>
      <div><dt>{t.hubAgent}</dt><dd>{status.agent}</dd></div>
      <div><dt>{t.hubBudget}</dt><dd>{status.tokens_used_today}/{status.daily_budget}</dd></div>
      <div><dt>{t.hubSecret}</dt><dd>{status.has_secret ? t.hubHasKey : t.hubNoKey}</dd></div>
    </dl>}
    <form className="catalog-form" onSubmit={saveKey} autoComplete="off">
      <h3>{t.hubKeyName}</h3>
      <label className="field"><span>{t.hubSecret}</span><input required type="password" autoComplete="new-password" maxLength={8192} value={key} onChange={(e) => setKey(e.target.value)} /><small>{t.hubKeyHint}</small></label>
      <div className="btn-row catalog-form-actions">
        <button className="btn primary" disabled={busy || !key}>{busy ? t.saving : t.saveSecret}</button>
        <button type="button" className="btn secondary" disabled={busy || !status?.configured} onClick={() => void runCheck()}>{busy ? t.hubChecking : t.hubCheck}</button>
      </div>
    </form>
  </section>
}

function TelegramPanel({ t }: { t: Copy }) {
  const [settings, setSettings] = useState<Settings | null>(null)
  const [status, setStatus] = useState<TelegramStatus | null>(null)
  const [mode, setMode] = useState<'relay' | 'direct' | 'off'>('off')
  const [chatId, setChatId] = useState('')
  const [topicId, setTopicId] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState(false)
  const [notice, setNotice] = useState('')
  const load = async () => {
    try {
      const [next, health] = await Promise.all([api<Settings>('/api/settings'), api<TelegramStatus>('/api/admin/telegram')])
      setSettings(next); setStatus(health); setMode(next.telegram_mode_stored)
      setChatId(next.telegram_chat_id === null ? '' : String(next.telegram_chat_id))
      setTopicId(next.telegram_alerts_topic_id === null ? '' : String(next.telegram_alerts_topic_id))
      setError(false)
    } catch { setError(true) }
  }
  useEffect(() => { void load() }, [])
  const save = async (event: FormEvent) => {
    event.preventDefault(); setBusy(true); setError(false); setNotice('')
    try {
      await api<Settings>('/api/settings', { method: 'PATCH', body: JSON.stringify({ telegram_mode: mode, telegram_chat_id: chatId.trim(), telegram_alerts_topic_id: topicId.trim() }) })
      setNotice(t.savedToast); await load()
    } catch { setError(true) } finally { setBusy(false) }
  }
  const flush = async () => {
    setBusy(true); setError(false); setNotice('')
    try {
      const result = await api<{ sent: number }>('/api/admin/telegram/flush', { method: 'POST' })
      setNotice(t.tgFlushed(result.sent)); await load()
    } catch { setError(true) } finally { setBusy(false) }
  }
  if (!settings) return <div role={error ? 'alert' : 'status'} className="loading-inline">{error ? t.settingsError : t.loading}</div>
  const ready = settings.telegram_ready
  return <section className="telegram-admin" aria-labelledby="telegram-title">
    <div className="section-heading"><div><span className="eyebrow">{t.admin}</span><h2 id="telegram-title">{t.tgTitle}</h2><p>{t.tgSub}</p></div>
      <span className={ready ? 'save-state' : 'muted'} role="status">{ready ? `✓ ${t.tgReady}` : `${t.tgNotReady}${settings.telegram_reason ? ` · ${settings.telegram_reason}` : ''}`}</span></div>
    {settings.telegram_env_override && <p className="catalog-notice" role="status">{t.tgEnvOverride}</p>}
    {error && <div className="form-error" role="alert">{t.settingsError}</div>}
    {notice && <div className="save-state" role="status">✓ {notice}</div>}
    <form className="catalog-form" onSubmit={save} autoComplete="off">
      <Select label={t.tgMode} value={mode} options={[{ value: 'off', label: t.tgOff }, { value: 'relay', label: t.tgRelay }, { value: 'direct', label: t.tgDirect }]} onChange={(value) => setMode(value as 'relay' | 'direct' | 'off')} />
      <label className="field"><span>{t.tgChatId}</span><input inputMode="numeric" value={chatId} onChange={(e) => setChatId(e.target.value)} placeholder="-1001234567890" /></label>
      <label className="field"><span>{t.tgTopic}</span><input inputMode="numeric" value={topicId} onChange={(e) => setTopicId(e.target.value)} placeholder="42" /></label>
      <p className="muted">{settings.telegram_has_relay_token ? `✓ ${t.tgRelayToken}` : `— ${t.tgRelayToken}`} · {settings.telegram_has_bot_token ? `✓ ${t.tgBotToken}` : `— ${t.tgBotToken}`}</p>
      {status && <p className="muted">{t.tgQueue}: {status.queue.pending} {t.tgPending} · {status.queue.sending} · {status.queue.sent} {t.tgSent} · {status.queue.failed} {t.tgFailed}</p>}
      <div className="btn-row catalog-form-actions">
        <button className="btn primary" disabled={busy}>{busy ? t.saving : t.tgSave}</button>
        <button type="button" className="btn secondary" disabled={busy || !ready} onClick={() => void flush()}>{t.tgFlushNow}</button>
      </div>
    </form>
  </section>
}

function SecretsManager({ t }: { t: Copy }) {
  const [items, setItems] = useState<SecretMeta[] | null>(null)
  const [fingerprint, setFingerprint] = useState<string | null>(null)
  const [closed, setClosed] = useState(false)
  const [name, setName] = useState('')
  const [value, setValue] = useState('')
  const [busy, setBusy] = useState(false)
  const [notice, setNotice] = useState('')
  const [error, setError] = useState(false)
  const load = () => api<{ data: SecretMeta[]; key_fingerprint: string | null }>('/api/admin/secrets')
    .then((payload) => { setItems(payload.data); setFingerprint(payload.key_fingerprint); setClosed(false) })
    .catch((err) => { setItems([]); setClosed(err instanceof ApiError && err.status === 503) })
  useEffect(() => { void load() }, [])
  const save = async (event: FormEvent) => {
    event.preventDefault(); setBusy(true); setError(false); setNotice('')
    try {
      await api(`/api/admin/secrets/${encodeURIComponent(name)}`, { method: 'PUT', body: JSON.stringify({ value }) })
      // The value is cleared immediately and never read back from the server.
      setValue(''); setName(''); setNotice(t.secretSaved); await load()
    } catch { setError(true) } finally { setBusy(false) }
  }
  const remove = async (keyName: string) => {
    setBusy(true); setError(false)
    try { await api(`/api/admin/secrets/${encodeURIComponent(keyName)}`, { method: 'DELETE' }); await load() }
    catch { setError(true) } finally { setBusy(false) }
  }
  return <section className="secrets-admin" aria-labelledby="secrets-title">
    <div className="section-heading"><div><span className="eyebrow">{t.admin}</span><h2 id="secrets-title">{t.secretsTitle}</h2><p>{t.secretsSub}</p></div>{fingerprint && <span className="muted">key {fingerprint}</span>}</div>
    {closed && <p className="catalog-notice" role="status">{t.notConfigured}</p>}
    {error && <div className="form-error" role="alert">{t.settingsError}</div>}
    {notice && <div className="save-state" role="status">✓ {notice}</div>}
    <div className="catalog-layout">
      <div className="catalog-list">
        {!items ? <p className="muted">{t.loading}</p> : items.length === 0 ? <p className="muted">{t.noRegions}</p> : items.map((item) => <article className="catalog-entry" key={item.name}>
          <div className="catalog-entry-main"><strong>{item.name}</strong><span>v{item.key_version} · {item.updated_at}</span></div>
          <div className="catalog-entry-actions"><button type="button" className="text-button" disabled={busy} onClick={() => void remove(item.name)}>{busy ? t.deleting : t.removeSecret}</button></div>
        </article>)}
      </div>
      <form className="catalog-form" onSubmit={save} autoComplete="off">
        <label className="field"><span>{t.secretName}</span><input required maxLength={64} pattern="[a-z0-9]+(-[a-z0-9]+)*" value={name} onChange={(e) => setName(e.target.value)} placeholder="provider-api-key" /></label>
        <label className="field"><span>{t.secretValue}</span><input required type="password" autoComplete="new-password" maxLength={8192} value={value} onChange={(e) => setValue(e.target.value)} /></label>
        <div className="btn-row catalog-form-actions"><button className="btn primary" disabled={busy || closed || !name || !value}>{busy ? t.saving : t.saveSecret}</button></div>
      </form>
    </div>
  </section>
}

function LegalSourcesPanel({ t, lang }: { t: Copy; lang: Lang }) {
  const [items, setItems] = useState<LegalSource[] | null>(null)
  const [error, setError] = useState('')
  const load = () => api<{ data: LegalSource[] }>('/api/admin/legal-sources').then((p) => setItems(p.data)).catch(() => setItems([]))
  useEffect(() => { load() }, [])
  const toggle = async (row: LegalSource) => {
    setError('')
    try {
      await api(`/api/admin/legal-sources/${encodeURIComponent(row.code)}`, { method: 'PUT', body: JSON.stringify({ enabled: !row.enabled }) })
      load()
    } catch (err) {
      // The server names the refusal (`terms_unverified`, `no_documented_channel`, …) and
      // the copy table holds one human sentence per code, so no error text is invented here.
      const code = (err as Error).message || ''
      const known = code in t ? String((t as unknown as Record<string, string>)[code]) : ''
      setError(known || t.loadError)
    }
  }
  const date = (value: string) => value ? new Intl.DateTimeFormat(lang === 'vi' ? 'vi-VN' : 'en-GB', { day: 'numeric', month: 'short', year: 'numeric' }).format(new Date(`${value}T00:00:00`)) : ''
  return <section className="legal-sources" aria-labelledby="legal-title">
    <div className="section-heading"><div><span className="eyebrow">{t.admin}</span><h2 id="legal-title">{t.legalTitle}</h2><p>{t.legalSub}</p></div></div>
    {error && <div className="form-error" role="alert">{error}</div>}
    {!items ? <p className="muted">{t.loading}</p> : items.length === 0 ? <p className="muted">{t.legalSub}</p> : <div className="catalog-list">{items.map((row) => <article className="catalog-entry" key={row.code}>
      <div className="catalog-entry-main">
        <strong>{lang === 'vi' ? row.name_vi : row.name_en}</strong>
        <span>{row.region} · {row.licence}</span>
        {row.block_reason === 'no_documented_channel' && <span className="legal-reason"><strong>{t.legalNoChannel}</strong> · {row.access_note}</span>}
        {row.block_reason === 'terms_unverified' && <span className="legal-reason">{t.legalReasonUnverified}</span>}
        {row.block_reason === 'terms_prohibit_polling' && <span className="legal-reason">{t.legalReasonProhibited}</span>}
        <span className="catalog-health">{t.legalChecked}: {date(row.terms_checked_on)}{row.evidence_stale ? ` · ${t.legalStale}` : ''}</span>
        {row.terms_quote && <details className="legal-terms"><summary>{t.legalTerms}</summary><blockquote>{row.terms_quote}</blockquote><a href={row.terms_url} target="_blank" rel="noopener noreferrer">{row.terms_url}</a></details>}
      </div>
      <div className="catalog-entry-actions">
        {row.enabled
          ? <button className="btn" onClick={() => toggle(row)}>{t.legalTurnsOff}</button>
          : row.pollable && !row.block_reason
            ? <button className="btn primary" onClick={() => toggle(row)}>{t.legalTurnsOn}</button>
            : <span className="catalog-status inactive">{row.block_reason === 'no_documented_channel' ? t.legalNoChannel : row.block_reason === 'terms_prohibit_polling' ? t.legalBlockedProhibited : row.block_reason === 'terms_unverified' ? t.legalBlockedUnverified : t.legalBlocked}</span>}
      </div>
    </article>)}</div>}
  </section>
}

/// The Business MCP Hub card. It shows **where** each value came from and whether it
/// is set — never the value itself, because both of the important ones are secrets.
/// Saving here only touches the audience and the scope ceiling; the two secrets are
/// written through the provider-keys panel like every other sealed value.
function ConnectorHubPanel({ t }: { t: Copy }) {
  const [card, setCard] = useState<ConnectorCard | null>(null)
  const [writes, setWrites] = useState<AgentWrite[] | null>(null)
  const [analyses, setAnalyses] = useState<AgentAnalysis[] | null>(null)
  const [audience, setAudience] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState(false)
  const [notice, setNotice] = useState('')
  const load = () => api<ConnectorCard>('/api/admin/connector').then((next) => { setCard(next); setAudience(next.audience) }).catch(() => setCard(null))
  const loadAgentWork = () => {
    api<{ data: AgentWrite[] }>('/api/admin/connector/writes').then((p) => setWrites(p.data)).catch(() => setWrites([]))
    api<{ data: AgentAnalysis[] }>('/api/admin/connector/analyses').then((p) => setAnalyses(p.data)).catch(() => setAnalyses([]))
  }
  useEffect(() => { void load(); loadAgentWork() }, [])
  useEffect(() => { void load() }, [])
  const save = async (event: FormEvent) => {
    event.preventDefault(); setBusy(true); setError(false); setNotice('')
    try {
      await api('/api/admin/connector', { method: 'PATCH', body: JSON.stringify({ audience: audience.trim() }) })
      setNotice(t.mcpSaved); await load()
    } catch { setError(true) } finally { setBusy(false) }
  }
  // "set" must come from the source, not be printed unconditionally: an unset value
  // read as "đã đặt · chưa đặt" and told the owner nothing.
  const source = (value: ConnectorSource) => value === 'unset' ? t.mcpUnset : `${t.mcpSet} · ${value === 'env' ? t.mcpFromEnv : t.mcpFromAdmin}`
  return <section className="secrets-admin" aria-labelledby="mcp-title">
    <div className="section-heading"><div><span className="eyebrow">{t.admin}</span><h2 id="mcp-title">{t.mcpTitle}</h2><p>{t.mcpSub}</p></div>
      {card && <span className={card.enabled ? 'catalog-status' : 'catalog-status inactive'}>{card.enabled ? t.mcpReady : t.mcpNotReady}</span>}</div>
    {error && <div className="form-error" role="alert">{t.settingsError}</div>}
    {notice && <div className="save-state" role="status">✓ {notice}</div>}
    {card && !card.secret_store_open && <div className="form-error" role="alert">{t.mcpStoreClosed}</div>}
    {card && <dl className="stat-pairs">
      <div><dt>{t.mcpBearer}</dt><dd>{source(card.bearer)}</dd></div>
      <div><dt>{t.mcpSecret}</dt><dd>{source(card.delegation_secret)}</dd></div>
      <div><dt>{t.mcpScopes}</dt><dd>{card.max_scopes.join(', ') || '—'}</dd></div>
    </dl>}
    <form className="catalog-form" onSubmit={save} autoComplete="off">
      <label className="field"><span>{t.mcpAudience}</span><input value={audience} maxLength={64} onChange={(e) => setAudience(e.target.value)} /><small>{t.mcpHint}</small></label>
      <div className="btn-row catalog-form-actions"><button className="btn primary" disabled={busy}>{busy ? t.saving : t.mcpSave}</button></div>
    </form>
    <h3>{t.mcpAnalyses}</h3>
    {!analyses ? <p className="muted">{t.loading}</p> : analyses.length === 0 ? <p className="muted">{t.mcpNoAnalyses}</p> : <div className="catalog-list">{analyses.map((row) => <article className="catalog-entry" key={row.id}>
      <div className="catalog-entry-main"><strong>{row.title}</strong>
        <span>{row.kind}{row.topic_slug ? ` · ${row.topic_slug}` : ''}{row.region_code ? ` · ${row.region_code}` : ''} · {row.source_article_ids.length} {t.mcpSources}</span>
        {/* The author label is the whole point: the owner must be able to tell an agent's
            output from their own at a glance. */}
        <span className="catalog-status">{row.author ?? t.mcpOwner}</span>
      </div>
    </article>)}</div>}
    <h3>{t.mcpWrites}</h3>
    <p className="muted">{t.mcpWritesSub}</p>
    {!writes ? <p className="muted">{t.loading}</p> : writes.length === 0 ? <p className="muted">{t.mcpNoWrites}</p> : <div className="catalog-list">{writes.map((row) => <article className="catalog-entry" key={row.id}>
      <div className="catalog-entry-main"><strong>{row.tool}</strong>
        <span>{row.record_kind}{row.record_id !== null ? ` #${row.record_id}` : ''}{row.note ? ` · ${row.note}` : ''}</span>
        <span className="catalog-status">{row.client_id ?? t.mcpOwner}</span>
        <span className="catalog-health">{new Intl.DateTimeFormat(undefined, { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' }).format(new Date(row.at * 1000))}</span>
      </div>
    </article>)}</div>}
  </section>
}

function ConnectorsPanel({ t }: { t: Copy }) {
  const [items, setItems] = useState<ConnectorStatus[] | null>(null)
  const [autoEnabled, setAutoEnabled] = useState(false)
  const [busyId, setBusyId] = useState('')
  const [notice, setNotice] = useState('')
  const [error, setError] = useState('')
  const load = async () => {
    try {
      const payload = await api<{ data: ConnectorStatus[]; auto_enabled?: boolean }>('/api/admin/connectors')
      setItems(payload.data)
      setAutoEnabled(Boolean(payload.auto_enabled))
    } catch { setItems([]) }
  }
  useEffect(() => { void load() }, [])
  const explain = (code: string) => {
    if (code === 'connector_region_off') return t.connectorErrRegion
    if (code === 'connector_secret_missing' || code === 'connector_secret_unreadable') return t.connectorErrSecret
    if (code === 'connector_not_configured') return t.connectorErrConfig
    if (code === 'connector_cache_fresh' || code === 'connector_not_due') return t.connectorErrCache
    if (code === 'connector_quota_exhausted') return t.connectorErrQuota
    return `${t.loadError} (${code})`
  }
  const toggleAuto = async () => {
    setBusyId('auto'); setError(''); setNotice('')
    try {
      const next = await api<Settings>('/api/settings', { method: 'PATCH', body: JSON.stringify({ connectors_auto_enabled: !autoEnabled }) })
      setAutoEnabled(next.connectors_auto_enabled)
      setNotice(t.savedToast)
    } catch { setError(t.settingsError) } finally { setBusyId('') }
  }
  const toggleConnector = async (item: ConnectorStatus) => {
    setBusyId(item.id); setError(''); setNotice('')
    try {
      await api(`/api/admin/connectors/${encodeURIComponent(item.id)}`, { method: 'PUT', body: JSON.stringify({ enabled: !item.enabled }) })
      await load()
    } catch (err) {
      setError(explain(err instanceof Error ? err.message : 'error'))
    } finally { setBusyId('') }
  }
  const runNow = async (item: ConnectorStatus) => {
    setBusyId(`run:${item.id}`); setError(''); setNotice('')
    try {
      const { data } = await api<{ data: { observations: number; documents: number; events: number } }>(`/api/admin/connectors/${encodeURIComponent(item.id)}/run`, { method: 'POST' })
      setNotice(t.connectorRanOk(data.observations, data.documents, data.events))
      await load()
    } catch (err) {
      setError(explain(err instanceof Error ? err.message : 'error'))
      await load()
    } finally { setBusyId('') }
  }
  const when = (seconds: number | null) => seconds ? new Intl.DateTimeFormat(undefined, { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' }).format(new Date(seconds * 1000)) : null
  return <section className="connectors-admin" aria-labelledby="connectors-title">
    <div className="section-heading"><div><span className="eyebrow">{t.admin}</span><h2 id="connectors-title">{t.connectors}</h2><p>{t.connectorAutoHelp}</p></div></div>
    <div className="switch-row"><div><strong>{autoEnabled ? t.connectorAutoOn : t.connectorAutoOff}</strong><span>{t.connectorAutoHelp}</span></div><button type="button" role="switch" className="switch" aria-checked={autoEnabled} disabled={busyId !== ''} onClick={() => void toggleAuto()} /></div>
    {error && <div className="form-error" role="alert">{error}</div>}
    {notice && <div className="save-state" role="status">✓ {notice}</div>}
    {!items ? <p className="muted">{t.loading}</p> : items.length === 0 ? <p className="muted">{t.noSeriesSub}</p> : <div className="catalog-list">{items.map((item) => <article className="catalog-entry" key={item.id}>
      <div className="catalog-entry-main"><strong>{item.id}</strong><span>{item.provider} · {item.region}{item.needs_secret ? ` · ${t.needsSecret}` : ''}</span>
        <span className={item.enabled ? 'catalog-status' : 'catalog-status inactive'}>{item.enabled ? t.connectorEnable : t.connectorDisable} · {t.connectorHealth}: {item.health}</span>
        <span className="catalog-health">{item.last_run_at ? `${t.asOf}: ${when(item.last_run_at)}` : t.neverRan}{item.last_error ? ` · ${item.last_error}` : ''} · {t.quota}: {item.requests_today}/{item.daily_quota}</span>
      </div>
      <div className="catalog-entry-actions">
        <button type="button" className="text-button" disabled={busyId !== '' || !item.enabled} onClick={() => void runNow(item)}>{busyId === `run:${item.id}` ? t.connectorRunning : t.connectorRunNow}</button>
        <button type="button" className="text-button" disabled={busyId !== ''} onClick={() => void toggleConnector(item)}>{item.enabled ? t.connectorDisable : t.connectorEnable}</button>
      </div>
    </article>)}</div>}
  </section>
}

function ReportsPage({ t, lang, region, dashboard, onOpenArticle, onOpenAgents }: { t: Copy; lang: Lang; region: string; dashboard: Dashboard | null; onOpenArticle: (id: number) => void; onOpenAgents: () => void }) {
  const topics = dashboard?.topics ?? []
  const [items, setItems] = useState<AgentAnalysis[] | null>(null)
  const [selectedId, setSelectedId] = useState<number | null>(null)
  const [detail, setDetail] = useState<AnalysisDetailPayload | null>(null)
  const [question, setQuestion] = useState('')
  const [topic, setTopic] = useState('')
  const [mode, setMode] = useState<'analyse' | 'agent'>('analyse')
  const [saveAsBrief, setSaveAsBrief] = useState(true)
  const [customTitle, setCustomTitle] = useState('')
  const [asking, setAsking] = useState(false)
  const [researchOut, setResearchOut] = useState<ResearchResponse | null>(null)
  const [researchErr, setResearchErr] = useState('')
  const [noteText, setNoteText] = useState('')
  const [noteBusy, setNoteBusy] = useState(false)
  const [error, setError] = useState(false)
  const [companionOpen, setCompanionOpen] = useState(false)

  const loadList = async (pickId?: number) => {
    try {
      const suffix = region ? `&region=${encodeURIComponent(region)}` : ''
      const payload = await api<{ data: AgentAnalysis[] }>(`/api/v1/analyses?limit=50${suffix}`)
      setItems(payload.data); setError(false)
      const nextId = pickId ?? (selectedId && payload.data.some((row) => row.id === selectedId) ? selectedId : payload.data[0]?.id ?? null)
      setSelectedId(nextId)
    } catch { setError(true); setItems([]) }
  }
  const loadDetail = async (id: number) => {
    try {
      const payload = await api<AnalysisDetailPayload>(`/api/v1/analyses/${id}`)
      setDetail(payload)
    } catch { setDetail(null) }
  }
  useEffect(() => { void loadList() }, [region])
  useEffect(() => {
    if (selectedId !== null) void loadDetail(selectedId)
    else setDetail(null)
  }, [selectedId])

  const submitResearch = async (event: FormEvent) => {
    event.preventDefault()
    if (!question.trim()) return
    setAsking(true); setResearchErr(''); setResearchOut(null)
    try {
      const { data } = await api<{ data: ResearchResponse }>('/api/admin/research', {
        method: 'POST',
        body: JSON.stringify({
          question: question.trim(),
          region: region || undefined,
          topic: topic || undefined,
          mode,
          save_as_brief: saveAsBrief,
          title: customTitle.trim() || undefined,
        }),
      })
      setResearchOut(data)
      if (data.saved_analysis_id) {
        setCustomTitle('')
        await loadList(data.saved_analysis_id)
        await loadDetail(data.saved_analysis_id)
      }
    } catch (err) {
      const code = err instanceof Error ? err.message : ''
      setResearchErr(code.startsWith('provider_') ? t.askHubHint : t.loadError)
    } finally { setAsking(false) }
  }

  const submitNote = async (event: FormEvent) => {
    event.preventDefault()
    if (!selectedId || !noteText.trim()) return
    setNoteBusy(true)
    try {
      await api('/api/notes', { method: 'POST', body: JSON.stringify({ analysis_id: selectedId, body: noteText.trim() }) })
      setNoteText('')
      await loadDetail(selectedId)
    } catch { setError(true) } finally { setNoteBusy(false) }
  }

  const deleteNote = async (noteId: number) => {
    if (!selectedId) return
    setNoteBusy(true)
    try {
      await api(`/api/notes/${noteId}`, { method: 'DELETE' })
      await loadDetail(selectedId)
    } catch { setError(true) } finally { setNoteBusy(false) }
  }

  const stamp = (sec: number) => formatDate(lang, new Date(sec * 1000).toISOString()) ?? ''
  return <div className="content-stack">
    <section className="section-heading"><div><span className="eyebrow">{t.reports}</span><h2>{t.reportsTitle}</h2><p>{t.reportsSub}</p></div></section>
    <div className="reports-salon-banner">
      <div>
        <strong>✦ {t.reportsSalonTitle}</strong>
        <p>{t.reportsSalonSub}</p>
      </div>
      <button type="button" className="btn primary" onClick={onOpenAgents}>
        ✦ {t.reportsSalonBtn}
      </button>
    </div>
    <form className="research-card" onSubmit={submitResearch}>
      <div><h3>{t.askTitle}</h3><p className="muted" style={{ margin: '4px 0 0' }}>{t.askSub}</p></div>
      <div className="chip-row">
        <button type="button" className="chip" onClick={() => setQuestion(t.askChip1Prompt)}>{t.askChip1}</button>
        <button type="button" className="chip" onClick={() => setQuestion(t.askChip2Prompt)}>{t.askChip2}</button>
        <button type="button" className="chip" onClick={() => setQuestion(t.askChip3Prompt)}>{t.askChip3}</button>
      </div>
      <label className="field"><span>{t.askTitle}</span><textarea rows={3} maxLength={2000} required value={question} placeholder={t.askPlaceholder} onChange={(e) => setQuestion(e.target.value)} /></label>
      <div className="research-controls">
        <Select label={t.byTopic} value={topic} options={[{ value: '', label: t.allTopics }, ...topics.map((item) => ({ value: item.slug, label: lang === 'vi' ? item.label_vi : item.label_en }))]} onChange={setTopic} />
        <Select label={t.askMode} value={mode} options={[{ value: 'analyse', label: t.askModeAnalyse }, { value: 'agent', label: t.askModeAgent }]} onChange={(v) => setMode(v as 'analyse' | 'agent')} />
        <label className="field"><span>{t.askBriefTitle}</span><input maxLength={300} value={customTitle} onChange={(e) => setCustomTitle(e.target.value)} placeholder="Morning Macro Brief…" /></label>
      </div>
      <label className="toggle-field"><span>{t.askSaveBrief}</span><input type="checkbox" checked={saveAsBrief} onChange={(e) => setSaveAsBrief(e.target.checked)} /></label>
      <div className="btn-row"><button className="btn primary" disabled={asking || !question.trim()}>{asking ? t.askRunning : t.askSubmit}</button></div>
      {researchErr && <div className="catalog-notice" role="status">{researchErr}</div>}
      {researchOut && <div className="research-answer" role="status">
        <div className="article-meta"><span className="tier-badge">{researchOut.model}</span><span>{researchOut.tokens} tokens</span>{researchOut.stable_key && <span>· {t.askSavedAs} <code>{researchOut.stable_key}</code></span>}</div>
        <div className="brief-body">{researchOut.answer}</div>
      </div>}
    </form>

    {error && <div className="form-error" role="alert">{t.loadError}</div>}
    {!items ? <p className="muted" role="status">{t.loading}</p> : items.length === 0
      ? <div className="empty-card"><div className="empty-art">◫</div><h3>{t.noBriefs}</h3><p>{t.noBriefsSub}</p></div>
      : <div className="reports-layout">
          <div className="brief-list" aria-label={t.reports}>
            {items.map((item) => <button type="button" key={item.id} className={selectedId === item.id ? 'brief-item active' : 'brief-item'} onClick={() => setSelectedId(item.id)}>
              <div className="article-meta"><span className="key-chip">{item.stable_key ?? `analysis-${item.id}`}</span><span>{item.author ?? t.mcpOwner}</span></div>
              <h3>{item.title}</h3>
              <div className="article-meta"><span>{stamp(item.updated_at)}</span>{item.region_code && <span>· {item.region_code}</span>}<span>· {item.source_article_ids.length} {t.mcpSources}</span></div>
            </button>)}
          </div>
          {detail && <article className="brief-reader">
            <header className="brief-reader-head">
              <div className="brief-reader-top">
                <div className="article-meta"><span className="key-chip">{detail.data.stable_key ?? `analysis-${detail.data.id}`}</span><span className="tier-badge">{detail.data.kind}</span><span>{detail.data.author ?? t.mcpOwner}</span>{detail.data.region_code && <span>· {detail.data.region_code}</span>}{detail.data.topic_slug && <span>· {detail.data.topic_slug}</span>}<span>· {stamp(detail.data.updated_at)}</span></div>
                <button type="button" className={companionOpen ? 'btn primary compact-btn' : 'btn secondary compact-btn'} onClick={() => setCompanionOpen((v) => !v)}>{t.companionToggleReportBtn}</button>
              </div>
              <h2>{detail.data.title}</h2>
            </header>
            <CompanionDrawer
              open={companionOpen}
              onClose={() => setCompanionOpen(false)}
              lang={lang}
              kind="report"
              reportId={detail.data.id}
              title={detail.data.title}
              sourceAttribution={detail.data.author ?? t.mcpOwner}
              region={detail.data.region_code ?? region}
              topic={detail.data.topic_slug}
              contentText={detail.data.body ?? null}
              onOpenArticle={onOpenArticle}
            />
            <div className="brief-body">{detail.data.body}</div>
            <section className="cited-box">
              <h3>{t.citedArticles} ({detail.cited_articles.length})</h3>
              {detail.cited_articles.length === 0 ? <p className="muted">{t.noCited}</p> : <ul className="cited-list">{detail.cited_articles.map((art) => <li key={art.id}><span className="key-chip">#{art.id}</span><span className="tier-badge">{art.source_tier}</span><button type="button" className="cited-open-btn" onClick={() => onOpenArticle(art.id)}>{art.title}</button><span className="muted">({art.source_name})</span><a className="ext-pill" href={art.url} target="_blank" rel="noopener noreferrer">{t.openOriginal}</a></li>)}</ul>}
            </section>
            <section className="notes-box">
              <div><h3>{t.feedbackTitle} ({detail.notes.length})</h3><p className="muted" style={{ margin: '4px 0 0' }}>{t.feedbackSub}</p></div>
              {detail.notes.length === 0 ? <p className="muted">{t.feedbackEmpty}</p> : <div className="notes-list">{detail.notes.map((note) => <div className="note-card" key={note.id}>
                <div className="note-head">
                  <span><strong>{note.author ?? t.mcpOwner}</strong> · {stamp(note.updated_at)}</span>
                  <button type="button" className="text-button" disabled={noteBusy} onClick={() => void deleteNote(note.id)}>{t.removeSecret}</button>
                </div>
                <p>{note.body}</p>
              </div>)}</div>}
              <form className="login-form" onSubmit={submitNote}>
                <textarea rows={3} maxLength={4000} required value={noteText} placeholder={t.feedbackPlaceholder} onChange={(e) => setNoteText(e.target.value)} />
                <div className="btn-row"><button className="btn secondary" disabled={noteBusy || !noteText.trim()}>{noteBusy ? t.saving : t.feedbackSubmit}</button></div>
              </form>
            </section>
          </article>}
        </div>}
  </div>
}

function RegionsPage({ t, lang, region, onOpenArticle }: { t: Copy; lang: Lang; region: string; onOpenArticle: (id: number) => void }) {
  const [docs, setDocs] = useState<DocsPayload | null>(null)
  const [tax, setTax] = useState<TaxPayload | null>(null)
  const [query, setQuery] = useState('')
  const [topic, setTopic] = useState('')
  const [error, setError] = useState(false)
  const suffix = region ? `&region=${encodeURIComponent(region)}` : ''
  useEffect(() => {
    setError(false)
    const search = query.trim() ? `&q=${encodeURIComponent(query.trim())}` : ''
    const topicFilter = topic ? `&topic=${encodeURIComponent(topic)}` : ''
    api<DocsPayload>(`/api/v1/documents?limit=50${suffix}${search}${topicFilter}`).then(setDocs).catch(() => setError(true))
    api<TaxPayload>(`/api/v1/tax-calendar?horizon_days=180${suffix}`).then(setTax).catch(() => setError(true))
  }, [region, query, topic])
  const date = (value: string | null) => value ? new Intl.DateTimeFormat(lang === 'vi' ? 'vi-VN' : 'en-GB', { day: 'numeric', month: 'short', year: 'numeric' }).format(new Date(`${value}T00:00:00`)) : null
  return <div className="content-stack">
    <section className="section-heading"><div><span className="eyebrow">{t.regionsPage}</span><h2>{t.regionsPage}</h2><p>{t.regionsPageSub}</p></div></section>
    {error && <div className="form-error" role="alert">{t.loadError}</div>}
    <section className="region-block"><div className="section-heading"><div><span className="eyebrow">{t.regionNews}</span><h3>{t.latest}</h3></div></div><ArticleList t={t} lang={lang} topic={null} region={region} onOpenArticle={onOpenArticle} /></section>
    <section className="region-block"><div className="section-heading"><div><span className="eyebrow">{t.regionDocs}</span><h3>{t.regionDocs}</h3></div></div>
      <div className="doc-filters">
        <label className="field"><span>{t.searchDocs}</span><input type="search" value={query} placeholder={t.searchHint} onChange={(e) => setQuery(e.target.value)} /><small>{t.searchHint}</small></label>
        <Select label={t.allTopicsDoc} value={topic} options={[{ value: '', label: t.allTopicsDoc }, { value: 'tax', label: lang === 'vi' ? 'Thuế' : 'Tax' }]} onChange={setTopic} />
      </div>
      {!docs ? <p className="muted" role="status">{t.loading}</p> : docs.data.length === 0 ? <p className="muted">{t.noDocs}</p> : <div className="doc-list">{docs.data.map((doc) => <article className="doc-row" key={`${doc.provider}-${doc.number}`}>
        <a href={doc.url} target="_blank" rel="noopener noreferrer">{doc.title}</a>
        <div className="article-meta"><span className="tier-badge">{doc.region}</span><span>{doc.agency || doc.provider}</span><span>· {doc.number}</span>{doc.effective_on && <span>· {t.effectiveOn} {date(doc.effective_on)}</span>}{doc.published_on && <span>· {t.publishedOn} {date(doc.published_on)}</span>}</div>
        {doc.summary && <div className="doc-summary"><strong>{t.docSummary}</strong><p>{doc.summary}</p><small>{doc.disclaimer}</small></div>}
      </article>)}</div>}
    </section>
    <section className="region-block"><div className="section-heading"><div><span className="eyebrow">{t.regionTax}</span><h3>{t.regionTax}</h3></div></div>
      <p className="catalog-notice">{t.taxNotice} {t.holidaysNote}</p>
      {!tax ? <p className="muted" role="status">{t.loading}</p> : tax.data.length === 0 ? <p className="muted">{t.noTax}</p> : <ul className="tax-list">{tax.data.map((item) => <li key={item.code} className={item.due_soon ? 'tax-row due' : 'tax-row'}>
        <div><strong>{lang === 'vi' ? item.name_vi : item.name_en}</strong><span>{item.region} · {item.cadence} · {date(item.due_on)}</span>{item.note && <small>{item.note}</small>}</div>
        <div className="tax-side"><span className={item.due_soon ? 'tax-days due' : 'tax-days'}>{item.due_soon ? t.dueSoon : t.dueIn(item.days_until)}</span><span className={item.verified ? 'catalog-status' : 'catalog-status inactive'}>{item.verified ? t.verified : t.unverified}</span></div>
      </li>)}</ul>}
    </section>
  </div>
}

function ArticleReaderModal({ t, lang, articleId, topics, onClose, onUpdated }: { t: Copy; lang: Lang; articleId: number; topics?: Topic[]; onClose: () => void; onUpdated: () => void }) {
  const [payload, setPayload] = useState<ArticleDetailPayload | null>(null)
  const [extracting, setExtracting] = useState(false)
  const [error, setError] = useState(false)
  const [noteText, setNoteText] = useState('')
  const [noteBusy, setNoteBusy] = useState(false)
  const [companionOpen, setCompanionOpen] = useState(false)
  const autoTried = useRef<number | null>(null)

  const load = async (id: number) => {
    setError(false)
    try {
      const next = await api<ArticleDetailPayload>(`/api/v1/articles/${id}`)
      setPayload(next)
      if (!next.data.content_text && next.data.content_source === 'none' && autoTried.current !== id) {
        autoTried.current = id
        setExtracting(true)
        try {
          const extracted = await api<ArticleDetailPayload>(`/api/articles/${id}/extract`, { method: 'POST' })
          setPayload(extracted)
          if (extracted.data.content_text) onUpdated()
        } catch { /* fallback to summary/link cleanly */ } finally { setExtracting(false) }
      }
    } catch { setError(true) }
  }

  useEffect(() => {
    setPayload(null)
    void load(articleId)
    const onKey = (event: KeyboardEvent) => { if (event.key === 'Escape') onClose() }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [articleId])

  const triggerExtract = async () => {
    setExtracting(true); setError(false)
    try {
      const extracted = await api<ArticleDetailPayload>(`/api/articles/${articleId}/extract`, { method: 'POST' })
      setPayload(extracted)
      onUpdated()
    } catch { setError(true) } finally { setExtracting(false) }
  }

  const submitNote = async (event: FormEvent) => {
    event.preventDefault()
    if (!noteText.trim()) return
    setNoteBusy(true)
    try {
      await api('/api/notes', { method: 'POST', body: JSON.stringify({ article_id: articleId, body: noteText.trim() }) })
      setNoteText('')
      const refreshed = await api<ArticleDetailPayload>(`/api/v1/articles/${articleId}`)
      setPayload(refreshed)
    } catch { setError(true) } finally { setNoteBusy(false) }
  }

  const deleteNote = async (noteId: number) => {
    setNoteBusy(true)
    try {
      await api(`/api/notes/${noteId}`, { method: 'DELETE' })
      const refreshed = await api<ArticleDetailPayload>(`/api/v1/articles/${articleId}`)
      setPayload(refreshed)
    } catch { setError(true) } finally { setNoteBusy(false) }
  }

  const item = payload?.data
  const notes = payload?.notes ?? []
  const paragraphs = (item?.content_text ?? '').split(/\n{2,}/).map((p) => p.trim()).filter(Boolean)
  const words = item?.word_count ?? 0
  const minutes = words > 0 ? Math.max(1, Math.round(words / 220)) : 0
  const topicText = item ? topicLabel(lang, topics, item.topic_slug) : null
  const stamp = (sec: number) => formatDate(lang, new Date(sec * 1000).toISOString()) ?? ''

  return <div className="reader-backdrop" role="dialog" aria-modal="true" aria-label={item?.title ?? t.readInApp} onClick={(e) => { if (e.target === e.currentTarget) onClose() }}>
    <div className="reader-sheet reader-shell">
      <div className="reader-toolbar">
        <div className="article-meta">
          {item && <span className="tier-badge">{item.source_tier}</span>}
          {item && <strong>{item.source_name}</strong>}
          {topicText && <span>· {topicText}</span>}
          {item?.region_code && <span>· {item.region_code}</span>}
          {words > 0 && <span className="content-badge">{words} {t.wordsUnit} · {t.readMinutes(minutes)}</span>}
        </div>
        <div className="reader-toolbar-actions">
          {item && <button type="button" className={companionOpen ? 'btn primary compact-btn' : 'btn secondary compact-btn'} onClick={() => setCompanionOpen((v) => !v)}>{t.companionToggleBtn}</button>}
          {item && <button type="button" className="btn secondary compact-btn" disabled={extracting} onClick={() => void triggerExtract()}>{extracting ? t.loading : item.content_text ? t.reExtract : t.extractNow}</button>}
          {item && <a className="btn secondary compact-btn" href={item.url} target="_blank" rel="noopener noreferrer">{t.openOriginal}</a>}
          <button type="button" className="btn primary compact-btn" onClick={onClose}>{t.closeReader}</button>
        </div>
      </div>

      {item && (
        <CompanionDrawer
          open={companionOpen}
          onClose={() => setCompanionOpen(false)}
          lang={lang}
          kind="article"
          articleId={articleId}
          title={item.title}
          sourceAttribution={item.source_attribution || item.source_name}
          region={item.region_code}
          topic={item.topic_slug}
          contentText={item.content_text}
          excerpt={item.summary ?? null}
          onOpenArticle={(id) => void load(id)}
        />
      )}

      {error && <div className="form-error" role="alert">{t.loadError}</div>}
      {!item && !error && <p className="muted" role="status">{t.loading}</p>}

      {item && <article className="reader-article">
        <header className="reader-article-head">
          <h2>{item.title}</h2>
          <div className="article-meta">
            <span>{formatDate(lang, item.published_at) ?? `${t.fetchedAt} ${formatDate(lang, item.fetched_at) ?? ''}`}</span>
            {item.content_source && <span>· {item.content_source === 'web' ? t.contentWeb : item.content_source === 'feed' ? t.contentFeed : t.contentSummary}</span>}
            {item.source_attribution && <span>· {item.source_attribution}</span>}
          </div>
        </header>

        {extracting && <div className="catalog-notice" role="status">{t.extracting}</div>}

        {item.summary && (!item.content_text || !item.content_text.startsWith(item.summary.slice(0, 80))) && (
          <blockquote className="reader-lead">{item.summary}</blockquote>
        )}

        {paragraphs.length > 0 ? (
          <div className="reader-prose">
            {paragraphs.map((para, idx) => <p key={idx}>{para}</p>)}
          </div>
        ) : !extracting && (
          <div className="reader-fallback">
            {!item.summary && <p className="muted">{t.readerNoBody}</p>}
            <a className="btn primary" href={item.url} target="_blank" rel="noopener noreferrer">{t.openOriginal}</a>
          </div>
        )}

        <section className="notes-box">
          <h3>{t.articleNotesTitle} ({notes.length})</h3>
          {notes.length > 0 && <div className="notes-list">{notes.map((note) => <div className="note-card" key={note.id}>
            <div className="note-head">
              <span><strong>{note.author ?? t.mcpOwner}</strong> · {stamp(note.updated_at)}</span>
              <button type="button" className="text-button" disabled={noteBusy} onClick={() => void deleteNote(note.id)}>{t.removeSecret}</button>
            </div>
            <p>{note.body}</p>
          </div>)}</div>}
          <form className="login-form" onSubmit={submitNote}>
            <textarea rows={2} maxLength={4000} required value={noteText} placeholder={t.articleNotesPlaceholder} onChange={(e) => setNoteText(e.target.value)} />
            <div className="btn-row"><button className="btn secondary" disabled={noteBusy || !noteText.trim()}>{noteBusy ? t.saving : t.feedbackSubmit}</button></div>
          </form>
        </section>
      </article>}
    </div>
  </div>
}

createRoot(document.getElementById('root')!).render(<App />)
