import { FormEvent, useEffect, useState } from 'react'
import { Select } from './Select'

type Lang = 'vi' | 'en'
type Api = <T>(path: string, init?: RequestInit) => Promise<T>
type Topic = { slug: string; label_vi: string; label_en: string; parent_slug: string | null; enabled: boolean }
type SourceInput = { slug: string; name: string; url: string; tier: string; enabled: boolean; language: string; region: string; topic_slug: string | null; fetch_interval_minutes: number }
type Source = SourceInput & { id: number; last_checked_at: string | null; last_success_at: string | null; last_error: string | null; consecutive_failures: number; paused_reason: string | null; next_fetch_at: number | null; article_count: number; pack_key: string | null }
type Region = { code: string; label_vi: string; label_en: string; parent_code: string | null; enabled: boolean }
type RunSummary = { status: 'ok' | 'not_modified' | 'error'; items_seen: number; items_new: number; error: string | null; paused: boolean }
type Preview = { final_url: string; items_seen: number; sample: { title: string; url: string; published_at: string | null }[] }
type Preset = { slug: string; name: string; url: string; tier: string; language: string; region: string; topic_slug: string | null; fetch_interval_minutes: number; attribution: string; installed: boolean; region_enabled: boolean }

type RegionsPayload = { data: Region[]; effective: string[] }
type Props = { lang: Lang; api: Api; regions?: RegionsPayload | null; onChanged: () => void }
const strings = {
  vi: {
    title: 'Nguồn & chủ đề', subtitle: 'Quản lý nguồn RSS/Atom và cây chủ đề dùng để xếp tin.', topics: 'Chủ đề', sources: 'Nguồn tin',
    notice: 'Nguồn đang bật chỉ được quét tự động khi bật “Thu thập tin tự động” ở trên. “Quét ngay” quét một lần. Newsman chỉ lưu tiêu đề, đường dẫn và ngày đăng, không lưu nội dung bài.',
    noTopics: 'Chưa có chủ đề.', noSources: 'Chưa có nguồn tin. Anh có thể thêm nguồn công khai bên dưới.',
    enabled: 'Đang bật', disabled: 'Đã tắt', paused: 'Tạm dừng do lỗi', edit: 'Sửa', disable: 'Tắt', enable: 'Bật lại', fetchNow: 'Quét ngay', resume: 'Tiếp tục', fetching: 'Đang quét…',
    addTopic: 'Thêm chủ đề', editTopic: 'Sửa chủ đề', addSource: 'Thêm nguồn tin', editSource: 'Sửa nguồn tin',
    slug: 'Mã định danh', slugHint: 'Chữ thường không dấu, số và dấu gạch ngang.', topicSlugHint: 'Mã chủ đề không đổi sau khi tạo.',
    viName: 'Tên tiếng Việt', enName: 'Tên tiếng Anh', parent: 'Chủ đề cha', none: 'Không chọn',
    sourceName: 'Tên nguồn', url: 'URL nguồn RSS / Atom', urlHint: 'Chỉ nhập URL công khai, không chứa mật khẩu, token hoặc khóa API.',
    tier: 'Bậc nguồn', language: 'Ngôn ngữ', region: 'Vùng', topic: 'Chủ đề mặc định',
    packManaged: 'Do region pack quản lý: chỉ bật/tắt ở đây, sửa nội dung trong file pack.', invalid_region: 'Vùng không tồn tại hoặc đang tắt. Bật vùng trước rồi mới bật nguồn.', source_managed_by_pack: 'Nguồn này do pack quản lý; chỉ có thể bật hoặc tắt.', regionOff: 'đang tắt',
    regionDisabled: 'Vùng của nguồn đang tắt — nguồn này sẽ không được quét cho đến khi bật vùng.',
    interval: 'Khoảng quét (phút)', intervalHint: '5–10.080 phút. Nguồn lỗi sẽ được thử lại thưa dần, tối đa mỗi ngày một lần.',
    active: 'Bật mục này', save: 'Lưu thay đổi', create: 'Thêm mới', cancel: 'Hủy sửa', saving: 'Đang lưu…', saved: 'Đã lưu danh mục.',
    test: 'Kiểm tra feed', testing: 'Đang kiểm tra…', previewOk: (n: number) => `Đọc được ${n} tin. Vài tiêu đề đầu:`, articles: 'tin',
    lastSuccess: 'Quét thành công', lastError: 'Lỗi gần nhất', never: 'Chưa quét', failures: (n: number) => `${n} lần lỗi liên tiếp`,
    runOk: (seen: number, added: number) => `Đã quét: ${seen} tin trong feed, ${added} tin mới.`, runNotModified: 'Feed chưa thay đổi kể từ lần quét trước.', runError: 'Quét không thành công:', previewError: 'Không đọc được feed:',
    duplicate_slug: 'Mã chủ đề đã tồn tại.', duplicate_source: 'Mã nguồn hoặc URL nguồn đã tồn tại.',
    topic_in_use: 'Hãy tắt chủ đề con và nguồn đang bật thuộc chủ đề này trước.', topic_cycle: 'Không thể đặt chủ đề cha tạo thành vòng lặp.',
    invalid_parent: 'Chủ đề cha không tồn tại hoặc đang tắt.', invalid_topic: 'Chủ đề không hợp lệ hoặc đang tắt.',
    invalid_source: 'Thông tin nguồn không hợp lệ. Kiểm tra URL công khai và khoảng quét.', immutable_slug: 'Không thể đổi mã chủ đề sau khi tạo.',
    fetch_in_progress: 'Nguồn này đang được quét. Hãy thử lại sau ít giây.',
    error: 'Không lưu hoặc tải được danh mục. Hãy kiểm tra dữ liệu và thử lại.', loading: 'Đang tải danh mục…',
    tier1: 'T1 · Cơ quan chính thức', tier2: 'T2 · Hãng thông tấn', tier3: 'T3 · Báo / tạp chí', tier4: 'T4 · Nguồn khác',
    presetsTitle: 'Gói nguồn gợi ý (1 chạm)', presetsSub: 'Các feed chính thức & báo chí công khai đã rà soát. Bấm “Thêm nguồn” sẽ đăng ký ở trạng thái Đã tắt (chưa gọi mạng); sau đó anh bật vùng tương ứng, bật nguồn hoặc bấm “Quét ngay”.',
    presetAdd: '+ Thêm nguồn', presetAdding: 'Đang thêm…', presetAdded: 'Đã có trong danh sách', presetSaved: 'Đã thêm nguồn gợi ý (đang tắt). Anh có thể bấm “Quét ngay” hoặc “Bật lại” ở danh sách dưới.', presetNeedRegion: 'cần bật vùng',
  },
  en: {
    title: 'Sources & topics', subtitle: 'Manage RSS/Atom sources and the topic tree used to file items.', topics: 'Topics', sources: 'Sources',
    notice: 'Enabled sources are only collected automatically while “Automatic collection” above is on. “Fetch now” runs once. Newsman stores titles, links and dates only, never article text.',
    noTopics: 'No topics yet.', noSources: 'No sources yet. Add a public source below.',
    enabled: 'Enabled', disabled: 'Disabled', paused: 'Paused after errors', edit: 'Edit', disable: 'Disable', enable: 'Enable', fetchNow: 'Fetch now', resume: 'Resume', fetching: 'Fetching…',
    addTopic: 'Add topic', editTopic: 'Edit topic', addSource: 'Add source', editSource: 'Edit source',
    slug: 'Identifier', slugHint: 'Lowercase letters, numbers and hyphens.', topicSlugHint: 'Topic identifiers cannot be changed after creation.',
    viName: 'Vietnamese label', enName: 'English label', parent: 'Parent topic', none: 'None',
    sourceName: 'Source name', url: 'RSS / Atom source URL', urlHint: 'Use a public URL without passwords, tokens or API keys.',
    tier: 'Source tier', language: 'Language', region: 'Region', topic: 'Default topic',
    packManaged: 'Managed by a region pack: enable or disable here, edit the content in the pack file.', invalid_region: 'The region does not exist or is disabled. Enable it before enabling the source.', source_managed_by_pack: 'This source is managed by a pack; only enable/disable is allowed.', regionOff: 'off',
    regionDisabled: 'This source’s region is off — it will not be collected until the region is enabled.',
    interval: 'Check interval (minutes)', intervalHint: '5–10,080 minutes. Failing sources are retried less often, at most once a day.',
    active: 'Enable this item', save: 'Save changes', create: 'Add item', cancel: 'Cancel editing', saving: 'Saving…', saved: 'Catalog saved.',
    test: 'Test feed', testing: 'Testing…', previewOk: (n: number) => `Read ${n} items. First titles:`, articles: 'items',
    lastSuccess: 'Last success', lastError: 'Last error', never: 'Not fetched yet', failures: (n: number) => `${n} consecutive failure${n === 1 ? '' : 's'}`,
    runOk: (seen: number, added: number) => `Fetched: ${seen} items in feed, ${added} new.`, runNotModified: 'The feed has not changed since the last check.', runError: 'Fetch failed:', previewError: 'Could not read the feed:',
    duplicate_slug: 'This topic identifier already exists.', duplicate_source: 'The source identifier or URL already exists.',
    topic_in_use: 'Disable active child topics and sources under this topic first.', topic_cycle: 'This parent would create a topic cycle.',
    invalid_parent: 'The parent topic does not exist or is disabled.', invalid_topic: 'The topic is invalid or disabled.',
    invalid_source: 'Invalid source. Check the public URL and interval.', immutable_slug: 'A topic identifier cannot be changed after creation.',
    fetch_in_progress: 'This source is already being fetched. Try again in a few seconds.',
    error: 'Could not load or save the catalog. Check the data and try again.', loading: 'Loading catalog…',
    tier1: 'T1 · Official', tier2: 'T2 · News agency', tier3: 'T3 · Publisher', tier4: 'T4 · Other source',
    presetsTitle: 'Curated starter feeds (1-click)', presetsSub: 'Vetted official and public RSS/Atom feeds. Clicking “Add feed” registers the source disabled with zero network calls; enable its region first, then enable or click “Fetch now”.',
    presetAdd: '+ Add feed', presetAdding: 'Adding…', presetAdded: 'Already added', presetSaved: 'Starter feed added (disabled). Click “Fetch now” or “Enable” in the list below.', presetNeedRegion: 'enable region',
  },
} as const

const reasons: Record<Lang, Record<string, string>> = {
  vi: { blocked_address: 'địa chỉ nội bộ hoặc không công khai bị chặn', invalid_url: 'URL hoặc cổng không được phép', dns_failed: 'không phân giải được tên miền', timeout: 'quá thời gian chờ', network_error: 'lỗi kết nối', response_too_large: 'phản hồi quá 5 MB', too_many_redirects: 'chuyển hướng quá nhiều lần', not_a_feed: 'đây không phải feed RSS/Atom', invalid_feed: 'feed bị lỗi định dạng', not_modified: 'feed chưa thay đổi' },
  en: { blocked_address: 'internal or non-public address blocked', invalid_url: 'URL or port not allowed', dns_failed: 'could not resolve the host', timeout: 'timed out', network_error: 'connection error', response_too_large: 'response over 5 MB', too_many_redirects: 'too many redirects', not_a_feed: 'not an RSS/Atom feed', invalid_feed: 'malformed feed', not_modified: 'feed not modified' },
}
const describe = (lang: Lang, code: string | null) => {
  if (!code) return ''
  if (code.startsWith('http_')) return `HTTP ${code.slice(5)}`
  return reasons[lang][code] ?? code
}

const blankTopic = (): Topic => ({ slug: '', label_vi: '', label_en: '', parent_slug: null, enabled: true })
const blankSource = (): SourceInput => ({ slug: '', name: '', url: '', tier: 'T3', enabled: false, language: '', region: 'global', topic_slug: null, fetch_interval_minutes: 60 })
const sourceInput = (source: Source): SourceInput => ({ slug: source.slug, name: source.name, url: source.url, tier: source.tier, enabled: source.enabled, language: source.language, region: source.region, topic_slug: source.topic_slug, fetch_interval_minutes: source.fetch_interval_minutes })
const formatTime = (lang: Lang, value: string | null) => value ? new Intl.DateTimeFormat(lang === 'vi' ? 'vi-VN' : 'en-GB', { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' }).format(new Date(value)) : null

export function CatalogManager({ lang, api, regions, onChanged }: Props) {
  const t = strings[lang]
  const [tab, setTab] = useState<'topics' | 'sources'>('topics')
  const [topics, setTopics] = useState<Topic[]>([])
  const [sources, setSources] = useState<Source[]>([])
  const [presets, setPresets] = useState<Preset[]>([])
  const [addingPreset, setAddingPreset] = useState('')
  const [topicForm, setTopicForm] = useState<Topic>(blankTopic)
  const [sourceForm, setSourceForm] = useState<SourceInput>(blankSource)
  const [editingTopic, setEditingTopic] = useState(false)
  const [editingSource, setEditingSource] = useState<number | null>(null)
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState(false)
  const [fetchingId, setFetchingId] = useState<number | null>(null)
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const [preview, setPreview] = useState<Preview | null>(null)
  const [previewError, setPreviewError] = useState('')
  const [testing, setTesting] = useState(false)

  const reload = async () => {
    const [topicData, sourceData, presetData] = await Promise.all([
      api<{ data: Topic[] }>('/api/admin/topics'),
      api<{ data: Source[] }>('/api/admin/sources'),
      api<{ data: Preset[] }>('/api/admin/sources/presets').catch(() => ({ data: [] as Preset[] })),
    ])
    setTopics(topicData.data)
    setSources(sourceData.data)
    setPresets(presetData.data)
  }
  useEffect(() => { reload().catch(() => setError('load')).finally(() => setLoading(false)) }, [])

  const addPreset = async (slug: string) => {
    setAddingPreset(slug); clear()
    try {
      await api(`/api/admin/sources/presets/${encodeURIComponent(slug)}/add`, { method: 'POST' })
      await reload(); onChanged(); setNotice(t.presetSaved)
    } catch (err) { fail(err) } finally { setAddingPreset('') }
  }

  const clear = () => { setError(''); setNotice('') }
  const fail = (err: unknown) => { setNotice(''); setError(err instanceof Error ? err.message : 'unknown') }
  const saveTopic = async (event: FormEvent) => {
    event.preventDefault(); setBusy(true); clear()
    try {
      const path = editingTopic ? `/api/admin/topics/${encodeURIComponent(topicForm.slug)}` : '/api/admin/topics'
      await api(path, { method: editingTopic ? 'PUT' : 'POST', body: JSON.stringify(topicForm) })
      setTopicForm(blankTopic()); setEditingTopic(false); await reload(); onChanged(); setNotice(t.saved)
    } catch (err) { fail(err) } finally { setBusy(false) }
  }
  const saveSource = async (event: FormEvent) => {
    event.preventDefault(); setBusy(true); clear()
    try {
      const path = editingSource === null ? '/api/admin/sources' : `/api/admin/sources/${editingSource}`
      await api(path, { method: editingSource === null ? 'POST' : 'PUT', body: JSON.stringify(sourceForm) })
      setSourceForm(blankSource()); setEditingSource(null); setPreview(null); setPreviewError(''); await reload(); onChanged(); setNotice(t.saved)
    } catch (err) { fail(err) } finally { setBusy(false) }
  }
  const toggleTopic = async (topic: Topic) => {
    setBusy(true); clear()
    try {
      await api(`/api/admin/topics/${encodeURIComponent(topic.slug)}`, { method: 'PUT', body: JSON.stringify({ ...topic, enabled: !topic.enabled }) })
      if (editingTopic && topicForm.slug === topic.slug) setTopicForm((current) => ({ ...current, enabled: !topic.enabled }))
      await reload(); onChanged(); setNotice(t.saved)
    } catch (err) { fail(err) } finally { setBusy(false) }
  }
  const toggleSource = async (source: Source) => {
    setBusy(true); clear()
    try {
      await api(`/api/admin/sources/${source.id}`, { method: 'PUT', body: JSON.stringify({ ...sourceInput(source), enabled: !source.enabled }) })
      if (editingSource === source.id) setSourceForm((current) => ({ ...current, enabled: !source.enabled }))
      await reload(); onChanged(); setNotice(t.saved)
    } catch (err) { fail(err) } finally { setBusy(false) }
  }
  const fetchSource = async (source: Source) => {
    setFetchingId(source.id); clear()
    try {
      const { data } = await api<{ data: RunSummary }>(`/api/admin/sources/${source.id}/fetch`, { method: 'POST' })
      if (data.status === 'error') setError(`run:${data.error}`)
      else setNotice(data.status === 'not_modified' ? t.runNotModified : t.runOk(data.items_seen, data.items_new))
      await reload(); onChanged()
    } catch (err) { fail(err) } finally { setFetchingId(null) }
  }
  const resumeSource = async (source: Source) => {
    setBusy(true); clear()
    try { await api(`/api/admin/sources/${source.id}/resume`, { method: 'POST' }); await reload(); onChanged(); setNotice(t.saved) }
    catch (err) { fail(err) } finally { setBusy(false) }
  }
  const testFeed = async () => {
    setTesting(true); setPreview(null); setPreviewError('')
    try {
      const { data } = await api<{ data: Preview }>('/api/admin/sources/preview', { method: 'POST', body: JSON.stringify({ url: sourceForm.url }) })
      setPreview(data)
    } catch (err) {
      const message = err instanceof Error ? err.message : ''
      setPreviewError(message.startsWith('preview_failed:') ? describe(lang, message.slice(15)) : t[message as 'invalid_source'] ?? t.error)
    } finally { setTesting(false) }
  }
  const resetTopic = () => { setTopicForm(blankTopic()); setEditingTopic(false); clear() }
  const resetSource = () => { setSourceForm(blankSource()); setEditingSource(null); setPreview(null); setPreviewError(''); clear() }
  const errorMessage = error.startsWith('run:') ? `${t.runError} ${describe(lang, error.slice(4))}` : Object.prototype.hasOwnProperty.call(t, error) && typeof t[error as keyof typeof t] === 'string' ? t[error as keyof typeof t] as string : t.error
  const regionOptions = (current: string) => (regions?.data ?? []).filter((item) => item.enabled || item.code === current).map((item) => ({ value: item.code, label: `${lang === 'vi' ? item.label_vi : item.label_en}${item.enabled ? '' : ` (${t.regionOff})`}`, hint: item.code }))
  const topicOptions = (current: string | null, exclude?: string) => [{ value: '', label: t.none }, ...topics.filter((item) => (item.enabled || item.slug === current) && item.slug !== exclude).map((item) => ({ value: item.slug, label: `${lang === 'vi' ? item.label_vi : item.label_en}${item.enabled ? '' : ` (${t.disabled})`}`, hint: item.slug }))]

  return <section className="catalog" aria-labelledby="catalog-title">
    <div className="section-heading"><div><span className="eyebrow">{lang === 'vi' ? 'Quản trị danh mục' : 'Catalog administration'}</span><h2 id="catalog-title">{t.title}</h2><p>{t.subtitle}</p></div></div>
    <p className="catalog-notice">{t.notice}</p>
    <div className="catalog-tabs" role="group" aria-label={t.title}>
      <button type="button" aria-pressed={tab === 'topics'} className={tab === 'topics' ? 'active' : ''} onClick={() => { setTab('topics'); clear() }}>{t.topics} <span>{topics.length}</span></button>
      <button type="button" aria-pressed={tab === 'sources'} className={tab === 'sources' ? 'active' : ''} onClick={() => { setTab('sources'); clear() }}>{t.sources} <span>{sources.length}</span></button>
    </div>
    {error && <div className="form-error" role="alert">{errorMessage}</div>}
    {notice && <div className="save-state" role="status">✓ {notice}</div>}
    {loading ? <p className="muted" role="status">{t.loading}</p> : tab === 'topics' ? <div className="catalog-layout">
      <div className="catalog-list" aria-label={t.topics}>
        {topics.length === 0 && <p className="muted">{t.noTopics}</p>}
        {topics.map((topic) => <article className="catalog-entry" key={topic.slug}>
          <div className="catalog-entry-main"><strong>{lang === 'vi' ? topic.label_vi : topic.label_en}</strong><span>{topic.slug}{topic.parent_slug ? ` · ↳ ${topic.parent_slug}` : ''}</span><span className={topic.enabled ? 'catalog-status' : 'catalog-status inactive'}>{topic.enabled ? t.enabled : t.disabled}</span></div>
          <div className="catalog-entry-actions"><button type="button" className="text-button" disabled={busy} onClick={() => { setTopicForm(topic); setEditingTopic(true); clear() }}>{t.edit}</button><button type="button" className="text-button" disabled={busy} onClick={() => void toggleTopic(topic)}>{topic.enabled ? t.disable : t.enable}</button></div>
        </article>)}
      </div>
      <form className="catalog-form" onSubmit={saveTopic}>
        <h3>{editingTopic ? t.editTopic : t.addTopic}</h3>
        <label className="field"><span>{t.slug}</span><input required maxLength={64} pattern="[a-z0-9]+(-[a-z0-9]+)*" disabled={editingTopic} value={topicForm.slug} onChange={(e) => setTopicForm({ ...topicForm, slug: e.target.value })} /><small>{t.slugHint} {t.topicSlugHint}</small></label>
        <label className="field"><span>{t.viName}</span><input required maxLength={120} value={topicForm.label_vi} onChange={(e) => setTopicForm({ ...topicForm, label_vi: e.target.value })} /></label>
        <label className="field"><span>{t.enName}</span><input required maxLength={120} value={topicForm.label_en} onChange={(e) => setTopicForm({ ...topicForm, label_en: e.target.value })} /></label>
        <Select label={t.parent} value={topicForm.parent_slug ?? ''} options={topicOptions(topicForm.parent_slug, topicForm.slug)} onChange={(value) => setTopicForm({ ...topicForm, parent_slug: value || null })} />
        <label className="toggle-field"><span>{t.active}</span><input type="checkbox" checked={topicForm.enabled} onChange={(e) => setTopicForm({ ...topicForm, enabled: e.target.checked })} /></label>
        <div className="catalog-form-actions actions-row"><button className="btn primary" disabled={busy}>{busy ? t.saving : editingTopic ? t.save : t.create}</button>{editingTopic && <button type="button" className="btn secondary" onClick={resetTopic}>{t.cancel}</button>}</div>
      </form>
    </div> : <>
      {presets.length > 0 && <div className="presets-box">
        <div className="presets-head"><h3>{t.presetsTitle}</h3><span className="muted">{presets.filter((p) => p.installed).length}/{presets.length}</span></div>
        <p className="muted" style={{ margin: 0 }}>{t.presetsSub}</p>
        <div className="presets-grid">{presets.map((preset) => <div className="preset-card" key={preset.slug}>
          <div className="preset-main">
            <strong>{preset.name}</strong>
            <span><span className="tier-badge">{preset.tier}</span> · {preset.region}{!preset.region_enabled ? ` (${t.presetNeedRegion})` : ''}{preset.topic_slug ? ` · ${preset.topic_slug}` : ''}</span>
            <small>{preset.attribution}</small>
          </div>
          {preset.installed
            ? <span className="catalog-status">{t.presetAdded}</span>
            : <button type="button" className="btn secondary" disabled={busy || addingPreset !== ''} onClick={() => void addPreset(preset.slug)}>{addingPreset === preset.slug ? t.presetAdding : t.presetAdd}</button>}
        </div>)}</div>
      </div>}
      <div className="catalog-layout">
      <div className="catalog-list" aria-label={t.sources}>
        {sources.length === 0 && <p className="muted">{t.noSources}</p>}
        {sources.map((source) => {
          const status = source.paused_reason ? 'paused' : source.enabled ? 'enabled' : 'disabled'
          return <article className="catalog-entry" key={source.id}>
            <div className="catalog-entry-main"><strong>{source.name}</strong><span className="catalog-url">{source.url}</span><span>{source.tier} · {source.region}{source.topic_slug ? ` · ${source.topic_slug}` : ''} · {source.article_count} {t.articles}</span>
              <span className={status === 'enabled' ? 'catalog-status' : status === 'paused' ? 'catalog-status paused' : 'catalog-status inactive'}>{t[status]}</span>
              {source.pack_key && <span className="catalog-health">{t.packManaged}</span>}
              {source.enabled && regions && !regions.data.some((item) => item.code === source.region && item.enabled) && <span className="catalog-health error" role="status">{t.regionDisabled}</span>}
              <span className="catalog-health">{source.last_success_at ? `${t.lastSuccess}: ${formatTime(lang, source.last_success_at)}` : t.never}</span>
              {source.last_error && <span className="catalog-health error">{t.lastError}: {describe(lang, source.last_error)}{source.consecutive_failures > 0 ? ` · ${t.failures(source.consecutive_failures)}` : ''}</span>}
            </div>
            <div className="catalog-entry-actions">
              {/* Pack content is file-managed, so "fetch now" is the only action
                  beyond enable/disable; hand-added sources keep the full editor. */}
              {!source.pack_key && <button type="button" className="text-button" disabled={busy} onClick={() => { setSourceForm(sourceInput(source)); setEditingSource(source.id); setPreview(null); setPreviewError(''); clear() }}>{t.edit}</button>}
              <button type="button" className="text-button" disabled={busy || fetchingId !== null} onClick={() => void fetchSource(source)}>{fetchingId === source.id ? t.fetching : t.fetchNow}</button>
              {source.paused_reason && <button type="button" className="text-button" disabled={busy} onClick={() => void resumeSource(source)}>{t.resume}</button>}

              <button type="button" className="text-button" disabled={busy} onClick={() => void toggleSource(source)}>{source.enabled ? t.disable : t.enable}</button>
            </div>
          </article>
        })}
      </div>
      <form className="catalog-form" onSubmit={saveSource}>
        <h3>{editingSource === null ? t.addSource : t.editSource}</h3>
        <label className="field"><span>{t.slug}</span><input required maxLength={64} pattern="[a-z0-9]+(-[a-z0-9]+)*" value={sourceForm.slug} onChange={(e) => setSourceForm({ ...sourceForm, slug: e.target.value })} /><small>{t.slugHint}</small></label>
        <label className="field"><span>{t.sourceName}</span><input required maxLength={120} value={sourceForm.name} onChange={(e) => setSourceForm({ ...sourceForm, name: e.target.value })} /></label>
        <label className="field"><span>{t.url}</span><input required type="url" maxLength={2048} placeholder="https://example.org/feed.xml" value={sourceForm.url} onChange={(e) => { setSourceForm({ ...sourceForm, url: e.target.value }); setPreview(null); setPreviewError('') }} /><small>{t.urlHint}</small></label>
        <div className="catalog-form-actions actions-row"><button type="button" className="btn secondary" disabled={testing || !sourceForm.url} onClick={() => void testFeed()}>{testing ? t.testing : t.test}</button></div>
        {previewError && <div className="form-error" role="alert">{t.previewError} {previewError}</div>}
        {preview && <div className="preview-box" role="status"><strong>{t.previewOk(preview.items_seen)}</strong><ol>{preview.sample.map((item) => <li key={item.url}>{item.title}</li>)}</ol></div>}
        <Select label={t.tier} value={sourceForm.tier} options={[{ value: 'T1', label: t.tier1 }, { value: 'T2', label: t.tier2 }, { value: 'T3', label: t.tier3 }, { value: 'T4', label: t.tier4 }]} onChange={(value) => setSourceForm({ ...sourceForm, tier: value })} />
        <div className="catalog-form-row"><label className="field"><span>{t.language}</span><input maxLength={16} placeholder="vi, en…" value={sourceForm.language} onChange={(e) => setSourceForm({ ...sourceForm, language: e.target.value })} /></label><Select label={t.region} value={sourceForm.region} options={regionOptions(sourceForm.region)} onChange={(value) => setSourceForm({ ...sourceForm, region: value })} /></div>
        <Select label={t.topic} value={sourceForm.topic_slug ?? ''} options={topicOptions(sourceForm.topic_slug)} onChange={(value) => setSourceForm({ ...sourceForm, topic_slug: value || null })} />
        <label className="field"><span>{t.interval}</span><input type="number" required min={5} max={10080} step={1} value={sourceForm.fetch_interval_minutes} onChange={(e) => setSourceForm({ ...sourceForm, fetch_interval_minutes: Number(e.target.value) })} /><small>{t.intervalHint}</small></label>
        <label className="toggle-field"><span>{t.active}</span><input type="checkbox" checked={sourceForm.enabled} onChange={(e) => setSourceForm({ ...sourceForm, enabled: e.target.checked })} /></label>
        <div className="catalog-form-actions actions-row"><button className="btn primary" disabled={busy}>{busy ? t.saving : editingSource === null ? t.create : t.save}</button>{editingSource !== null && <button type="button" className="btn secondary" onClick={resetSource}>{t.cancel}</button>}</div>
      </form>
    </div></>}
  </section>
}
