import { FormEvent, useEffect, useState } from 'react'
import { createRoot } from 'react-dom/client'
import { CatalogManager } from './CatalogManager'
import { Select } from './Select'
import './style.css'

type Lang = 'vi' | 'en'
type ReaderMode = 'paper' | 'sepia' | 'night'
type Page = 'today' | 'topics' | 'regions' | 'market' | 'stories' | 'reports' | 'alerts' | 'timeline' | 'saved' | 'settings'
type Topic = { slug: string; label_vi: string; label_en: string; parent_slug: string | null; enabled: boolean }
type Stats = { collector_enabled: boolean; articles_total: number; articles_24h: number; sources_enabled: number; sources_paused: number; topic_counts: Record<string, number> }
type Dashboard = { topics: Topic[]; stats: Stats }
type Settings = { version: string; language: Lang; default_reader_mode: ReaderMode; collector_enabled: boolean }
type Article = { id: number; title: string; url: string; published_at: string | null; fetched_at: string | null; topic_slug: string | null; region_code: string | null; source_name: string; source_tier: string }
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
type ProviderStatus = { configured: boolean; base_host: string | null; allowed_hosts: string[]; models: [string, string, string]; agent: string; timeout_seconds: number; tokens_used_today: number; daily_budget: number; calls_today: number; has_secret: boolean }
type ConnectorStatus = { id: string; provider: string; region: string; enabled: boolean; health: string; last_error: string | null; last_run_at: number | null; next_run_at: number | null; requests_today: number; daily_quota: number; needs_secret: boolean }

const copy = {
  vi: {
    today: 'Hôm nay', topics: 'Chủ đề', stories: 'Câu chuyện', reports: 'Báo cáo', alerts: 'Cảnh báo', timeline: 'Dòng thời gian', saved: 'Đã lưu', settings: 'Cài đặt',
    subtitle: 'Không gian đọc và nghiên cứu tin', signIn: 'Đăng nhập quản trị', key: 'Khóa quản trị', keyHint: 'Dùng khóa MANAGEMENT_KEY được cấu hình cho ứng dụng.', enter: 'Vào Newsman', invalid: 'Không đăng nhập được. Hãy kiểm tra khóa và thử lại.', limited: 'Tạm khóa đăng nhập do thử quá nhiều lần. Hãy thử lại sau 5 phút.', missing: 'Chưa cấu hình MANAGEMENT_KEY trên máy chủ.', welcome: 'Chào mừng trở lại.', checking: 'Đang kiểm tra…',
    briefing: 'Một nơi để đọc sâu hơn', briefingSub: 'Tin mới từ các nguồn đang bật, xếp theo thời gian. Mỗi tin dẫn về bài gốc; Newsman chỉ lưu tiêu đề và đường dẫn.', noStories: 'Chưa có tin', noStoriesSub: 'Hãy thêm nguồn RSS/Atom, bật nguồn và bật thu thập tự động trong Cài đặt, hoặc bấm “Quét ngay” ở từng nguồn.', nextPhase: 'Thu thập tin tự động',
    latest: 'Tin mới nhất', articles24h: 'Tin trong 24 giờ', articlesTotal: 'Tổng số tin', sourcesEnabled: 'Nguồn đang bật', sourcesPaused: 'Nguồn tạm dừng do lỗi', allTopics: 'Tất cả', loadMore: 'Xem thêm', loading: 'Đang tải…', fetchedAt: 'Lấy về', noDate: 'Không rõ ngày đăng',
    collectorOn: 'Đang bật thu thập tự động', collectorOff: 'Thu thập tự động đang tắt', collectorHelp: 'Khi bật, hệ thống quét các nguồn đang bật theo khoảng quét đã đặt, cách nhau ít nhất 5 giây cho mỗi máy chủ. Nguồn lỗi 5 lần liên tiếp sẽ tự tạm dừng.', collectorToggle: 'Bật hoặc tắt thu thập tự động', topicArticles: 'tin',
    paper: 'Giấy', sepia: 'Sepia', night: 'Đêm', changeTheme: 'Đổi chế độ đọc', refresh: 'Làm mới', topicsTracked: 'Chủ đề đang bật', pendingAlerts: 'Cảnh báo', upcoming: 'Chưa triển khai',
    settingsTitle: 'Cài đặt hiển thị', settingsSub: 'Đặt mặc định cho lần đầu mở ứng dụng. Thiết lập trên trình duyệt hiện tại sẽ được ưu tiên.', language: 'Ngôn ngữ mặc định', readerMode: 'Chế độ đọc mặc định', save: 'Lưu mặc định', saving: 'Đang lưu…', savedToast: 'Đã lưu cài đặt', admin: 'Quản trị', signOut: 'Đăng xuất',
    emptyTitle: 'Tính năng đang được chuẩn bị', emptySub: 'Báo cáo, cảnh báo, dòng thời gian và mục đã lưu sẽ có ở các giai đoạn sau.', sampleTopics: 'Các chủ đề đang bật', sampleTopicsSub: 'Chọn một chủ đề để xem tin của chủ đề đó và các chủ đề con. Quản lý chủ đề trong phần Cài đặt.', settingsError: 'Không tải hoặc lưu được cài đặt. Hãy thử lại.', loadError: 'Không tải được dữ liệu. Hãy thử làm mới.', dismiss: 'Đóng', failedLogout: 'Không đăng xuất được; hãy thử lại.', changeLanguage: 'Chuyển ngôn ngữ',
    region: 'Vùng', allRegions: 'Mọi vùng đang bật', regionsTitle: 'Vùng và region pack', regionsSub: 'Bật vùng để job quét và trang đọc dùng vùng đó cùng các vùng cha. Vùng mới phát hiện từ thư mục pack luôn ở trạng thái tắt.',
    reloadRegions: 'Nạp lại vùng', reloading: 'Đang nạp…', packIssues: 'Lỗi pack', packClean: 'Không có lỗi pack.', packSummary: (r: number, s: number) => `${r} vùng, ${s} nguồn từ pack`, packNever: 'Chưa nạp pack', packLast: 'Lần nạp gần nhất',
    regionTree: 'Cây vùng', regionEnable: 'Bật hoặc tắt vùng', regionInherit: 'gồm vùng cha', noRegions: 'Chưa có vùng nào.', regionError: 'Không đổi được vùng. Kiểm tra vùng cha trước.',
    regions: 'Vùng',
    regionsPage: 'Vùng', regionsPageSub: 'Tin, văn bản chính thức và lịch thuế theo vùng đang bật (gồm vùng cha).', regionNews: 'Tin theo vùng', regionDocs: 'Văn bản chính thức', regionTax: 'Lịch thuế', noDocs: 'Chưa có văn bản nào cho vùng này.', noTax: 'Chưa có hạn nộp nào được khai báo cho vùng này.',
    dueIn: (n: number) => n === 0 ? 'hôm nay' : `còn ${n} ngày`, dueSoon: 'Sắp tới hạn', unverified: 'Chưa xác minh với cơ quan thuế', verified: 'Đã xác minh', taxNotice: 'Không phải tư vấn pháp lý/thuế. Hạn nộp do region pack khai báo, chưa gọi mạng; hãy kiểm tra lại với cơ quan thuế.', holidaysNote: 'Chưa tính ngày lễ công cộng; chỉ dời theo cuối tuần khi pack khai báo.', docSummary: 'Tóm tắt tự viết', docNoSummary: 'Chưa có tóm tắt tự viết cho văn bản này.', effectiveOn: 'Hiệu lực', publishedOn: 'Ban hành', searchDocs: 'Tìm văn bản', searchHint: 'Gõ không dấu cũng tìm được (vd: thue).', allTopicsDoc: 'Tất cả chủ đề',
    market: 'Thị trường', marketTitle: 'Chuỗi số liệu', marketSub: 'Dữ liệu số theo vùng. Chưa kết nối nhà cung cấp nào: danh sách này chỉ có khi một connector thật được duyệt và chạy.', noSeries: 'Chưa có chuỗi số liệu', noSeriesSub: 'P2 mới có kho chuỗi và khung connector. Dữ liệu thật cần chọn nhà cung cấp (plan mục 8).',
    seriesPoints: (n: number) => `${n} điểm`, observing: 'Lấy dữ liệu', latestValue: 'Giá trị mới nhất', asOf: 'Ghi nhận lúc', observedAt: 'Kỳ dữ liệu', revision: (n: number) => `bản ${n}`, noAttribution: 'Chưa có dòng ghi nguồn cho chuỗi này.',
    hubTitle: 'LNC Agent Hub', hubSub: 'Kết nối Hub cho phân loại, tóm tắt và nghiên cứu. Khóa chỉ ghi, không hiện lại. Cấu hình địa chỉ do máy chủ quyết định (biến môi trường), không nhập ở đây.', hubKeyName: 'hub-api-key', hubKeyHint: 'Khóa riêng của Newsman trên Hub. Dán vào đây; giá trị không bao giờ được trả lại.', hubCheck: 'Kiểm tra kết nối', hubChecking: 'Đang kiểm tra…', hubCheckOk: (n: number) => `Kết nối được, thấy ${n} model. Chỉ liệt kê model, chưa sinh nội dung, chưa tốn token.`, hubCheckFail: 'Không kết nối được Hub. Kiểm tra địa chỉ, allowlist host và khóa.', hubHost: 'Host', hubAllowed: 'Allowlist', hubModels: 'Model theo tác vụ', hubClassify: 'Phân loại', hubSummarise: 'Tóm tắt', hubAnalyse: 'Phân tích', hubAgent: 'Agent', hubBudget: 'Token hôm nay', hubSecret: 'Khóa', hubSaved: 'Đã lưu khóa Hub.', hubHasKey: 'đã có khóa', hubNoKey: 'chưa có khóa', hubNotConfigured: 'Máy chủ chưa đặt HUB_BASE_URL hoặc host chưa nằm trong HUB_ALLOWED_HOSTS.',
    secretsTitle: 'Khóa nhà cung cấp', secretsSub: 'Chỉ ghi, không bao giờ hiển thị lại. Cần LNC_MASTER_KEY hợp lệ trên máy chủ.', secretName: 'Tên khóa', secretValue: 'Giá trị', saveSecret: 'Lưu khóa', deleting: 'Đang xóa…', removeSecret: 'Xóa', secretSaved: 'Đã lưu khóa.', notConfigured: 'Máy chủ chưa cấu hình LNC_MASTER_KEY nên kho khóa đang đóng.', connectors: 'Connector', connectorHealth: 'Sức khỏe', quota: 'Hạn mức hôm nay', needsSecret: 'cần khóa', neverRan: 'Chưa chạy',
  },
  en: {
    today: 'Today', topics: 'Topics', stories: 'Stories', reports: 'Reports', alerts: 'Alerts', timeline: 'Timeline', saved: 'Saved', settings: 'Settings',
    subtitle: 'A place to read and research', signIn: 'Management sign in', key: 'Management key', keyHint: 'Use the MANAGEMENT_KEY configured for this application.', enter: 'Enter Newsman', invalid: 'Could not sign in. Check the key and try again.', limited: 'Too many sign-in attempts. Please try again in 5 minutes.', missing: 'MANAGEMENT_KEY is not configured on the server.', welcome: 'Welcome back.', checking: 'Checking…',
    briefing: 'A quieter place to read', briefingSub: 'New items from enabled sources, newest first. Every item links to the original; Newsman stores only titles and links.', noStories: 'No news yet', noStoriesSub: 'Add an RSS/Atom source, enable it and turn on automatic collection in Settings, or press “Fetch now” on a source.', nextPhase: 'Automatic collection',
    latest: 'Latest news', articles24h: 'Items in 24 hours', articlesTotal: 'Total items', sourcesEnabled: 'Enabled sources', sourcesPaused: 'Sources paused by errors', allTopics: 'All', loadMore: 'Load more', loading: 'Loading…', fetchedAt: 'Fetched', noDate: 'Publication date unknown',
    collectorOn: 'Automatic collection is on', collectorOff: 'Automatic collection is off', collectorHelp: 'When on, enabled sources are checked at their configured interval, at least 5 seconds apart per host. A source that fails 5 times in a row is paused automatically.', collectorToggle: 'Turn automatic collection on or off', topicArticles: 'items',
    paper: 'Paper', sepia: 'Sepia', night: 'Night', changeTheme: 'Change reading mode', refresh: 'Refresh', topicsTracked: 'Enabled topics', pendingAlerts: 'Alerts', upcoming: 'Not implemented yet',
    settingsTitle: 'Display settings', settingsSub: 'Set defaults for first-time visitors. This browser’s own preferences take precedence.', language: 'Default language', readerMode: 'Default reading mode', save: 'Save defaults', saving: 'Saving…', savedToast: 'Settings saved', admin: 'Admin', signOut: 'Sign out',
    emptyTitle: 'This feature is being prepared', emptySub: 'Reports, alerts, timeline and saved items arrive in later phases.', sampleTopics: 'Enabled topics', sampleTopicsSub: 'Pick a topic to read its items, including child topics. Manage topics in Settings.', settingsError: 'Could not load or save settings. Please try again.', loadError: 'Could not load data. Please refresh.', dismiss: 'Dismiss', failedLogout: 'Could not sign out. Please try again.', changeLanguage: 'Change language',
    region: 'Region', allRegions: 'All enabled regions', regionsTitle: 'Regions and region packs', regionsSub: 'Enable a region so jobs and the reader use it together with its parent regions. Regions discovered from the pack directory always start disabled.',
    reloadRegions: 'Reload regions', reloading: 'Reloading…', packIssues: 'Pack issues', packClean: 'No pack issues.', packSummary: (r: number, s: number) => `${r} regions, ${s} sources from packs`, packNever: 'Pack not loaded yet', packLast: 'Last load',
    regionTree: 'Region tree', regionEnable: 'Turn the region on or off', regionInherit: 'includes parents', noRegions: 'No regions yet.', regionError: 'Could not change the region. Check the parent region first.',
    regions: 'Regions',
    regionsPage: 'Regions', regionsPageSub: 'News, official documents and tax deadlines for the enabled regions (parents included).', regionNews: 'News by region', regionDocs: 'Official documents', regionTax: 'Tax calendar', noDocs: 'No documents for this region yet.', noTax: 'No deadline is declared for this region.',
    dueIn: (n: number) => n === 0 ? 'today' : `in ${n} days`, dueSoon: 'Due soon', unverified: 'Not checked with the tax authority', verified: 'Verified', taxNotice: 'Not legal or tax advice. Deadlines come from a declared region pack rule and no network call was made; verify them with the tax authority.', holidaysNote: 'Public holidays are not modelled; a date is only shifted by a declared weekend rule.', docSummary: 'Written summary', docNoSummary: 'No written summary for this document yet.', effectiveOn: 'Effective', publishedOn: 'Published', searchDocs: 'Search documents', searchHint: 'Accents are optional (e.g. thue).', allTopicsDoc: 'All topics',
    market: 'Markets', marketTitle: 'Time series', marketSub: 'Numeric data by region. No provider is connected yet: this list only fills once a real connector is approved and running.', noSeries: 'No series yet', noSeriesSub: 'P2 provides only the series store and the connector framework. Real data needs a chosen provider (plan section 8).',
    seriesPoints: (n: number) => `${n} points`, observing: 'Fetch data', latestValue: 'Latest value', asOf: 'Known as of', observedAt: 'Period', revision: (n: number) => `revision ${n}`, noAttribution: 'This series has no credit line yet.',
    hubTitle: 'LNC Agent Hub', hubSub: 'Connect the Hub for classification, summaries and research. The key is write-only and never shown again. The address is server configuration (environment), not entered here.', hubKeyName: 'hub-api-key', hubKeyHint: 'Newsman\u2019s own Hub key. Paste it here; the value is never returned.', hubCheck: 'Check connection', hubChecking: 'Checking…', hubCheckOk: (n: number) => `Connected, ${n} models visible. Model listing only — no content generated, no tokens billed.`, hubCheckFail: 'Could not reach the Hub. Check the address, the host allowlist and the key.', hubHost: 'Host', hubAllowed: 'Allowlist', hubModels: 'Model per task', hubClassify: 'Classify', hubSummarise: 'Summarise', hubAnalyse: 'Analyse', hubAgent: 'Agent', hubBudget: 'Tokens today', hubSecret: 'Key', hubSaved: 'Hub key saved.', hubHasKey: 'key present', hubNoKey: 'no key', hubNotConfigured: 'The server has no HUB_BASE_URL, or the host is not in HUB_ALLOWED_HOSTS.',
    secretsTitle: 'Provider keys', secretsSub: 'Write-only; never shown again. Requires a valid LNC_MASTER_KEY on the server.', secretName: 'Key name', secretValue: 'Value', saveSecret: 'Save key', deleting: 'Deleting…', removeSecret: 'Delete', secretSaved: 'Key saved.', notConfigured: 'The server has no LNC_MASTER_KEY, so the secret store is closed.', connectors: 'Connectors', connectorHealth: 'Health', quota: 'Today’s quota', needsSecret: 'needs a key', neverRan: 'Never ran',
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
const validMode = (value: string | null): value is ReaderMode => value === 'paper' || value === 'sepia' || value === 'night'
const pages: Page[] = ['today', 'topics', 'regions', 'market', 'stories', 'reports', 'alerts', 'timeline', 'saved', 'settings']
function currentPage(): Page { const hash = location.hash.slice(1) as Page; return pages.includes(hash) ? hash : 'today' }

function App() {
  const storedLang = storageGet('lnc-newsman-language')
  const storedMode = storageGet('lnc-newsman-reader-mode')
  const [authenticated, setAuthenticated] = useState<boolean | null>(null)
  const [managementConfigured, setManagementConfigured] = useState(false)
  const [lang, setLangState] = useState<Lang>(validLang(storedLang) ? storedLang : 'vi')
  const [mode, setModeState] = useState<ReaderMode>(validMode(storedMode) ? storedMode : 'paper')
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
      if (!validMode(storageGet('lnc-newsman-reader-mode')) && validMode(settings.default_reader_mode)) setModeState(settings.default_reader_mode)
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
  return <main className={`login-page mode-${mode}`}><section className="login-card">
    <div className="login-brand"><img src="/favicon.svg" alt="" /><div><strong>LNC Newsman</strong><span>{t.subtitle}</span></div></div>
    <div className="login-copy"><p className="eyebrow">{t.signIn}</p><h1>{t.welcome}</h1><p>{offline ? t.loadError : configured ? t.keyHint : t.missing}</p></div>
    <form onSubmit={submit} className="login-form"><label htmlFor="management-key">{t.key}</label><input id="management-key" type="password" autoComplete="current-password" autoFocus value={key} onChange={(e) => setKey(e.target.value)} placeholder="••••••••••••" /><button className="btn primary" disabled={!key || pending || !configured}>{pending ? t.checking : t.enter}</button>{error && <div className="form-error" role="alert">{error}</div>}</form>
    <div className="login-footer"><span>Private internal workspace</span><button type="button" className="language-link" onClick={() => setLang(lang === 'vi' ? 'en' : 'vi')}>{lang === 'vi' ? 'English' : 'Tiếng Việt'}</button></div>
  </section></main>
}

function Shell({ lang, setLang, mode, setMode, page, setPage, dashboard, regions, region, setRegion, onRegionsChanged, error, setError, onCatalogChanged, onSignOut }: { lang: Lang; setLang: (v: Lang) => void; mode: ReaderMode; setMode: (v: ReaderMode) => void; page: Page; setPage: (v: Page) => void; dashboard: Dashboard | null; regions: RegionsPayload | null; region: string; setRegion: (v: string) => void; onRegionsChanged: () => void; error: string; setError: (v: string) => void; onCatalogChanged: () => void; onSignOut: () => void }) {
  const t = copy[lang]
  const icons: Record<Page, string> = { today: '⌂', topics: '◌', regions: '⌖', market: '◭', stories: '▤', reports: '◫', alerts: '◈', timeline: '⌁', saved: '♡', settings: '⚙' }
  const setRoute = (next: Page) => { location.hash = next; setPage(next) }
  const cycleMode = () => setMode(mode === 'paper' ? 'sepia' : mode === 'sepia' ? 'night' : 'paper')
  const signOut = async () => { try { await api('/api/auth/logout', { method: 'POST' }); onSignOut() } catch { setError('logout') } }
  return <div className={`app-shell mode-${mode}`}>
    <aside className="sidebar"><div className="brand"><img src="/favicon.svg" alt="" /><div><strong>LNC Newsman</strong><span>INTELLIGENCE READER</span></div></div>
      <nav className="nav" aria-label="Primary navigation">{pages.map((id) => <button key={id} aria-label={t[id]} aria-current={page === id ? 'page' : undefined} className={page === id ? 'nav-item active' : 'nav-item'} onClick={() => setRoute(id)}><span className="nav-icon" aria-hidden="true">{icons[id]}</span><span>{t[id]}</span></button>)}</nav>
      <div className="side-divider" /><div className="sidebar-footer"><div className="version">v0.1.1 <span>· P1 collector</span></div><button className="footer-action" onClick={cycleMode} aria-label={`${t.changeTheme}: ${t[mode]}`}><span aria-hidden="true">{mode === 'night' ? '☾' : '☼'}</span>{t[mode]}</button><button className="footer-action signout" onClick={signOut}><span aria-hidden="true">↪</span>{t.signOut}</button></div>
    </aside>
    <main className="page-shell"><header className="page-header"><div><div className="breadcrumb">LNC / NEWSMAN</div><h1>{t[page]}</h1></div><div className="header-actions"><div className="region-picker">{regions && <Select label={t.region} value={region} options={[{ value: '', label: t.allRegions }, ...regions.data.filter((item) => item.enabled).map((item) => ({ value: item.code, label: lang === 'vi' ? item.label_vi : item.label_en, hint: item.code }))]} onChange={setRegion} />}</div><button className="language-toggle" aria-label={t.changeLanguage} onClick={() => setLang(lang === 'vi' ? 'en' : 'vi')}>{lang.toUpperCase()}</button><button className="icon-button" aria-label={t.refresh} onClick={() => location.reload()}>↻</button></div></header>
      {error && <div className="alert error" role="alert"><strong>{error === 'logout' ? t.failedLogout : t.loadError}</strong><button onClick={() => setError('')}>{t.dismiss}</button></div>}
      <div className="page-content">{page === 'today' && <Today t={t} lang={lang} dashboard={dashboard} region={region} setPage={setRoute} />}{page === 'topics' && <Topics t={t} lang={lang} dashboard={dashboard} region={region} />}{page === 'regions' && <RegionsPage t={t} lang={lang} region={region} />}{page === 'market' && <Market t={t} lang={lang} region={region} />}{page === 'settings' && <SettingsPage t={t} lang={lang} setLang={setLang} setMode={setMode} onCatalogChanged={onCatalogChanged} onRegionsChanged={onRegionsChanged} regions={regions} dashboard={dashboard} />}{!['today', 'topics', 'regions', 'market', 'settings'].includes(page) && <EmptyPage t={t} />}</div>
    </main>
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
function ArticleList({ t, lang, topic, topics, region }: { t: Copy; lang: Lang; topic: string | null; topics?: Topic[]; region?: string }) {
  const [items, setItems] = useState<Article[] | null>(null)
  const [error, setError] = useState(false)
  const [more, setMore] = useState(false)
  const [busy, setBusy] = useState(false)
  const pageSize = 30
  const load = async (offset: number) => {
    setBusy(true); setError(false)
    try {
      const query = new URLSearchParams({ limit: String(pageSize), offset: String(offset) })
      if (topic) query.set('topic', topic)
      if (region) query.set('region', region)
      const result = await api<{ data: Article[] }>(`/api/articles?${query}`)
      setItems((current) => offset === 0 || !current ? result.data : [...current, ...result.data])
      setMore(result.data.length === pageSize)
    } catch { setError(true) } finally { setBusy(false) }
  }
  useEffect(() => { setItems(null); void load(0) }, [topic, region])
  if (error && !items) return <p className="form-error" role="alert">{t.loadError}</p>
  if (!items) return <p className="muted" role="status">{t.loading}</p>
  if (items.length === 0) return <div className="empty-card"><div className="empty-art">✦</div><h3>{t.noStories}</h3><p>{t.noStoriesSub}</p></div>
  return <div><div className="article-list">{items.map((item) => {
    const published = formatDate(lang, item.published_at)
    const label = topicLabel(lang, topics, item.topic_slug)
    return <article className="article-row" key={item.id}>
      <a href={item.url} target="_blank" rel="noopener noreferrer">{item.title}</a>
      <div className="article-meta"><span className="tier-badge">{item.source_tier}</span><span>{item.source_name}</span>{label && <span>· {label}</span>}<span>· {published ?? `${t.fetchedAt} ${formatDate(lang, item.fetched_at) ?? ''}`}</span></div>
    </article>
  })}</div>{error && <p className="form-error" role="alert">{t.loadError}</p>}{more && <button className="btn secondary load-more" disabled={busy} onClick={() => void load(items.length)}>{busy ? t.loading : t.loadMore}</button>}</div>
}
function Today({ t, lang, dashboard, region, setPage }: { t: Copy; lang: Lang; dashboard: Dashboard | null; region: string; setPage: (v: Page) => void }) {
  const stats = dashboard?.stats
  return <div className="content-stack"><section className="welcome-grid"><div className="welcome-card"><div className="eyebrow">{new Intl.DateTimeFormat(lang === 'vi' ? 'vi-VN' : 'en-US', { weekday: 'long', day: 'numeric', month: 'long' }).format(new Date())}</div><h2>{t.briefing}</h2><p>{t.briefingSub}</p><div className="welcome-actions"><button className="btn primary" onClick={() => setPage('topics')}>{t.topics} <span>→</span></button><span className="quiet-note">{stats ? (stats.collector_enabled ? t.collectorOn : t.collectorOff) : ''}</span></div></div><div className="signal-card"><span className="signal-ring">◎</span><div><strong>{stats ? stats.articles_24h : '—'}</strong><span>{t.articles24h}</span></div><div className="signal-line" /><span className="muted">{stats ? `${stats.sources_enabled} · ${t.sourcesEnabled}${stats.sources_paused ? ` · ${stats.sources_paused} ${t.sourcesPaused.toLowerCase()}` : ''}` : '—'}</span></div></section>
    <section className="section-heading"><div><span className="eyebrow">{t.stories}</span><h2>{t.latest}</h2></div><span className="muted">{stats ? `${stats.articles_total} · ${t.articlesTotal}` : '—'}</span></section><ArticleList t={t} lang={lang} topic={null} topics={dashboard?.topics} region={region} /></div>
}
function Topics({ t, lang, dashboard, region }: { t: Copy; lang: Lang; dashboard: Dashboard | null; region: string }) {
  const [selected, setSelected] = useState<string | null>(null)
  const topics = dashboard?.topics ?? []
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
  // Count a topic together with its descendants, matching the reader filter.
  const total = (slug: string, seen = new Set<string>()): number => {
    if (seen.has(slug)) return 0
    seen.add(slug)
    return (dashboard?.stats.topic_counts[slug] ?? 0) + topics.filter((item) => item.parent_slug === slug).reduce((sum, child) => sum + total(child.slug, seen), 0)
  }
  if (!dashboard) return <p className="muted">{t.loadError}</p>
  return <div className="content-stack"><section className="section-heading"><div><span className="eyebrow">{t.topics}</span><h2>{t.sampleTopics}</h2><p>{t.sampleTopicsSub}</p></div></section>
    <div className="topic-grid">{topics.map((topic) => <button type="button" className="topic-card" key={topic.slug} aria-pressed={selected === topic.slug} onClick={() => setSelected(selected === topic.slug ? null : topic.slug)}><span className="topic-dot" />{parentTrail(topic) && <p>{parentTrail(topic)} →</p>}<h3>{lang === 'vi' ? topic.label_vi : topic.label_en}</h3><p>{lang === 'vi' ? topic.label_en : topic.label_vi}</p><span className="topic-count">{total(topic.slug)} {t.topicArticles}</span></button>)}</div>
    <section className="section-heading"><div><span className="eyebrow">{t.stories}</span><h2>{selected ? topicLabel(lang, topics, selected) : t.latest}</h2></div><div className="topic-chip-row"><button type="button" className="topic-chip" aria-pressed={selected === null} onClick={() => setSelected(null)}>{t.allTopics}</button></div></section>
    <ArticleList t={t} lang={lang} topic={selected} topics={topics} region={region} /></div>
}
function EmptyPage({ t }: { t: Copy }) { return <div className="content-stack"><div className="empty-card"><div className="empty-art">✦</div><h3>{t.emptyTitle}</h3><p>{t.emptySub}</p></div></div> }
function SettingsPage({ t, lang, setLang, setMode, onCatalogChanged, onRegionsChanged, regions, dashboard }: { t: Copy; lang: Lang; setLang: (v: Lang) => void; setMode: (v: ReaderMode) => void; onCatalogChanged: () => void; onRegionsChanged: () => void; regions: RegionsPayload | null; dashboard: Dashboard | null }) {
  const [settings, setSettings] = useState<Settings | null>(null)
  const [error, setError] = useState(false)
  const [saved, setSaved] = useState(false)
  const [busy, setBusy] = useState(false)
  useEffect(() => { api<Settings>('/api/settings').then(setSettings).catch(() => setError(true)) }, [])
  const save = async () => {
    if (!settings) return
    setBusy(true); setError(false); setSaved(false)
    try {
      const result = await api<Settings>('/api/settings', { method: 'PATCH', body: JSON.stringify({ language: settings.language, default_reader_mode: settings.default_reader_mode }) })
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
  if (!settings) return <div role={error ? 'alert' : 'status'} className="loading-inline">{error ? t.settingsError : t.loading}</div>
  const stats = dashboard?.stats
  return <div className="content-stack settings-page"><section className="section-heading"><div><span className="eyebrow">{t.admin}</span><h2>{t.settingsTitle}</h2><p>{t.settingsSub}</p></div>{saved && <span className="save-state" role="status">✓ {t.savedToast}</span>}</section>
    {error && <div className="form-error" role="alert">{t.settingsError}</div>}
    <div className="settings-grid"><section className="settings-card"><div className="card-heading"><span className="card-icon">Aa</span><div><h3>{t.settingsTitle}</h3><p>{t.settingsSub}</p></div></div>
      <Select label={t.language} value={settings.language} options={[{ value: 'vi', label: 'Tiếng Việt' }, { value: 'en', label: 'English' }]} onChange={(value) => { setSaved(false); setSettings({ ...settings, language: value as Lang }) }} />
      <Select label={t.readerMode} value={settings.default_reader_mode} options={[{ value: 'paper', label: t.paper }, { value: 'sepia', label: t.sepia }, { value: 'night', label: t.night }]} onChange={(value) => { setSaved(false); setSettings({ ...settings, default_reader_mode: value as ReaderMode }) }} />
    </section><section className="settings-card"><div className="card-heading"><span className="card-icon">◷</span><div><h3>{t.nextPhase}</h3><p>{t.collectorHelp}</p></div></div>
      <div className="switch-row"><div><strong>{settings.collector_enabled ? t.collectorOn : t.collectorOff}</strong></div><button type="button" role="switch" className="switch" aria-checked={settings.collector_enabled} aria-label={t.collectorToggle} disabled={busy} onClick={() => void toggleCollector()} /></div>
      {stats && <dl className="stat-pairs"><div><dt>{t.sourcesEnabled}</dt><dd>{stats.sources_enabled}</dd></div><div><dt>{t.sourcesPaused}</dt><dd>{stats.sources_paused}</dd></div><div><dt>{t.articles24h}</dt><dd>{stats.articles_24h}</dd></div><div><dt>{t.articlesTotal}</dt><dd>{stats.articles_total}</dd></div></dl>}
    </section></div>
    <div className="settings-actions"><span className="muted">Version {settings.version} · P1.5 regions</span><button className="btn primary" onClick={save} disabled={busy}>{busy ? t.saving : t.save}</button></div><RegionsManager t={t} lang={lang} regions={regions} onChanged={onRegionsChanged} /><HubManager t={t} /><SecretsManager t={t} /><ConnectorsPanel t={t} /><CatalogManager lang={lang} api={api} regions={regions} onChanged={onCatalogChanged} /></div>
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
      <div className="regions-actions"><span className="muted">{pack?.last_loaded_at ? `${t.packLast}: ${new Intl.DateTimeFormat(lang === 'vi' ? 'vi-VN' : 'en-GB', { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' }).format(new Date(pack.last_loaded_at * 1000))}` : t.packNever}</span>
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
      <div className="catalog-form-actions">
        <button className="btn primary" disabled={busy || !key}>{busy ? t.saving : t.saveSecret}</button>
        <button type="button" className="btn secondary" disabled={busy || !status?.configured} onClick={() => void runCheck()}>{busy ? t.hubChecking : t.hubCheck}</button>
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
        <div className="catalog-form-actions"><button className="btn primary" disabled={busy || closed || !name || !value}>{busy ? t.saving : t.saveSecret}</button></div>
      </form>
    </div>
  </section>
}

function ConnectorsPanel({ t }: { t: Copy }) {
  const [items, setItems] = useState<ConnectorStatus[] | null>(null)
  useEffect(() => { api<{ data: ConnectorStatus[] }>('/api/admin/connectors').then((p) => setItems(p.data)).catch(() => setItems([])) }, [])
  const when = (seconds: number | null) => seconds ? new Intl.DateTimeFormat(undefined, { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' }).format(new Date(seconds * 1000)) : null
  return <section className="connectors-admin" aria-labelledby="connectors-title">
    <div className="section-heading"><div><span className="eyebrow">{t.admin}</span><h2 id="connectors-title">{t.connectors}</h2></div></div>
    {!items ? <p className="muted">{t.loading}</p> : items.length === 0 ? <p className="muted">{t.noSeriesSub}</p> : <div className="catalog-list">{items.map((item) => <article className="catalog-entry" key={item.id}>
      <div className="catalog-entry-main"><strong>{item.id}</strong><span>{item.provider} · {item.region}{item.needs_secret ? ` · ${t.needsSecret}` : ''}</span>
        <span className={item.health === 'healthy' ? 'catalog-status' : 'catalog-status inactive'}>{t.connectorHealth}: {item.health}</span>
        <span className="catalog-health">{item.last_run_at ? `${t.asOf}: ${when(item.last_run_at)}` : t.neverRan}{item.last_error ? ` · ${item.last_error}` : ''}</span>
      </div>
      <div className="catalog-entry-actions"><span className="catalog-health">{t.quota}: {item.requests_today}/{item.daily_quota}</span></div>
    </article>)}</div>}
  </section>
}

function RegionsPage({ t, lang, region }: { t: Copy; lang: Lang; region: string }) {
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
    <section className="region-block"><div className="section-heading"><div><span className="eyebrow">{t.regionNews}</span><h3>{t.latest}</h3></div></div><ArticleList t={t} lang={lang} topic={null} region={region} /></section>
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

createRoot(document.getElementById('root')!).render(<App />)
