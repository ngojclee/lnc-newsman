import { FormEvent, useCallback, useEffect, useMemo, useState } from 'react'
import { createPortal } from 'react-dom'
import { Select } from './Select'

export type AdminApiFn = <T>(path: string, init?: RequestInit) => Promise<T>

type Lang = 'vi' | 'en'

export interface ApiKeyRow {
  id: string
  name: string
  label: string
  prefix: string
  last4: string
  scopes: string[]
  status: string
  source: string
  env_managed: boolean
  copy_available: boolean
  created_at: number
  updated_at: number
  last_used_at: number
  revoked_at: number
}

export interface MgmtStateInfo {
  configured: boolean
  source: string
  oidc_enabled: boolean
  vault_configured: boolean
  env_pinned: boolean
}

export interface OidcAdminView {
  ok: boolean
  enabled: boolean
  active: boolean
  source: string
  env_managed: boolean
  issuer_url: string
  client_id: string
  has_client_secret: boolean
  client_secret_masked: string
  redirect_uri: string
  default_redirect_uri: string
  scopes: string
  allowed_users: string[]
  allowed_groups: string[]
  allowed_hosts: string[]
}

export interface AuditEventRow {
  id: number
  at: number
  actor: string
  auth_method: string
  client_ip: string
  action: string
  target_type: string
  target_id: string
  detail: Record<string, unknown>
}

export interface ProviderRow {
  id: string
  name: string
  kind: string
  base_url: string
  capabilities: string[]
  enabled: boolean
  status: string
  is_default: boolean
  configured: boolean
  has_secret: boolean
  key_last4: string
  copy_available: boolean
  source: string
  env_managed: boolean
  model_count: number
  consumer_refs: number
  created_at: string
  updated_at: string
}

export interface ModelCatalogRow {
  model_id: string
  display_name: string
  provider: string
  provider_id?: string
  capabilities: string[]
  context_window: number | null
  enabled: boolean
  source: string
  last_fetched_at: string
}

export interface McpConnectorRow {
  id: string
  name: string
  direction: 'inbound' | 'outbound' | string
  endpoint_url: string
  audience: string
  scopes: string[]
  enabled: boolean
  status: string
  bearer_configured?: boolean
  bearer_source?: string
  bearer_copy_available?: boolean
  delegation_configured?: boolean
  delegation_source?: string
  delegation_copy_available?: boolean
  has_secret?: boolean
  key_last4?: string
  copy_available: boolean
  env_managed: boolean
  source: string
  tool_count?: number
  created_at?: number
  updated_at?: number
  revoked_at?: number
}

interface AgentWriteRow {
  id: number
  at: number
  client_id: string | null
  app_id: string | null
  tool: string
  record_kind: string
  record_id: number | null
  outcome: string
  note: string | null
}

interface AgentAnalysisRow {
  id: number
  title: string
  kind: string
  topic_slug: string | null
  region_code: string | null
  author: string | null
  updated_at: number
  source_article_ids: number[]
}

interface BridgeConfigSnap {
  astraea_url?: string
  has_astraea_token?: boolean
}

interface TelegramConfigSnap {
  mode?: string
  cygnus_url?: string
  has_bot_token?: boolean
  has_webhook_secret?: boolean
  has_cygnus_key?: boolean
}

const MASK_SENTINEL = '********'

export async function copyText(
  value: string,
  onFallbackReveal?: () => void,
): Promise<boolean> {
  if (!value) return false
  try {
    if (window.isSecureContext && navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(value)
      return true
    }
  } catch {
    // Fall back to hidden textarea selection on plain HTTP LAN endpoints.
  }
  try {
    const area = document.createElement('textarea')
    area.value = value
    area.setAttribute('readonly', '')
    area.style.position = 'fixed'
    area.style.top = '-9999px'
    area.style.left = '-9999px'
    area.style.opacity = '0'
    document.body.appendChild(area)
    area.focus()
    area.select()
    const ok = document.execCommand('copy')
    document.body.removeChild(area)
    if (!ok) onFallbackReveal?.()
    return ok
  } catch {
    onFallbackReveal?.()
    return false
  }
}

function omitBlankOrMasked(raw: string): string | undefined {
  const trimmed = raw.trim()
  if (!trimmed || trimmed === MASK_SENTINEL || /^[•*]{4,}$/.test(trimmed)) {
    return undefined
  }
  return trimmed
}

function parseCommaList(raw: string): string[] {
  return raw
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean)
}

function formatTs(lang: Lang, sec: number): string {
  if (!sec) return '—'
  return new Intl.DateTimeFormat(lang === 'vi' ? 'vi-VN' : 'en-GB', {
    day: 'numeric',
    month: 'short',
    hour: '2-digit',
    minute: '2-digit',
  }).format(new Date(sec * 1000))
}

export function FloatingToast({
  notice,
  error,
  onDismissNotice,
  onDismissError,
}: {
  notice?: string
  error?: string
  onDismissNotice?: () => void
  onDismissError?: () => void
}) {
  useEffect(() => {
    if (!notice || !onDismissNotice) return
    const timer = window.setTimeout(onDismissNotice, 4500)
    return () => window.clearTimeout(timer)
  }, [notice, onDismissNotice])

  if (!notice && !error) return null
  if (typeof document === 'undefined') return null

  return createPortal(
    <div className="bp-toast-stack" aria-live="polite">
      {error && (
        <div className="bp-toast bp-toast--error" role="alert">
          <span>{error}</span>
          {onDismissError && (
            <button type="button" className="bp-toast-close" onClick={onDismissError} aria-label="Dismiss">
              ×
            </button>
          )}
        </div>
      )}
      {notice && (
        <div className="bp-toast bp-toast--ok" role="status">
          <span>✓ {notice}</span>
          {onDismissNotice && (
            <button type="button" className="bp-toast-close" onClick={onDismissNotice} aria-label="Dismiss">
              ×
            </button>
          )}
        </div>
      )}
    </div>,
    document.body,
  )
}

function SecretCopySlot({
  lang,
  item,
  onDismiss,
  onCopied,
}: {
  lang: Lang
  item: { label: string; secret: string } | null
  onDismiss: () => void
  onCopied: (msg: string) => void
}) {
  const [revealPlain, setRevealPlain] = useState(false)

  useEffect(() => {
    setRevealPlain(false)
  }, [item?.secret])

  return (
    <div
      id="secret-copy-slot"
      className={item ? 'secret-copy-slot is-populated' : 'secret-copy-slot'}
      role="status"
      aria-live="polite"
    >
      {item ? (
        <div className="secret-copy-slot-inner">
          <div className="secret-copy-slot-meta">
            <strong>{item.label}:</strong>
            <code className="secret-copy-code">
              {revealPlain
                ? item.secret
                : item.secret.length > 14
                  ? `${item.secret.slice(0, 8)}••••••••${item.secret.slice(-4)}`
                  : '••••••••••••'}
            </code>
          </div>
          <div className="bp-row-actions">
            <button
              type="button"
              className="btn secondary bp-action-btn"
              onClick={() => setRevealPlain((v) => !v)}
            >
              {revealPlain ? (lang === 'vi' ? 'Ẩn' : 'Hide') : lang === 'vi' ? 'Hiện' : 'Reveal'}
            </button>
            <button
              type="button"
              className="btn primary bp-action-btn"
              onClick={() =>
                void copyText(item.secret, () => setRevealPlain(true)).then((ok) => {
                  onCopied(
                    ok
                      ? lang === 'vi'
                        ? 'Đã sao chép vào clipboard.'
                        : 'Copied secret to clipboard.'
                      : lang === 'vi'
                        ? 'Đã hiển thị khóa để sao chép thủ công.'
                        : 'Revealed secret for manual copy.',
                  )
                })
              }
            >
              {lang === 'vi' ? 'Sao chép' : 'Copy'}
            </button>
            <button type="button" className="btn secondary bp-action-btn" onClick={onDismiss}>
              {lang === 'vi' ? 'Đóng' : 'Clear'}
            </button>
          </div>
        </div>
      ) : (
        <div className="secret-copy-slot-idle">
          <span>
            {lang === 'vi'
              ? 'Vùng sao chép bảo mật sẵn sàng — bấm Tạo mới, Xoay vòng hoặc Sao chép trên từng dòng để nhận khóa.'
              : 'Secure copy slot ready — Create, Rotate, or click Copy on any credential row to populate.'}
          </span>
        </div>
      )}
    </div>
  )
}

function ConfirmRemoveModal({
  open,
  title,
  message,
  warning,
  confirmLabel,
  cancelLabel,
  busy,
  onConfirm,
  onClose,
}: {
  open: boolean
  title: string
  message: string
  warning?: string
  confirmLabel: string
  cancelLabel: string
  busy: boolean
  onConfirm: () => void
  onClose: () => void
}) {
  useEffect(() => {
    if (!open) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && !busy) onClose()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [open, busy, onClose])

  if (!open) return null
  return (
    <div
      className="bp-modal-backdrop"
      role="dialog"
      aria-modal="true"
      aria-labelledby="bp-modal-title"
      onClick={(e) => {
        if (e.target === e.currentTarget && !busy) onClose()
      }}
    >
      <div className="bp-modal-card">
        <h3 id="bp-modal-title" className="bp-card-title">
          {title}
        </h3>
        <p style={{ margin: 0, lineHeight: 1.55 }}>{message}</p>
        {warning && (
          <div className="form-error" role="alert" style={{ margin: 0 }}>
            {warning}
          </div>
        )}
        <div className="btn-row" style={{ justifyContent: 'flex-end' }}>
          <button
            type="button"
            className="btn secondary"
            disabled={busy}
            onClick={onClose}
          >
            {cancelLabel}
          </button>
          <button
            type="button"
            className="btn secondary bp-action-danger"
            disabled={busy}
            onClick={onConfirm}
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  )
}

function ActionBar({
  lang,
  copyAvailable,
  envManaged,
  isRevoked,
  removeDisabled,
  removeTooltip,
  showRevoke = true,
  busy,
  onCopy,
  onRotate,
  onRevoke,
  onRemove,
}: {
  lang: Lang
  copyAvailable: boolean
  envManaged: boolean
  isRevoked?: boolean
  removeDisabled?: boolean
  removeTooltip?: string
  showRevoke?: boolean
  busy?: boolean
  onCopy: () => void
  onRotate: () => void
  onRevoke?: () => void
  onRemove: () => void
}) {
  const labels =
    lang === 'vi'
      ? {
          copy: 'Sao chép',
          rotate: 'Xoay vòng',
          revoke: isRevoked ? 'Kích hoạt lại' : 'Thu hồi',
          remove: 'Xóa',
          rotateToCopy: 'Xoay vòng để bật Sao chép',
        }
      : {
          copy: 'Copy',
          rotate: 'Rotate',
          revoke: isRevoked ? 'Reactivate' : 'Revoke',
          remove: 'Remove',
          rotateToCopy: 'Rotate to enable Copy',
        }

  const copyTitle = copyAvailable ? labels.copy : labels.rotateToCopy

  return (
    <div className="bp-row-actions">
      <button
        type="button"
        className="btn secondary bp-action-btn"
        disabled={busy || envManaged || isRevoked || !copyAvailable}
        title={copyTitle}
        onClick={onCopy}
      >
        {labels.copy}
      </button>
      <button
        type="button"
        className="btn secondary bp-action-btn"
        disabled={busy || envManaged}
        title={labels.rotate}
        onClick={onRotate}
      >
        {labels.rotate}
      </button>
      {showRevoke && onRevoke && (
        <button
          type="button"
          className="btn secondary bp-action-btn"
          disabled={busy || envManaged}
          title={labels.revoke}
          onClick={onRevoke}
        >
          {labels.revoke}
        </button>
      )}
      <button
        type="button"
        className="btn secondary bp-action-btn bp-action-danger"
        disabled={busy || envManaged || removeDisabled}
        title={removeTooltip || labels.remove}
        onClick={onRemove}
      >
        {labels.remove}
      </button>
    </div>
  )
}

export function AuditActivityCard({
  lang,
  api,
  initialEvents,
}: {
  lang: Lang
  api: AdminApiFn
  initialEvents?: AuditEventRow[]
}) {
  const [events, setEvents] = useState<AuditEventRow[]>(initialEvents ?? [])

  useEffect(() => {
    if (initialEvents) {
      setEvents(initialEvents)
      return
    }
    api<{ ok: boolean; items: AuditEventRow[] }>('/api/admin/audit')
      .then((res) => setEvents(res.items ?? []))
      .catch(() => setEvents([]))
  }, [api, initialEvents])

  return (
    <section className="bp-card">
      <div className="bp-card-head">
        <div>
          <span className="eyebrow">{lang === 'vi' ? 'Nhật ký hoạt động' : 'Audit Trail'}</span>
          <h3 className="bp-card-title" style={{ fontSize: 18 }}>
            {lang === 'vi' ? 'Hoạt động quản trị gần đây' : 'Recent Activity Log'}
          </h3>
        </div>
      </div>
      <div className="bp-table-wrap">
        <table className="bp-table" id="audit-activity-table">
          <thead>
            <tr>
              <th>{lang === 'vi' ? 'Thời gian' : 'Time'}</th>
              <th>{lang === 'vi' ? 'Tác nhân' : 'Actor'}</th>
              <th>{lang === 'vi' ? 'Hành động' : 'Action'}</th>
              <th>{lang === 'vi' ? 'Mục tiêu' : 'Target'}</th>
              <th>IP</th>
            </tr>
          </thead>
          <tbody>
            {events.length === 0 ? (
              <tr>
                <td colSpan={5} className="muted">
                  {lang === 'vi' ? 'Chưa có sự kiện nào.' : 'No activity recorded yet.'}
                </td>
              </tr>
            ) : (
              events.slice(0, 20).map((ev) => (
                <tr key={ev.id}>
                  <td>{formatTs(lang, ev.at)}</td>
                  <td>
                    <strong>{ev.actor}</strong> <small className="muted">({ev.auth_method})</small>
                  </td>
                  <td>
                    <code>{ev.action}</code>
                  </td>
                  <td>
                    {ev.target_type}:{ev.target_id}
                  </td>
                  <td>
                    <code>{ev.client_ip}</code>
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
    </section>
  )
}

export function ApiKeysPanel({
  lang,
  api,
}: {
  lang: Lang
  api: AdminApiFn
}) {
  const [items, setItems] = useState<ApiKeyRow[] | null>(null)
  const [mgmt, setMgmt] = useState<MgmtStateInfo | null>(null)
  const [events, setEvents] = useState<AuditEventRow[]>([])
  const [oidc, setOidc] = useState<OidcAdminView | null>(null)
  const [oidcEnabled, setOidcEnabled] = useState(false)
  const [oidcIssuer, setOidcIssuer] = useState('')
  const [oidcClientId, setOidcClientId] = useState('')
  const [oidcSecret, setOidcSecret] = useState('')
  const [oidcRedirect, setOidcRedirect] = useState('')
  const [oidcScopes, setOidcScopes] = useState('openid profile email')
  const [oidcUsers, setOidcUsers] = useState('')
  const [oidcGroups, setOidcGroups] = useState('')
  const [oidcHosts, setOidcHosts] = useState('')
  const [oidcProbeStatus, setOidcProbeStatus] = useState('')

  const [busy, setBusy] = useState(false)
  const [notice, setNotice] = useState('')
  const [error, setError] = useState('')
  const [revealed, setRevealed] = useState<{ label: string; secret: string } | null>(null)
  const [newLabel, setNewLabel] = useState('')
  const [scopeRead, setScopeRead] = useState(true)
  const [scopeWrite, setScopeWrite] = useState(false)
  const [customSecret, setCustomSecret] = useState('')
  const [customMgmtKey, setCustomMgmtKey] = useState('')
  const [removeTarget, setRemoveTarget] = useState<ApiKeyRow | null>(null)
  const [revokeAllOpen, setRevokeAllOpen] = useState(false)

  const syncOidcForm = useCallback((view: OidcAdminView) => {
    setOidc(view)
    setOidcEnabled(Boolean(view.enabled))
    setOidcIssuer(view.issuer_url || '')
    setOidcClientId(view.client_id || '')
    setOidcSecret('')
    setOidcRedirect(view.redirect_uri || view.default_redirect_uri || '')
    setOidcScopes(view.scopes || 'openid profile email')
    setOidcUsers((view.allowed_users ?? []).join(', '))
    setOidcGroups((view.allowed_groups ?? []).join(', '))
    setOidcHosts((view.allowed_hosts ?? []).join(', '))
  }, [])

  const load = useCallback(async () => {
    try {
      const [keysRes, auditRes, oidcRes] = await Promise.all([
        api<{ ok: boolean; items: ApiKeyRow[]; mgmt: MgmtStateInfo }>('/api/admin/api-keys'),
        api<{ ok: boolean; items: AuditEventRow[] }>('/api/admin/audit'),
        api<OidcAdminView>('/api/admin/oidc'),
      ])
      setItems(keysRes.items ?? [])
      setMgmt(keysRes.mgmt ?? null)
      setEvents(auditRes.items ?? [])
      if (oidcRes) syncOidcForm(oidcRes)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load API keys')
      setItems([])
    }
  }, [api, syncOidcForm])

  useEffect(() => {
    void load()
  }, [load])

  const saveOidc = async (e: FormEvent) => {
    e.preventDefault()
    setBusy(true)
    setError('')
    setNotice('')
    try {
      const cleanedSecret = omitBlankOrMasked(oidcSecret)
      const res = await api<OidcAdminView>('/api/admin/oidc', {
        method: 'PUT',
        body: JSON.stringify({
          enabled: oidcEnabled,
          issuer_url: oidcIssuer.trim(),
          client_id: oidcClientId.trim(),
          client_secret: cleanedSecret ?? MASK_SENTINEL,
          redirect_uri: oidcRedirect.trim(),
          scopes: oidcScopes.trim() || 'openid profile email',
          allowed_users: parseCommaList(oidcUsers),
          allowed_groups: parseCommaList(oidcGroups),
          allowed_hosts: parseCommaList(oidcHosts),
        }),
      })
      syncOidcForm(res)
      setNotice(
        lang === 'vi'
          ? 'Đã lưu & áp dụng cấu hình Keycloak SSO trực tiếp.'
          : 'Saved and hot-reloaded Keycloak SSO configuration.',
      )
      await load()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to save SSO configuration')
    } finally {
      setBusy(false)
    }
  }

  const testOidcDiscovery = async () => {
    setBusy(true)
    setError('')
    setNotice('')
    try {
      const res = await api<{ ok: boolean; issuer: string; jwks_keys: number }>(
        '/api/admin/oidc/test',
        {
          method: 'POST',
          body: JSON.stringify({
            issuer_url: oidcIssuer.trim(),
            allowed_hosts: parseCommaList(oidcHosts),
          }),
        },
      )
      const summary = `${res.issuer} · ${res.jwks_keys} JWKS key(s)`
      setOidcProbeStatus(summary)
      setNotice(
        lang === 'vi'
          ? `Kết nối OIDC thành công (${summary}).`
          : `OIDC discovery verified (${summary}).`,
      )
    } catch (err) {
      setError(err instanceof Error ? err.message : 'OIDC discovery test failed')
    } finally {
      setBusy(false)
    }
  }

  const resetOidc = async () => {
    setBusy(true)
    setError('')
    setNotice('')
    try {
      const res = await api<OidcAdminView>('/api/admin/oidc/reset', {
        method: 'POST',
        body: JSON.stringify({}),
      })
      syncOidcForm(res)
      setOidcProbeStatus('')
      setNotice(
        lang === 'vi'
          ? 'Đã khôi phục cấu hình SSO mặc định.'
          : 'Reset SSO settings to default.',
      )
      await load()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Reset failed')
    } finally {
      setBusy(false)
    }
  }

  const createKey = async (e: FormEvent) => {
    e.preventDefault()
    if (!newLabel.trim()) return
    const scopes: string[] = []
    if (scopeRead) scopes.push('newsman:read')
    if (scopeWrite) scopes.push('newsman:write')
    if (scopes.length === 0) scopes.push('newsman:read')
    setBusy(true)
    setError('')
    setNotice('')
    try {
      const res = await api<{ ok: boolean; secret?: string; key?: string }>('/api/admin/api-keys', {
        method: 'POST',
        body: JSON.stringify({
          label: newLabel.trim(),
          scopes,
          ...(customSecret.trim() ? { custom_secret: customSecret.trim() } : {}),
        }),
      })
      const createdLabel = newLabel.trim()
      setNewLabel('')
      setCustomSecret('')
      const sec = res.secret ?? res.key ?? ''
      if (sec) {
        setRevealed({ label: createdLabel, secret: sec })
        void copyText(sec)
      }
      setNotice(lang === 'vi' ? 'Đã tạo API key.' : 'API key created.')
      await load()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Create failed')
    } finally {
      setBusy(false)
    }
  }

  const copyRow = async (row: ApiKeyRow) => {
    setBusy(true)
    setError('')
    setNotice('')
    try {
      const res = await api<{ ok: boolean; secret: string }>(
        `/api/admin/api-keys/${encodeURIComponent(row.id)}/copy`,
        { method: 'POST' },
      )
      setRevealed({ label: row.label, secret: res.secret })
      const ok = await copyText(res.secret)
      setNotice(
        ok
          ? lang === 'vi'
            ? `Đã sao chép khóa ${row.label}.`
            : `Copied ${row.label} to clipboard.`
          : lang === 'vi'
            ? `Đã nạp khóa ${row.label} vào vùng sao chép bảo mật.`
            : `Loaded ${row.label} into the secure copy slot.`,
      )
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Copy failed')
    } finally {
      setBusy(false)
    }
  }

  const rotateRow = async (row: ApiKeyRow) => {
    setBusy(true)
    setError('')
    setNotice('')
    try {
      const res = await api<{ ok: boolean; secret: string }>(
        `/api/admin/api-keys/${encodeURIComponent(row.id)}/rotate`,
        { method: 'POST', body: JSON.stringify({}) },
      )
      setRevealed({ label: `${row.label} (rotated)`, secret: res.secret })
      void copyText(res.secret)
      setNotice(lang === 'vi' ? `Đã xoay vòng ${row.label}.` : `Rotated ${row.label}.`)
      await load()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Rotate failed')
    } finally {
      setBusy(false)
    }
  }

  const toggleRevokeRow = async (row: ApiKeyRow) => {
    setBusy(true)
    setError('')
    setNotice('')
    try {
      if (row.status === 'revoked') {
        await api(`/api/admin/api-keys/${encodeURIComponent(row.id)}`, {
          method: 'PATCH',
          body: JSON.stringify({ status: 'active' }),
        })
        setNotice(lang === 'vi' ? `Đã kích hoạt lại ${row.label}.` : `Reactivated ${row.label}.`)
      } else {
        await api(`/api/admin/api-keys/${encodeURIComponent(row.id)}/revoke`, {
          method: 'POST',
        })
        setNotice(lang === 'vi' ? `Đã thu hồi ${row.label}.` : `Revoked ${row.label}.`)
      }
      await load()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Action failed')
    } finally {
      setBusy(false)
    }
  }

  const confirmRemove = async () => {
    if (!removeTarget) return
    setBusy(true)
    setError('')
    try {
      await api(`/api/admin/api-keys/${encodeURIComponent(removeTarget.id)}`, {
        method: 'DELETE',
      })
      setNotice(lang === 'vi' ? `Đã xóa khóa ${removeTarget.label}.` : `Removed ${removeTarget.label}.`)
      setRemoveTarget(null)
      await load()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Remove failed')
    } finally {
      setBusy(false)
    }
  }

  const rotateMgmtKey = async () => {
    setBusy(true)
    setError('')
    setNotice('')
    try {
      const res = await api<{ ok: boolean; secret: string }>(
        '/api/admin/mgmt/rotate',
        {
          method: 'POST',
          body: JSON.stringify(customMgmtKey.trim() ? { new_key: customMgmtKey.trim() } : {}),
        },
      )
      setCustomMgmtKey('')
      setRevealed({
        label: lang === 'vi' ? 'Khóa quản trị khẩn cấp' : 'Break-Glass Admin Key',
        secret: res.secret,
      })
      void copyText(res.secret)
      setNotice(
        lang === 'vi'
          ? 'Đã xoay vòng khóa quản trị khẩn cấp.'
          : 'Rotated break-glass admin key.',
      )
      await load()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Rotate admin key failed')
    } finally {
      setBusy(false)
    }
  }

  const confirmRevokeAll = async () => {
    setBusy(true)
    setError('')
    try {
      await api('/api/admin/sessions/revoke-all', { method: 'POST' })
      setRevokeAllOpen(false)
      window.dispatchEvent(new Event('newsman:session-expired'))
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Revoke all sessions failed')
      setBusy(false)
    }
  }

  return (
    <div className="bp-panel-grid">
      <FloatingToast
        notice={notice}
        error={error}
        onDismissNotice={() => setNotice('')}
        onDismissError={() => setError('')}
      />

      <SecretCopySlot
        lang={lang}
        item={revealed}
        onDismiss={() => setRevealed(null)}
        onCopied={setNotice}
      />

      <section className="bp-card" id="oidc-sso-card">
        <div className="bp-card-head">
          <div>
            <span className="eyebrow">Single Sign-On · Keycloak OIDC</span>
            <h2 className="bp-card-title">
              {lang === 'vi' ? 'Đăng nhập một lần Keycloak SSO' : 'Keycloak Single Sign-On (OIDC)'}
            </h2>
            <p className="bp-card-sub">
              {lang === 'vi'
                ? 'Cấu hình đăng nhập SSO trực tiếp không cần khởi động lại. Bỏ trống Client Secret khi lưu để giữ nguyên bí mật đã lưu.'
                : 'Configure live Keycloak SSO sign-in without restarting. Leave Client Secret blank when saving to keep the stored secret.'}
            </p>
          </div>
          <div className="chip-row">
            <span className={oidc?.active ? 'bp-chip ok' : 'bp-chip muted'}>
              {oidc?.active ? 'SSO Active' : 'SSO Disabled'}
            </span>
            {oidc?.source && <span className="bp-chip">{oidc.source}</span>}
            {oidcProbeStatus && <span className="bp-chip ok">{oidcProbeStatus}</span>}
          </div>
        </div>

        <form className="bp-form-grid" onSubmit={saveOidc} autoComplete="off">
          <div className="switch-row" style={{ gridColumn: '1 / -1' }}>
            <div>
              <strong>
                {oidcEnabled
                  ? lang === 'vi'
                    ? 'Bật đăng nhập qua Keycloak SSO'
                    : 'Enable Keycloak SSO Sign-In'
                  : lang === 'vi'
                    ? 'Tắt đăng nhập qua Keycloak SSO'
                    : 'Keycloak SSO Sign-In Disabled'}
              </strong>
              <span>
                {lang === 'vi'
                  ? 'Yêu cầu ít nhất một người dùng hoặc nhóm được phép truy cập (không chấp nhận ký tự đại diện *).'
                  : 'Requires at least one explicit allowed user or group (wildcards are rejected).'}
              </span>
            </div>
            <button
              type="button"
              role="switch"
              className="switch"
              aria-checked={oidcEnabled}
              aria-label="Enable Keycloak SSO"
              disabled={busy}
              onClick={() => setOidcEnabled((v) => !v)}
            />
          </div>

          <label className="field">
            <span>Issuer URL</span>
            <input
              value={oidcIssuer}
              placeholder="https://sso.lengoc.me/realms/lengoc"
              onChange={(e) => setOidcIssuer(e.target.value)}
            />
          </label>

          <label className="field">
            <span>Client ID</span>
            <input
              value={oidcClientId}
              placeholder="lnc-newsman"
              onChange={(e) => setOidcClientId(e.target.value)}
            />
          </label>

          <label className="field">
            <span>
              {lang === 'vi'
                ? 'Client Secret (bỏ trống để giữ khóa cũ)'
                : 'Client Secret (blank keeps stored secret)'}
            </span>
            <input
              type="password"
              autoComplete="new-password"
              value={oidcSecret}
              placeholder={
                oidc?.has_client_secret
                  ? `${MASK_SENTINEL} (${lang === 'vi' ? 'đã lưu' : 'saved'})`
                  : lang === 'vi'
                    ? 'Nhập Client Secret'
                    : 'Enter client secret'
              }
              onChange={(e) => setOidcSecret(e.target.value)}
            />
          </label>

          <label className="field">
            <span>Redirect URI</span>
            <div style={{ display: 'flex', gap: 6, alignItems: 'center' }}>
              <input
                style={{ flex: 1, minWidth: 0 }}
                value={oidcRedirect}
                placeholder={oidc?.default_redirect_uri || 'https://newsman.lengoc.me/api/auth/oidc/callback'}
                onChange={(e) => setOidcRedirect(e.target.value)}
              />
              <button
                type="button"
                className="btn secondary bp-action-btn"
                onClick={() =>
                  void copyText(oidcRedirect || oidc?.default_redirect_uri || '').then((ok) => {
                    if (ok) {
                      setNotice(
                        lang === 'vi' ? 'Đã chép Redirect URI.' : 'Copied Redirect URI.',
                      )
                    }
                  })
                }
              >
                {lang === 'vi' ? 'Chép' : 'Copy'}
              </button>
            </div>
          </label>

          <label className="field">
            <span>Scopes</span>
            <input
              value={oidcScopes}
              placeholder="openid profile email"
              onChange={(e) => setOidcScopes(e.target.value)}
            />
          </label>

          <label className="field">
            <span>{lang === 'vi' ? 'Người dùng cho phép (phẩy)' : 'Allowed Users (comma-separated)'}</span>
            <input
              value={oidcUsers}
              placeholder="louis, admin@lengoc.me"
              onChange={(e) => setOidcUsers(e.target.value)}
            />
          </label>

          <label className="field">
            <span>{lang === 'vi' ? 'Nhóm / Role cho phép (tuỳ chọn)' : 'Allowed Groups / Roles (optional)'}</span>
            <input
              value={oidcGroups}
              placeholder="lnc-admins"
              onChange={(e) => setOidcGroups(e.target.value)}
            />
          </label>

          <label className="field">
            <span>{lang === 'vi' ? 'Host nội bộ tin cậy (tuỳ chọn)' : 'Trusted Internal IdP Hosts (optional)'}</span>
            <input
              value={oidcHosts}
              placeholder="sso.lengoc.me"
              onChange={(e) => setOidcHosts(e.target.value)}
            />
          </label>

          <div className="btn-row" style={{ gridColumn: '1 / -1', justifyContent: 'flex-end' }}>
            <button
              type="button"
              className="btn secondary"
              disabled={busy || !oidcIssuer.trim()}
              onClick={() => void testOidcDiscovery()}
            >
              {lang === 'vi' ? 'Kiểm tra kết nối' : 'Test Discovery'}
            </button>
            <button
              type="button"
              className="btn secondary"
              disabled={busy}
              onClick={() => void resetOidc()}
            >
              {lang === 'vi' ? 'Đặt lại' : 'Reset'}
            </button>
            <button type="submit" className="btn primary" disabled={busy}>
              {lang === 'vi' ? 'Lưu SSO' : 'Save SSO'}
            </button>
          </div>
        </form>
      </section>

      <section className="bp-card">
        <div className="bp-card-head">
          <div>
            <span className="eyebrow">{lang === 'vi' ? 'Truy cập khẩn cấp' : 'Break-Glass Access'}</span>
            <h2 className="bp-card-title">
              {lang === 'vi' ? 'Phiên đăng nhập & Khóa quản trị' : 'Active Sessions & Break-Glass Admin Key'}
            </h2>
            <p className="bp-card-sub">
              {lang === 'vi'
                ? 'Xoay vòng khóa quản trị hoặc thu hồi toàn bộ phiên sẽ đăng xuất tức thì mọi trình duyệt cũ.'
                : 'Rotating the break-glass admin key or revoking sessions immediately signs out prior browser sessions.'}
            </p>
          </div>
          {mgmt && (
            <div className="chip-row">
              <span className={mgmt.oidc_enabled ? 'bp-chip ok' : 'bp-chip muted'}>
                SSO: {mgmt.oidc_enabled ? 'ON' : 'OFF'}
              </span>
              <span className={mgmt.configured ? 'bp-chip ok' : 'bp-chip warn'}>
                {mgmt.source}
              </span>
            </div>
          )}
        </div>

        <div className="catalog-form-row" style={{ alignItems: 'end' }}>
          <label className="field">
            <span>{lang === 'vi' ? 'Khóa mới tuỳ chọn (≥16 ký tự)' : 'Optional custom key (≥16 chars)'}</span>
            <input
              type="password"
              autoComplete="new-password"
              value={customMgmtKey}
              placeholder={lang === 'vi' ? 'Bỏ trống để tự sinh khóa mới…' : 'Leave blank to auto-generate…'}
              onChange={(e) => setCustomMgmtKey(e.target.value)}
            />
          </label>
          <div className="btn-row">
            <button
              type="button"
              className="btn secondary"
              disabled={busy}
              onClick={() => void rotateMgmtKey()}
            >
              {lang === 'vi' ? 'Xoay vòng khóa quản trị' : 'Rotate Admin Key'}
            </button>
            <button
              type="button"
              className="btn secondary bp-action-danger"
              disabled={busy}
              onClick={() => setRevokeAllOpen(true)}
            >
              {lang === 'vi' ? 'Thu hồi mọi phiên' : 'Revoke all sessions'}
            </button>
          </div>
        </div>
      </section>

      <section className="bp-card">
        <div className="bp-card-head">
          <div>
            <span className="eyebrow">Scoped Bearer Credentials</span>
            <h2 className="bp-card-title">{lang === 'vi' ? 'Khóa API truy cập' : 'Inbound API Keys'}</h2>
            <p className="bp-card-sub">
              {lang === 'vi'
                ? 'Khóa API dùng để đọc/ghi qua /api/v1/*. Mọi thao tác Sao chép đều được ghi nhật ký và giới hạn tốc độ.'
                : 'API keys grant scoped Bearer access to /api/v1/*. Copy actions are rate-limited and recorded in the activity log.'}
            </p>
          </div>
        </div>

        <form className="bp-form-grid" onSubmit={createKey} autoComplete="off">
          <label className="field">
            <span>{lang === 'vi' ? 'Tên nhãn khóa' : 'Key label'}</span>
            <input
              required
              maxLength={80}
              value={newLabel}
              placeholder="CLI / n8n / Reader Digest"
              onChange={(e) => setNewLabel(e.target.value)}
            />
          </label>
          <label className="field">
            <span>{lang === 'vi' ? 'Khóa tuỳ chọn (tuỳ chọn, ≥16 ký tự)' : 'Custom secret (optional, ≥16 chars)'}</span>
            <input
              type="password"
              autoComplete="new-password"
              value={customSecret}
              placeholder={lang === 'vi' ? 'Tự sinh nếu để trống' : 'Auto-generated if blank'}
              onChange={(e) => setCustomSecret(e.target.value)}
            />
          </label>
          <div className="chip-row" style={{ alignItems: 'center' }}>
            <button
              type="button"
              className={scopeRead ? 'chip active' : 'chip'}
              onClick={() => setScopeRead((v) => !v)}
            >
              {scopeRead ? '✓ ' : ''}newsman:read
            </button>
            <button
              type="button"
              className={scopeWrite ? 'chip active' : 'chip'}
              onClick={() => setScopeWrite((v) => !v)}
            >
              {scopeWrite ? '✓ ' : ''}newsman:write
            </button>
          </div>
          <div className="btn-row" style={{ justifyContent: 'flex-end' }}>
            <button type="submit" className="btn primary" disabled={busy || !newLabel.trim()}>
              {lang === 'vi' ? '+ Tạo API Key' : '+ Create API Key'}
            </button>
          </div>
        </form>

        <div className="bp-table-wrap">
          <table className="bp-table">
            <thead>
              <tr>
                <th>{lang === 'vi' ? 'Nhãn' : 'Label'}</th>
                <th>{lang === 'vi' ? 'Mã khóa' : 'Prefix / Last4'}</th>
                <th>Scopes</th>
                <th>{lang === 'vi' ? 'Trạng thái' : 'Status'}</th>
                <th>{lang === 'vi' ? 'Lần dùng cuối' : 'Last used'}</th>
                <th>{lang === 'vi' ? 'Thao tác' : 'Actions'}</th>
              </tr>
            </thead>
            <tbody>
              {items === null ? (
                <tr>
                  <td colSpan={6} className="muted">Loading…</td>
                </tr>
              ) : items.length === 0 ? (
                <tr>
                  <td colSpan={6} className="muted">
                    {lang === 'vi' ? 'Chưa có API key nào.' : 'No API keys configured yet.'}
                  </td>
                </tr>
              ) : (
                items.map((row) => (
                  <tr key={row.id}>
                    <td>
                      <strong>{row.label}</strong>
                    </td>
                    <td>
                      <code>{row.prefix}…{row.last4}</code>
                    </td>
                    <td>
                      <div className="chip-row">
                        {row.scopes.map((sc) => (
                          <span key={sc} className="bp-chip">{sc}</span>
                        ))}
                      </div>
                    </td>
                    <td>
                      <span className={row.status === 'active' ? 'bp-chip ok' : 'bp-chip warn'}>
                        {row.status}
                      </span>
                    </td>
                    <td>{formatTs(lang, row.last_used_at)}</td>
                    <td>
                      <ActionBar
                        lang={lang}
                        copyAvailable={row.copy_available}
                        envManaged={row.env_managed}
                        isRevoked={row.status === 'revoked'}
                        busy={busy}
                        onCopy={() => void copyRow(row)}
                        onRotate={() => void rotateRow(row)}
                        onRevoke={() => void toggleRevokeRow(row)}
                        onRemove={() => setRemoveTarget(row)}
                      />
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </section>

      <AuditActivityCard lang={lang} api={api} initialEvents={events} />

      <ConfirmRemoveModal
        open={Boolean(removeTarget)}
        title={lang === 'vi' ? 'Xác nhận xóa API Key' : 'Remove API Key'}
        message={
          removeTarget
            ? lang === 'vi'
              ? `Xóa vĩnh viễn khóa "${removeTarget.label}"? Mọi tiến trình đang dùng Bearer token này sẽ bị từ chối ngay lập tức.`
              : `Permanently delete API key "${removeTarget.label}"? Any caller presenting this token will fail immediately.`
            : ''
        }
        warning={
          lang === 'vi'
            ? 'Hành động này không thể hoàn tác.'
            : 'This action cannot be undone.'
        }
        confirmLabel={lang === 'vi' ? 'Xóa vĩnh viễn' : 'Remove permanently'}
        cancelLabel={lang === 'vi' ? 'Hủy' : 'Cancel'}
        busy={busy}
        onConfirm={() => void confirmRemove()}
        onClose={() => setRemoveTarget(null)}
      />

      <ConfirmRemoveModal
        open={revokeAllOpen}
        title={lang === 'vi' ? 'Thu hồi toàn bộ phiên đăng nhập' : 'Revoke All Active Sessions'}
        message={
          lang === 'vi'
            ? 'Đăng xuất ngay lập tức mọi trình duyệt hiện đang mở Newsman, bao gồm cả phiên hiện tại.'
            : 'Immediately sign out every active Newsman browser session, including this one.'
        }
        confirmLabel={lang === 'vi' ? 'Thu hồi ngay' : 'Revoke all sessions'}
        cancelLabel={lang === 'vi' ? 'Hủy' : 'Cancel'}
        busy={busy}
        onConfirm={() => void confirmRevokeAll()}
        onClose={() => setRevokeAllOpen(false)}
      />
    </div>
  )
}

export function ProvidersPanel({
  lang,
  api,
}: {
  lang: Lang
  api: AdminApiFn
}) {
  const [providers, setProviders] = useState<ProviderRow[] | null>(null)
  const [roles, setRoles] = useState<{
    default_smart_model: string
    fast_background_model: string
    resolved_fast_model: string
  }>({ default_smart_model: '', fast_background_model: '', resolved_fast_model: '' })
  const [catalog, setCatalog] = useState<ModelCatalogRow[]>([])
  const [busy, setBusy] = useState(false)
  const [notice, setNotice] = useState('')
  const [error, setError] = useState('')
  const [revealed, setRevealed] = useState<{ label: string; secret: string } | null>(null)
  const [editingId, setEditingId] = useState<string | null>(null)
  const [formName, setFormName] = useState('')
  const [formUrl, setFormUrl] = useState('')
  const [formKey, setFormKey] = useState('')
  const [smartModel, setSmartModel] = useState('')
  const [fastModel, setFastModel] = useState('')
  const [removeTarget, setRemoveTarget] = useState<ProviderRow | null>(null)

  const load = useCallback(async () => {
    try {
      const [provRes, catRes] = await Promise.all([
        api<{
          ok: boolean
          items: ProviderRow[]
          roles: {
            default_smart_model: string
            fast_background_model: string
            resolved_fast_model: string
          }
        }>('/api/admin/providers'),
        api<{
          items?: ModelCatalogRow[]
          models?: ModelCatalogRow[]
          data?: ModelCatalogRow[]
        }>('/api/admin/models/catalog'),
      ])
      setProviders(provRes.items ?? [])
      const nextRoles = provRes.roles ?? {
        default_smart_model: '',
        fast_background_model: '',
        resolved_fast_model: '',
      }
      setRoles(nextRoles)
      setSmartModel(nextRoles.default_smart_model || '')
      setFastModel(nextRoles.fast_background_model || '')
      setCatalog(catRes.items ?? catRes.models ?? catRes.data ?? [])
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load providers')
      setProviders([])
    }
  }, [api])

  useEffect(() => {
    void load()
  }, [load])

  const hubProvider = useMemo(
    () => (providers ?? []).find((p) => p.id === 'lnc-agents-hub') ?? null,
    [providers],
  )
  const customProviders = useMemo(
    () => (providers ?? []).filter((p) => p.id !== 'lnc-agents-hub'),
    [providers],
  )

  const selectableModelOptions = useMemo(() => {
    const enabledIds = catalog.filter((m) => m.enabled).map((m) => m.model_id)
    const fallbackIds = catalog.map((m) => m.model_id)
    const unique = Array.from(
      new Set([...enabledIds, ...(enabledIds.length ? [] : fallbackIds), smartModel, fastModel].filter(Boolean)),
    )
    return unique.map((id) => ({ value: id, label: id }))
  }, [catalog, smartModel, fastModel])

  const startEdit = (prov: ProviderRow) => {
    setEditingId(prov.id)
    setFormName(prov.name)
    setFormUrl(prov.base_url)
    setFormKey('')
  }

  const resetForm = () => {
    setEditingId(null)
    setFormName('')
    setFormUrl('')
    setFormKey('')
  }

  const submitProvider = async (e: FormEvent) => {
    e.preventDefault()
    setBusy(true)
    setError('')
    setNotice('')
    try {
      const cleanedKey = omitBlankOrMasked(formKey)
      const payload: Record<string, unknown> = {
        base_url: formUrl.trim(),
        ...(editingId !== 'lnc-agents-hub' && formName.trim() ? { name: formName.trim() } : {}),
        ...(cleanedKey !== undefined ? { api_key: cleanedKey } : {}),
      }
      if (editingId) {
        await api(`/api/admin/providers/${encodeURIComponent(editingId)}`, {
          method: 'PUT',
          body: JSON.stringify(payload),
        })
        setNotice(lang === 'vi' ? 'Đã cập nhật nhà cung cấp.' : 'Provider updated.')
      } else {
        await api('/api/admin/providers', {
          method: 'POST',
          body: JSON.stringify(payload),
        })
        setNotice(lang === 'vi' ? 'Đã thêm nhà cung cấp AI.' : 'Provider added.')
      }
      resetForm()
      await load()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Save failed')
    } finally {
      setBusy(false)
    }
  }

  const fetchModels = async (prov: ProviderRow) => {
    setBusy(true)
    setError('')
    setNotice('')
    try {
      const res = await api<{ ok: boolean; fetched_count: number }>(
        `/api/admin/providers/${encodeURIComponent(prov.id)}/fetch-models`,
        { method: 'POST' },
      )
      setNotice(
        lang === 'vi'
          ? `Đã đồng bộ ${res.fetched_count ?? 0} model từ ${prov.name}.`
          : `Synced ${res.fetched_count ?? 0} models from ${prov.name}.`,
      )
      await load()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Fetch models failed')
    } finally {
      setBusy(false)
    }
  }

  const copyProvider = async (prov: ProviderRow) => {
    setBusy(true)
    setError('')
    setNotice('')
    try {
      const res = await api<{ ok: boolean; secret: string }>(
        `/api/admin/providers/${encodeURIComponent(prov.id)}/copy`,
        { method: 'POST' },
      )
      setRevealed({ label: prov.name, secret: res.secret })
      const ok = await copyText(res.secret)
      setNotice(
        ok
          ? lang === 'vi'
            ? `Đã sao chép API key của ${prov.name}.`
            : `Copied key for ${prov.name}.`
          : lang === 'vi'
            ? `Đã nạp khóa ${prov.name} vào vùng sao chép bảo mật.`
            : `Loaded ${prov.name} key into secure copy slot.`,
      )
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Copy failed')
    } finally {
      setBusy(false)
    }
  }

  const rotateProvider = async (prov: ProviderRow) => {
    setBusy(true)
    setError('')
    setNotice('')
    try {
      const res = await api<{ ok: boolean; secret: string }>(
        `/api/admin/providers/${encodeURIComponent(prov.id)}/rotate`,
        { method: 'POST', body: JSON.stringify({}) },
      )
      setRevealed({ label: `${prov.name} (rotated)`, secret: res.secret })
      void copyText(res.secret)
      setNotice(lang === 'vi' ? `Đã xoay vòng khóa ${prov.name}.` : `Rotated key for ${prov.name}.`)
      await load()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Rotate failed')
    } finally {
      setBusy(false)
    }
  }

  const toggleRevokeProvider = async (prov: ProviderRow) => {
    setBusy(true)
    setError('')
    setNotice('')
    try {
      if (prov.status === 'revoked' || !prov.enabled) {
        await api(`/api/admin/providers/${encodeURIComponent(prov.id)}`, {
          method: 'PUT',
          body: JSON.stringify({ enabled: true }),
        })
        setNotice(lang === 'vi' ? `Đã kích hoạt lại ${prov.name}.` : `Re-enabled ${prov.name}.`)
      } else {
        await api(`/api/admin/providers/${encodeURIComponent(prov.id)}/revoke`, {
          method: 'POST',
        })
        setNotice(lang === 'vi' ? `Đã tạm thu hồi ${prov.name}.` : `Revoked ${prov.name}.`)
      }
      await load()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Action failed')
    } finally {
      setBusy(false)
    }
  }

  const confirmRemoveProvider = async () => {
    if (!removeTarget) return
    setBusy(true)
    setError('')
    try {
      await api(`/api/admin/providers/${encodeURIComponent(removeTarget.id)}`, {
        method: 'DELETE',
      })
      if (editingId === removeTarget.id) resetForm()
      setNotice(lang === 'vi' ? `Đã xóa nhà cung cấp ${removeTarget.name}.` : `Removed ${removeTarget.name}.`)
      setRemoveTarget(null)
      await load()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Delete failed')
    } finally {
      setBusy(false)
    }
  }

  const saveRoles = async () => {
    setBusy(true)
    setError('')
    setNotice('')
    try {
      await api('/api/admin/models/bindings', {
        method: 'PUT',
        body: JSON.stringify({
          default_smart_model: smartModel,
          fast_background_model: fastModel,
        }),
      })
      setNotice(lang === 'vi' ? 'Đã lưu vai trò model toàn cục.' : 'Saved global model roles.')
      await load()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to save bindings')
    } finally {
      setBusy(false)
    }
  }

  const toggleCatalogModel = async (item: ModelCatalogRow) => {
    setBusy(true)
    setError('')
    try {
      await api('/api/admin/models/catalog', {
        method: 'PATCH',
        body: JSON.stringify({ items: [{ model_id: item.model_id, enabled: !item.enabled }] }),
      })
      await load()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Toggle failed')
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="bp-panel-grid">
      <FloatingToast
        notice={notice}
        error={error}
        onDismissNotice={() => setNotice('')}
        onDismissError={() => setError('')}
      />

      <SecretCopySlot
        lang={lang}
        item={revealed}
        onDismiss={() => setRevealed(null)}
        onCopied={setNotice}
      />

      {hubProvider && (
        <section className="bp-card">
          <div className="bp-card-head">
            <div>
              <span className="eyebrow">Default Provider Hub</span>
              <h2 className="bp-card-title">⚡ {hubProvider.name}</h2>
              <p className="bp-card-sub">
                {lang === 'vi'
                  ? 'Nhà cung cấp AI hợp nhất cho tóm tắt tin, phân loại, Agent Salon và Companion. Bỏ trống trường khóa khi sửa để giữ nguyên bí mật cũ.'
                  : 'Unified AI & Agent Hub gateway for Newsman synthesis, classification, Agent Salon, and Companion. Leave the secret blank on edit to preserve the stored key.'}
              </p>
            </div>
            <div className="chip-row">
              <span className={hubProvider.enabled ? 'bp-chip ok' : 'bp-chip warn'}>
                {hubProvider.status}
              </span>
              <span className="bp-chip">{hubProvider.model_count} models</span>
            </div>
          </div>

          <div className="bp-split-two" style={{ alignItems: 'center' }}>
            <div>
              <small className="muted">Endpoint</small>
              <div>
                <button
                  type="button"
                  className="bp-copy-chip"
                  disabled={!hubProvider.base_url}
                  onClick={() =>
                    void copyText(hubProvider.base_url).then((ok) => {
                      if (ok) setNotice(lang === 'vi' ? 'Đã sao chép Hub URL.' : 'Copied Hub URL.')
                    })
                  }
                >
                  {hubProvider.base_url || '—'}
                </button>
              </div>
              <small className="muted">
                Secret: {hubProvider.has_secret ? `••••${hubProvider.key_last4 || 'set'} (${hubProvider.source})` : 'unset'}
              </small>
            </div>
            <div style={{ display: 'flex', justifyContent: 'flex-end', flexWrap: 'wrap', gap: 8 }}>
              <button
                type="button"
                className="btn secondary bp-action-btn"
                disabled={busy || !hubProvider.base_url}
                onClick={() => void fetchModels(hubProvider)}
              >
                ⇣ {lang === 'vi' ? 'Nạp & Test Model' : 'Fetch & Test Models'}
              </button>
              <button
                type="button"
                className="btn secondary bp-action-btn"
                disabled={busy}
                onClick={() => startEdit(hubProvider)}
              >
                {lang === 'vi' ? 'Sửa URL / Key' : 'Edit URL / Key'}
              </button>
              <ActionBar
                lang={lang}
                copyAvailable={hubProvider.copy_available}
                envManaged={hubProvider.env_managed}
                isRevoked={hubProvider.status === 'revoked'}
                removeDisabled
                removeTooltip={lang === 'vi' ? 'Không thể xóa LNC Agents Hub mặc định' : 'Built-in default provider'}
                busy={busy}
                onCopy={() => void copyProvider(hubProvider)}
                onRotate={() => void rotateProvider(hubProvider)}
                onRevoke={() => void toggleRevokeProvider(hubProvider)}
                onRemove={() => {}}
              />
            </div>
          </div>
        </section>
      )}

      <section className="bp-card">
        <div className="bp-card-head">
          <div>
            <span className="eyebrow">Provider Registry</span>
            <h2 className="bp-card-title">
              {editingId
                ? `${lang === 'vi' ? 'Chỉnh sửa' : 'Edit'}: ${formName || editingId}`
                : lang === 'vi'
                  ? '+ Thêm nhà cung cấp OpenAI tương thích'
                  : '+ Add OpenAI-Compatible Provider'}
            </h2>
          </div>
        </div>

        <form className="bp-form-grid" onSubmit={submitProvider} autoComplete="off">
          {editingId !== 'lnc-agents-hub' && (
            <label className="field">
              <span>{lang === 'vi' ? 'Tên nhà cung cấp' : 'Provider name'}</span>
              <input
                required
                value={formName}
                placeholder="OpenRouter / Ollama / vLLM"
                onChange={(e) => setFormName(e.target.value)}
              />
            </label>
          )}
          <label className="field">
            <span>Base URL</span>
            <input
              required
              value={formUrl}
              placeholder="https://api.example.com/v1"
              onChange={(e) => setFormUrl(e.target.value)}
            />
          </label>
          <label className="field">
            <span>
              {lang === 'vi' ? 'API Key (bỏ trống để giữ khóa cũ)' : 'API Key (blank keeps existing secret)'}
            </span>
            <input
              type="password"
              autoComplete="new-password"
              value={formKey}
              placeholder={editingId ? MASK_SENTINEL : 'sk-...'}
              onChange={(e) => setFormKey(e.target.value)}
            />
          </label>
          <div className="btn-row" style={{ alignItems: 'flex-end', justifyContent: 'flex-end' }}>
            {editingId && (
              <button type="button" className="btn secondary" onClick={resetForm}>
                {lang === 'vi' ? 'Hủy sửa' : 'Cancel'}
              </button>
            )}
            <button type="submit" className="btn primary" disabled={busy || !formUrl.trim()}>
              {editingId
                ? lang === 'vi'
                  ? 'Lưu thay đổi'
                  : 'Save changes'
                : lang === 'vi'
                  ? '+ Thêm Provider'
                  : '+ Add Provider'}
            </button>
          </div>
        </form>

        <div className="bp-table-wrap">
          <table className="bp-table">
            <thead>
              <tr>
                <th>{lang === 'vi' ? 'Nhà cung cấp' : 'Provider'}</th>
                <th>Base URL</th>
                <th>{lang === 'vi' ? 'Khóa' : 'Secret'}</th>
                <th>Models</th>
                <th>{lang === 'vi' ? 'Trạng thái' : 'Status'}</th>
                <th>{lang === 'vi' ? 'Thao tác' : 'Actions'}</th>
              </tr>
            </thead>
            <tbody>
              {providers === null ? (
                <tr>
                  <td colSpan={6} className="muted">Loading…</td>
                </tr>
              ) : customProviders.length === 0 ? (
                <tr>
                  <td colSpan={6} className="muted">
                    {lang === 'vi'
                      ? 'Chưa có nhà cung cấp phụ nào — LNC Agents Hub đang đảm nhận toàn bộ lưu lượng.'
                      : 'No extra providers configured — LNC Agents Hub serves all traffic.'}
                  </td>
                </tr>
              ) : (
                customProviders.map((prov) => (
                  <tr key={prov.id}>
                    <td>
                      <strong>{prov.name}</strong>
                      <div><small className="muted">refs: {prov.consumer_refs}</small></div>
                    </td>
                    <td>
                      <button
                        type="button"
                        className="bp-copy-chip"
                        onClick={() =>
                          void copyText(prov.base_url).then((ok) => {
                            if (ok) setNotice(`Copied ${prov.base_url}`)
                          })
                        }
                      >
                        {prov.base_url}
                      </button>
                    </td>
                    <td>{prov.has_secret ? `••••${prov.key_last4 || 'set'}` : 'no key'}</td>
                    <td>{prov.model_count}</td>
                    <td>
                      <span className={prov.enabled ? 'bp-chip ok' : 'bp-chip warn'}>
                        {prov.status}
                      </span>
                    </td>
                    <td>
                      <div className="bp-row-actions">
                        <button
                          type="button"
                          className="btn secondary bp-action-btn"
                          disabled={busy}
                          onClick={() => startEdit(prov)}
                        >
                          {lang === 'vi' ? 'Sửa' : 'Edit'}
                        </button>
                        <button
                          type="button"
                          className="btn secondary bp-action-btn"
                          disabled={busy}
                          onClick={() => void fetchModels(prov)}
                        >
                          {lang === 'vi' ? 'Nạp' : 'Fetch'}
                        </button>
                        <ActionBar
                          lang={lang}
                          copyAvailable={prov.copy_available}
                          envManaged={prov.env_managed}
                          isRevoked={prov.status === 'revoked'}
                          busy={busy}
                          onCopy={() => void copyProvider(prov)}
                          onRotate={() => void rotateProvider(prov)}
                          onRevoke={() => void toggleRevokeProvider(prov)}
                          onRemove={() => setRemoveTarget(prov)}
                        />
                      </div>
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </section>

      <section className="bp-card">
        <div className="bp-card-head">
          <div>
            <span className="eyebrow">Global Roles & Catalog</span>
            <h2 className="bp-card-title">
              {lang === 'vi' ? '2 Vai trò Mô hình Toàn cục' : '2 Global Model Roles'}
            </h2>
            <p className="bp-card-sub">
              Smart: <code>{roles.default_smart_model || '—'}</code> · Fast:{' '}
              <code>{roles.resolved_fast_model || roles.default_smart_model || '—'}</code>
            </p>
          </div>
          <button type="button" className="btn primary" disabled={busy} onClick={() => void saveRoles()}>
            {lang === 'vi' ? 'Lưu vai trò Model' : 'Save Model Roles'}
          </button>
        </div>

        <div className="bp-roles-grid">
          <Select
            variant="field"
            label={lang === 'vi' ? 'Smart Model (Suy luận chính)' : 'Default Smart Model'}
            value={smartModel}
            onChange={setSmartModel}
            options={selectableModelOptions}
          />
          <Select
            variant="field"
            label={lang === 'vi' ? 'Fast Background Model (Tác vụ nền)' : 'Fast Background Model'}
            value={fastModel}
            onChange={setFastModel}
            options={[
              {
                value: '',
                label: lang === 'vi' ? 'Theo Smart Model mặc định' : 'Follow Default Smart Model',
              },
              ...selectableModelOptions,
            ]}
          />
        </div>

        <div>
          <h3 style={{ margin: '6px 0 10px', fontSize: 15 }}>
            {lang === 'vi' ? 'Danh mục Model khả dụng' : 'Model Catalog'} (
            {catalog.filter((m) => m.enabled).length}/{catalog.length})
          </h3>
          {catalog.length === 0 ? (
            <p className="muted">
              {lang === 'vi'
                ? 'Bấm "Nạp & Test Model" trên LNC Agents Hub hoặc Provider để đồng bộ danh sách model.'
                : 'Click "Fetch & Test Models" on LNC Agents Hub or a provider to populate the shared catalog.'}
            </p>
          ) : (
            <div className="chip-row">
              {catalog.map((item) => (
                <button
                  key={item.model_id}
                  type="button"
                  disabled={busy}
                  className={item.enabled ? 'chip active' : 'chip'}
                  onClick={() => void toggleCatalogModel(item)}
                >
                  {item.enabled ? '✓ ' : ''}
                  {item.model_id} <small>({item.provider})</small>
                </button>
              ))}
            </div>
          )}
        </div>
      </section>

      <ConfirmRemoveModal
        open={Boolean(removeTarget)}
        title={lang === 'vi' ? 'Xác nhận xóa Nhà cung cấp AI' : 'Remove AI Provider'}
        message={
          removeTarget
            ? lang === 'vi'
              ? `Xóa nhà cung cấp "${removeTarget.name}" (${removeTarget.base_url})?`
              : `Remove provider "${removeTarget.name}" (${removeTarget.base_url})?`
            : ''
        }
        warning={
          removeTarget
            ? lang === 'vi'
              ? `Tác động dây chuyền: ${removeTarget.model_count} model trong danh mục sẽ bị xóa theo và ${removeTarget.consumer_refs} cấu hình gán model sẽ chuyển về mặc định.`
              : `Blast radius: ${removeTarget.model_count} catalog model(s) will be removed and ${removeTarget.consumer_refs} consumer binding(s) will fall back to default.`
            : ''
        }
        confirmLabel={lang === 'vi' ? 'Xóa Provider' : 'Remove Provider'}
        cancelLabel={lang === 'vi' ? 'Hủy' : 'Cancel'}
        busy={busy}
        onConfirm={() => void confirmRemoveProvider()}
        onClose={() => setRemoveTarget(null)}
      />
    </div>
  )
}

export function McpConnectorsPanel({
  lang,
  api,
}: {
  lang: Lang
  api: AdminApiFn
}) {
  const [connectors, setConnectors] = useState<McpConnectorRow[] | null>(null)
  const [writes, setWrites] = useState<AgentWriteRow[]>([])
  const [analyses, setAnalyses] = useState<AgentAnalysisRow[]>([])
  const [bridgeSnap, setBridgeSnap] = useState<BridgeConfigSnap>({})
  const [tgSnap, setTgSnap] = useState<TelegramConfigSnap>({})
  const [astraeaUrl, setAstraeaUrl] = useState('')
  const [astraeaToken, setAstraeaToken] = useState('')
  const [tgBotToken, setTgBotToken] = useState('')
  const [tgWebhookSecret, setTgWebhookSecret] = useState('')
  const [tgCygnusUrl, setTgCygnusUrl] = useState('')
  const [tgCygnusKey, setTgCygnusKey] = useState('')

  const [busy, setBusy] = useState(false)
  const [notice, setNotice] = useState('')
  const [error, setError] = useState('')
  const [revealed, setRevealed] = useState<{ label: string; secret: string } | null>(null)
  const [audience, setAudience] = useState('lnc-newsman')
  const [inboundScopeRead, setInboundScopeRead] = useState(true)
  const [inboundScopeWrite, setInboundScopeWrite] = useState(true)
  const [outName, setOutName] = useState('')
  const [outUrl, setOutUrl] = useState('')
  const [outScopes, setOutScopes] = useState('read')
  const [outSecret, setOutSecret] = useState('')
  const [removeTarget, setRemoveTarget] = useState<{
    id: string
    name: string
    inbound?: boolean
  } | null>(null)

  const load = useCallback(async () => {
    try {
      const [connRes, wRes, aRes, bRes, tRes] = await Promise.all([
        api<{ ok: boolean; items: McpConnectorRow[] }>('/api/admin/mcp-connectors'),
        api<{ data: AgentWriteRow[] }>('/api/admin/connector/writes'),
        api<{ data: AgentAnalysisRow[] }>('/api/admin/connector/analyses'),
        api<BridgeConfigSnap>('/api/app-agents/bridge-config').catch(() => ({} as BridgeConfigSnap)),
        api<TelegramConfigSnap>('/api/app-agents/telegram/config').catch(() => ({} as TelegramConfigSnap)),
      ])
      const rows = connRes.items ?? []
      setConnectors(rows)
      const inbound = rows.find((r) => r.direction === 'inbound' || r.id === 'inbound-mcp')
      if (inbound) {
        setAudience(inbound.audience || 'lnc-newsman')
        setInboundScopeRead(inbound.scopes.includes('newsman:read'))
        setInboundScopeWrite(inbound.scopes.includes('newsman:write'))
      }
      setWrites(wRes.data ?? [])
      setAnalyses(aRes.data ?? [])
      setBridgeSnap(bRes)
      setAstraeaUrl(bRes.astraea_url || '')
      setTgSnap(tRes)
      setTgCygnusUrl(tRes.cygnus_url || '')
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load MCP connectors')
      setConnectors([])
    }
  }, [api])

  useEffect(() => {
    void load()
  }, [load])

  const inbound = useMemo(
    () =>
      (connectors ?? []).find((c) => c.direction === 'inbound' || c.id === 'inbound-mcp') ?? null,
    [connectors],
  )
  const outboundList = useMemo(
    () => (connectors ?? []).filter((c) => c.direction !== 'inbound' && c.id !== 'inbound-mcp'),
    [connectors],
  )

  const saveInbound = async (e: FormEvent) => {
    e.preventDefault()
    if (!inbound) return
    const scopes: string[] = []
    if (inboundScopeRead) scopes.push('newsman:read')
    if (inboundScopeWrite) scopes.push('newsman:write')
    setBusy(true)
    setError('')
    setNotice('')
    try {
      await api(`/api/admin/mcp-connectors/${encodeURIComponent(inbound.id)}`, {
        method: 'PUT',
        body: JSON.stringify({ audience: audience.trim(), scopes }),
      })
      setNotice(lang === 'vi' ? 'Đã lưu cấu hình cổng MCP.' : 'Inbound MCP settings saved.')
      await load()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Save failed')
    } finally {
      setBusy(false)
    }
  }

  const copyInboundField = async (field: 'bearer' | 'delegation') => {
    if (!inbound) return
    setBusy(true)
    setError('')
    setNotice('')
    try {
      const res = await api<{ ok: boolean; secret: string }>(
        `/api/admin/mcp-connectors/${encodeURIComponent(inbound.id)}/copy?field=${field}`,
        { method: 'POST' },
      )
      setRevealed({ label: `Inbound MCP (${field})`, secret: res.secret })
      const ok = await copyText(res.secret)
      setNotice(
        ok
          ? lang === 'vi'
            ? `Đã sao chép khóa ${field} của cổng MCP.`
            : `Copied Inbound MCP ${field} secret.`
          : lang === 'vi'
            ? `Đã nạp khóa ${field} vào vùng sao chép bảo mật.`
            : `Loaded ${field} secret into secure copy slot.`,
      )
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Copy failed')
    } finally {
      setBusy(false)
    }
  }

  const rotateInboundField = async (field: 'bearer' | 'delegation') => {
    if (!inbound) return
    setBusy(true)
    setError('')
    setNotice('')
    try {
      const res = await api<{ ok: boolean; secret: string }>(
        `/api/admin/mcp-connectors/${encodeURIComponent(inbound.id)}/rotate`,
        { method: 'POST', body: JSON.stringify({ field }) },
      )
      setRevealed({ label: `Inbound MCP (${field} rotated)`, secret: res.secret })
      void copyText(res.secret)
      setNotice(
        lang === 'vi' ? `Đã xoay vòng khóa ${field}.` : `Rotated Inbound MCP ${field}.`,
      )
      await load()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Rotate failed')
    } finally {
      setBusy(false)
    }
  }

  const toggleRevoke = async (row: McpConnectorRow) => {
    setBusy(true)
    setError('')
    setNotice('')
    try {
      if (row.status === 'revoked' || !row.enabled) {
        await api(`/api/admin/mcp-connectors/${encodeURIComponent(row.id)}`, {
          method: 'PUT',
          body: JSON.stringify({ enabled: true }),
        })
        setNotice(lang === 'vi' ? `Đã bật lại ${row.name}.` : `Re-enabled ${row.name}.`)
      } else {
        await api(`/api/admin/mcp-connectors/${encodeURIComponent(row.id)}/revoke`, {
          method: 'POST',
        })
        setNotice(lang === 'vi' ? `Đã thu hồi ${row.name}.` : `Revoked ${row.name}.`)
      }
      await load()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Action failed')
    } finally {
      setBusy(false)
    }
  }

  const testConnector = async (row: McpConnectorRow) => {
    setBusy(true)
    setError('')
    setNotice('')
    try {
      const res = await api<{ ok: boolean; tool_count?: number }>(
        `/api/admin/mcp-connectors/${encodeURIComponent(row.id)}/test`,
        { method: 'POST' },
      )
      setNotice(
        lang === 'vi'
          ? `Kiểm tra ${row.name} thành công (${res.tool_count ?? 0} công cụ).`
          : `Tested ${row.name}: ${res.tool_count ?? 0} tools available.`,
      )
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Probe failed')
    } finally {
      setBusy(false)
    }
  }

  const createOutbound = async (e: FormEvent) => {
    e.preventDefault()
    if (!outName.trim() || !outUrl.trim()) return
    setBusy(true)
    setError('')
    setNotice('')
    try {
      const scopes = outScopes
        .split(',')
        .map((s) => s.trim())
        .filter(Boolean)
      const cleanedSec = omitBlankOrMasked(outSecret)
      await api('/api/admin/mcp-connectors', {
        method: 'POST',
        body: JSON.stringify({
          name: outName.trim(),
          endpoint_url: outUrl.trim(),
          scopes,
          ...(cleanedSec !== undefined ? { bearer_token: cleanedSec } : {}),
        }),
      })
      setOutName('')
      setOutUrl('')
      setOutSecret('')
      setNotice(lang === 'vi' ? 'Đã thêm MCP Connector.' : 'MCP connector added.')
      await load()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Create connector failed')
    } finally {
      setBusy(false)
    }
  }

  const copyOutbound = async (row: McpConnectorRow) => {
    setBusy(true)
    setError('')
    try {
      const res = await api<{ ok: boolean; secret: string }>(
        `/api/admin/mcp-connectors/${encodeURIComponent(row.id)}/copy`,
        { method: 'POST' },
      )
      setRevealed({ label: row.name, secret: res.secret })
      const ok = await copyText(res.secret)
      setNotice(
        ok
          ? lang === 'vi'
            ? `Đã sao chép khóa ${row.name}.`
            : `Copied secret for ${row.name}.`
          : lang === 'vi'
            ? `Đã nạp khóa ${row.name} vào vùng sao chép bảo mật.`
            : `Loaded ${row.name} secret into secure copy slot.`,
      )
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Copy failed')
    } finally {
      setBusy(false)
    }
  }

  const rotateOutbound = async (row: McpConnectorRow) => {
    setBusy(true)
    setError('')
    try {
      const res = await api<{ ok: boolean; secret: string }>(
        `/api/admin/mcp-connectors/${encodeURIComponent(row.id)}/rotate`,
        { method: 'POST', body: JSON.stringify({}) },
      )
      setRevealed({ label: `${row.name} (rotated)`, secret: res.secret })
      void copyText(res.secret)
      setNotice(lang === 'vi' ? `Đã xoay vòng ${row.name}.` : `Rotated ${row.name}.`)
      await load()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Rotate failed')
    } finally {
      setBusy(false)
    }
  }

  const saveChannelCredentials = async (e: FormEvent) => {
    e.preventDefault()
    setBusy(true)
    setError('')
    setNotice('')
    try {
      const cleanAstTok = omitBlankOrMasked(astraeaToken)
      await api('/api/app-agents/bridge-config', {
        method: 'PUT',
        body: JSON.stringify({
          astraea_url: astraeaUrl.trim(),
          ...(cleanAstTok !== undefined ? { astraea_token: cleanAstTok } : {}),
        }),
      })
      const cleanBot = omitBlankOrMasked(tgBotToken)
      const cleanWh = omitBlankOrMasked(tgWebhookSecret)
      const cleanCyg = omitBlankOrMasked(tgCygnusKey)
      await api('/api/app-agents/telegram/config', {
        method: 'PUT',
        body: JSON.stringify({
          cygnus_url: tgCygnusUrl.trim(),
          ...(cleanBot !== undefined ? { bot_token: cleanBot } : {}),
          ...(cleanWh !== undefined ? { webhook_secret: cleanWh } : {}),
          ...(cleanCyg !== undefined ? { cygnus_key: cleanCyg } : {}),
        }),
      })
      setAstraeaToken('')
      setTgBotToken('')
      setTgWebhookSecret('')
      setTgCygnusKey('')
      setNotice(
        lang === 'vi'
          ? 'Đã lưu thông tin xác thực Relay & Telegram.'
          : 'Saved relay and channel credentials.',
      )
      await load()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to save channel credentials')
    } finally {
      setBusy(false)
    }
  }

  const confirmRemove = async () => {
    if (!removeTarget) return
    setBusy(true)
    setError('')
    try {
      await api(`/api/admin/mcp-connectors/${encodeURIComponent(removeTarget.id)}`, {
        method: 'DELETE',
      })
      setNotice(
        removeTarget.inbound
          ? lang === 'vi'
            ? 'Đã xóa cặp khóa quản trị của cổng MCP.'
            : 'Cleared runtime MCP pairing secrets.'
          : lang === 'vi'
            ? `Đã xóa connector ${removeTarget.name}.`
            : `Removed connector ${removeTarget.name}.`,
      )
      setRemoveTarget(null)
      await load()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Remove failed')
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="bp-panel-grid">
      <FloatingToast
        notice={notice}
        error={error}
        onDismissNotice={() => setNotice('')}
        onDismissError={() => setError('')}
      />

      <SecretCopySlot
        lang={lang}
        item={revealed}
        onDismiss={() => setRevealed(null)}
        onCopied={setNotice}
      />

      {inbound && (
        <section className="bp-card">
          <div className="bp-card-head">
            <div>
              <span className="eyebrow">Single-Lane App Connector · LNC-Agent-Hub</span>
              <h2 className="bp-card-title">{inbound.name}</h2>
              <p className="bp-card-sub">
                {lang === 'vi'
                  ? 'Cổng kết nối chuẩn cho LNC-Agent-Hub (/v1/app-connectors/lnc-newsman/mcp) và /mcp kèm giấy ủy quyền có phạm vi.'
                  : 'Unified LNC-Agent-Hub app connector (/v1/app-connectors/lnc-newsman/mcp) and backward-compatible /mcp endpoint with scoped capability grants.'}
              </p>
            </div>
            <div className="chip-row">
              <span className={inbound.enabled ? 'bp-chip ok' : 'bp-chip warn'}>
                {inbound.status}
              </span>
              <span className="bp-chip">{inbound.tool_count ?? 0} tools</span>
            </div>
          </div>

          <div className="bp-split-two" style={{ alignItems: 'center' }}>
            <div>
              <small className="muted">Connector Endpoint ({lang === 'vi' ? 'bấm để chép' : 'click to copy'})</small>
              <div>
                <button
                  type="button"
                  className="bp-copy-chip"
                  onClick={() =>
                    void copyText(inbound.endpoint_url).then((ok) => {
                      if (ok) setNotice(lang === 'vi' ? 'Đã chép URL connector.' : 'Copied connector URL.')
                    })
                  }
                >
                  {inbound.endpoint_url}
                </button>
              </div>
            </div>
            <div className="btn-row" style={{ justifyContent: 'flex-end' }}>
              <button
                type="button"
                className="btn secondary bp-action-btn"
                disabled={busy}
                onClick={() => void testConnector(inbound)}
              >
                {lang === 'vi' ? 'Kiểm tra MCP' : 'Test MCP'}
              </button>
              <button
                type="button"
                className="btn secondary bp-action-btn"
                disabled={busy}
                onClick={() => void toggleRevoke(inbound)}
              >
                {inbound.status === 'revoked'
                  ? lang === 'vi'
                    ? 'Kích hoạt lại'
                    : 'Reactivate'
                  : lang === 'vi'
                    ? 'Thu hồi cổng MCP'
                    : 'Revoke MCP'}
              </button>
            </div>
          </div>

          <form className="bp-form-grid" onSubmit={saveInbound}>
            <label className="field">
              <span>Audience</span>
              <input value={audience} maxLength={64} onChange={(e) => setAudience(e.target.value)} />
            </label>
            <div className="field">
              <span>Max Scopes</span>
              <div className="chip-row" style={{ marginTop: 6 }}>
                <button
                  type="button"
                  className={inboundScopeRead ? 'chip active' : 'chip'}
                  onClick={() => setInboundScopeRead((v) => !v)}
                >
                  {inboundScopeRead ? '✓ ' : ''}newsman:read
                </button>
                <button
                  type="button"
                  className={inboundScopeWrite ? 'chip active' : 'chip'}
                  onClick={() => setInboundScopeWrite((v) => !v)}
                >
                  {inboundScopeWrite ? '✓ ' : ''}newsman:write
                </button>
                <button type="submit" className="btn primary bp-action-btn" disabled={busy}>
                  {lang === 'vi' ? 'Lưu' : 'Save'}
                </button>
              </div>
            </div>
          </form>

          <div className="bp-table-wrap">
            <table className="bp-table">
              <thead>
                <tr>
                  <th>{lang === 'vi' ? 'Bí mật ghép nối' : 'Pairing Secret'}</th>
                  <th>{lang === 'vi' ? 'Nguồn' : 'Source'}</th>
                  <th>{lang === 'vi' ? 'Trạng thái' : 'Status'}</th>
                  <th>{lang === 'vi' ? 'Thao tác' : 'Actions'}</th>
                </tr>
              </thead>
              <tbody>
                <tr>
                  <td><code>connector-bearer</code></td>
                  <td>
                    <span className="bp-chip">{inbound.bearer_source ?? 'unset'}</span>
                  </td>
                  <td>
                    <span className={inbound.bearer_configured ? 'bp-chip ok' : 'bp-chip warn'}>
                      {inbound.bearer_configured ? 'configured' : 'unset'}
                    </span>
                  </td>
                  <td>
                    <ActionBar
                      lang={lang}
                      copyAvailable={Boolean(inbound.bearer_copy_available)}
                      envManaged={false}
                      showRevoke={false}
                      busy={busy}
                      onCopy={() => void copyInboundField('bearer')}
                      onRotate={() => void rotateInboundField('bearer')}
                      onRemove={() =>
                        setRemoveTarget({ id: inbound.id, name: inbound.name, inbound: true })
                      }
                    />
                  </td>
                </tr>
                <tr>
                  <td><code>delegation-secret</code></td>
                  <td>
                    <span className="bp-chip">{inbound.delegation_source ?? 'unset'}</span>
                  </td>
                  <td>
                    <span className={inbound.delegation_configured ? 'bp-chip ok' : 'bp-chip warn'}>
                      {inbound.delegation_configured ? 'configured' : 'unset'}
                    </span>
                  </td>
                  <td>
                    <ActionBar
                      lang={lang}
                      copyAvailable={Boolean(inbound.delegation_copy_available)}
                      envManaged={false}
                      showRevoke={false}
                      busy={busy}
                      onCopy={() => void copyInboundField('delegation')}
                      onRotate={() => void rotateInboundField('delegation')}
                      onRemove={() =>
                        setRemoveTarget({ id: inbound.id, name: inbound.name, inbound: true })
                      }
                    />
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
        </section>
      )}

      <section className="bp-card">
        <div className="bp-card-head">
          <div>
            <span className="eyebrow">Outbound Connectors</span>
            <h2 className="bp-card-title">
              {lang === 'vi' ? 'Trình kết nối MCP mở rộng' : 'Custom Outbound MCP Connectors'}
            </h2>
          </div>
        </div>

        <form className="bp-form-grid" onSubmit={createOutbound} autoComplete="off">
          <label className="field">
            <span>{lang === 'vi' ? 'Tên kết nối' : 'Connector name'}</span>
            <input
              required
              value={outName}
              placeholder="Cortex Hub / Obsidian MCP"
              onChange={(e) => setOutName(e.target.value)}
            />
          </label>
          <label className="field">
            <span>Endpoint URL</span>
            <input
              required
              value={outUrl}
              placeholder="https://mcp.example.com/mcp"
              onChange={(e) => setOutUrl(e.target.value)}
            />
          </label>
          <label className="field">
            <span>Scopes</span>
            <input value={outScopes} onChange={(e) => setOutScopes(e.target.value)} />
          </label>
          <label className="field">
            <span>Bearer Token (optional)</span>
            <input
              type="password"
              autoComplete="new-password"
              value={outSecret}
              placeholder={MASK_SENTINEL}
              onChange={(e) => setOutSecret(e.target.value)}
            />
          </label>
          <div className="btn-row" style={{ gridColumn: '1 / -1', justifyContent: 'flex-end' }}>
            <button type="submit" className="btn primary" disabled={busy || !outName.trim() || !outUrl.trim()}>
              {lang === 'vi' ? '+ Thêm MCP Connector' : '+ Add MCP Connector'}
            </button>
          </div>
        </form>

        <div className="bp-table-wrap">
          <table className="bp-table">
            <thead>
              <tr>
                <th>{lang === 'vi' ? 'Tên' : 'Name'}</th>
                <th>Endpoint</th>
                <th>Scopes</th>
                <th>{lang === 'vi' ? 'Trạng thái' : 'Status'}</th>
                <th>{lang === 'vi' ? 'Thao tác' : 'Actions'}</th>
              </tr>
            </thead>
            <tbody>
              {outboundList.length === 0 ? (
                <tr>
                  <td colSpan={5} className="muted">
                    {lang === 'vi' ? 'Chưa có connector outbound nào.' : 'No custom outbound MCP connectors.'}
                  </td>
                </tr>
              ) : (
                outboundList.map((row) => (
                  <tr key={row.id}>
                    <td>
                      <strong>{row.name}</strong>
                      {row.has_secret && <div><small className="muted">••••{row.key_last4 || 'set'}</small></div>}
                    </td>
                    <td>
                      <button
                        type="button"
                        className="bp-copy-chip"
                        onClick={() =>
                          void copyText(row.endpoint_url).then((ok) => {
                            if (ok) setNotice(`Copied ${row.endpoint_url}`)
                          })
                        }
                      >
                        {row.endpoint_url}
                      </button>
                    </td>
                    <td>
                      <div className="chip-row">
                        {row.scopes.map((sc) => (
                          <span key={sc} className="bp-chip">{sc}</span>
                        ))}
                      </div>
                    </td>
                    <td>
                      <span className={row.enabled ? 'bp-chip ok' : 'bp-chip warn'}>
                        {row.status}
                      </span>
                    </td>
                    <td>
                      <div className="bp-row-actions">
                        <button
                          type="button"
                          className="btn secondary bp-action-btn"
                          disabled={busy}
                          onClick={() => void testConnector(row)}
                        >
                          {lang === 'vi' ? 'Thử' : 'Test'}
                        </button>
                        <ActionBar
                          lang={lang}
                          copyAvailable={row.copy_available}
                          envManaged={row.env_managed}
                          isRevoked={row.status === 'revoked'}
                          busy={busy}
                          onCopy={() => void copyOutbound(row)}
                          onRotate={() => void rotateOutbound(row)}
                          onRevoke={() => void toggleRevoke(row)}
                          onRemove={() => setRemoveTarget({ id: row.id, name: row.name })}
                        />
                      </div>
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </section>

      <section className="bp-card">
        <div className="bp-card-head">
          <div>
            <span className="eyebrow">Relay & Messaging Credentials</span>
            <h2 className="bp-card-title">
              {lang === 'vi' ? 'Kết nối Astraea Relay & Telegram' : 'Astraea Relay & Telegram Credentials'}
            </h2>
            <p className="bp-card-sub">
              {lang === 'vi'
                ? 'Quản lý tập trung khóa relay và bot kênh nhắn tin (bỏ trống trường khóa để giữ nguyên bí mật đã lưu).'
                : 'Centralized relay and messaging channel credentials (leave secret fields blank to keep stored values).'}
            </p>
          </div>
          <div className="chip-row">
            <span className={bridgeSnap.has_astraea_token ? 'bp-chip ok' : 'bp-chip muted'}>
              Astraea: {bridgeSnap.has_astraea_token ? 'configured' : 'unset'}
            </span>
            <span className={tgSnap.has_bot_token || tgSnap.has_cygnus_key ? 'bp-chip ok' : 'bp-chip muted'}>
              Telegram: {tgSnap.has_bot_token || tgSnap.has_cygnus_key ? 'configured' : 'unset'}
            </span>
          </div>
        </div>

        <form className="bp-form-grid" onSubmit={saveChannelCredentials} autoComplete="off">
          <label className="field">
            <span>Astraea Relay URL</span>
            <input
              value={astraeaUrl}
              placeholder="https://astraea.lengoc.me"
              onChange={(e) => setAstraeaUrl(e.target.value)}
            />
          </label>
          <label className="field">
            <span>Astraea Relay Token</span>
            <input
              type="password"
              autoComplete="new-password"
              value={astraeaToken}
              placeholder={bridgeSnap.has_astraea_token ? `${MASK_SENTINEL} (saved)` : 'ast_bridge_…'}
              onChange={(e) => setAstraeaToken(e.target.value)}
            />
          </label>
          <label className="field">
            <span>Telegram Cygnus Relay URL</span>
            <input
              value={tgCygnusUrl}
              placeholder="https://telegram.lengoc.me"
              onChange={(e) => setTgCygnusUrl(e.target.value)}
            />
          </label>
          <label className="field">
            <span>Telegram Cygnus Relay Key</span>
            <input
              type="password"
              autoComplete="new-password"
              value={tgCygnusKey}
              placeholder={tgSnap.has_cygnus_key ? `${MASK_SENTINEL} (saved)` : 'cyg_…'}
              onChange={(e) => setTgCygnusKey(e.target.value)}
            />
          </label>
          <label className="field">
            <span>Telegram Direct Bot Token</span>
            <input
              type="password"
              autoComplete="new-password"
              value={tgBotToken}
              placeholder={tgSnap.has_bot_token ? `${MASK_SENTINEL} (saved)` : '123456:ABC…'}
              onChange={(e) => setTgBotToken(e.target.value)}
            />
          </label>
          <label className="field">
            <span>Telegram Webhook Secret</span>
            <input
              type="password"
              autoComplete="new-password"
              value={tgWebhookSecret}
              placeholder={tgSnap.has_webhook_secret ? `${MASK_SENTINEL} (saved)` : 'tg_wh_…'}
              onChange={(e) => setTgWebhookSecret(e.target.value)}
            />
          </label>
          <div className="btn-row" style={{ gridColumn: '1 / -1', justifyContent: 'flex-end' }}>
            <button type="submit" className="btn primary" disabled={busy}>
              {lang === 'vi' ? 'Lưu thông tin xác thực' : 'Save Channel Credentials'}
            </button>
          </div>
        </form>
      </section>

      <div className="bp-split-two">
        <section className="bp-card">
          <h3 className="bp-card-title" style={{ fontSize: 18 }}>
            {lang === 'vi' ? 'Bản ghi từ MCP gần đây' : 'Recent MCP Writes'}
          </h3>
          {writes.length === 0 ? (
            <p className="muted">{lang === 'vi' ? 'Chưa có lượt ghi nào.' : 'No agent writes yet.'}</p>
          ) : (
            <div className="bp-table-wrap">
              <table className="bp-table">
                <thead>
                  <tr>
                    <th>Tool</th>
                    <th>Client</th>
                    <th>{lang === 'vi' ? 'Thời gian' : 'At'}</th>
                  </tr>
                </thead>
                <tbody>
                  {writes.slice(0, 10).map((w) => (
                    <tr key={w.id}>
                      <td><code>{w.tool}</code></td>
                      <td>{w.client_id ?? 'owner'}</td>
                      <td>{formatTs(lang, w.at)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </section>

        <section className="bp-card">
          <h3 className="bp-card-title" style={{ fontSize: 18 }}>
            {lang === 'vi' ? 'Brief đã lưu qua MCP' : 'Stored Briefs'}
          </h3>
          {analyses.length === 0 ? (
            <p className="muted">{lang === 'vi' ? 'Chưa có brief nào.' : 'No briefs stored yet.'}</p>
          ) : (
            <div className="bp-table-wrap">
              <table className="bp-table">
                <thead>
                  <tr>
                    <th>{lang === 'vi' ? 'Tiêu đề' : 'Title'}</th>
                    <th>{lang === 'vi' ? 'Tác giả' : 'Author'}</th>
                  </tr>
                </thead>
                <tbody>
                  {analyses.slice(0, 10).map((a) => (
                    <tr key={a.id}>
                      <td><strong>{a.title}</strong></td>
                      <td><span className="bp-chip">{a.author ?? 'owner'}</span></td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </section>
      </div>

      <ConfirmRemoveModal
        open={Boolean(removeTarget)}
        title={lang === 'vi' ? 'Xác nhận xóa MCP Connector' : 'Remove MCP Connector'}
        message={
          removeTarget?.inbound
            ? lang === 'vi'
              ? 'Xóa cặp khóa ghép nối runtime của cổng MCP? Các kết nối bên ngoài sẽ không gọi được MCP cho tới khi lưu khóa mới.'
              : 'Clear runtime pairing secrets on MCP? External Hub callers will receive 401 until new secrets are provisioned.'
            : removeTarget
              ? lang === 'vi'
                ? `Xóa vĩnh viễn MCP Connector "${removeTarget.name}"?`
                : `Permanently delete MCP connector "${removeTarget.name}"?`
              : ''
        }
        confirmLabel={lang === 'vi' ? 'Xóa ngay' : 'Remove'}
        cancelLabel={lang === 'vi' ? 'Hủy' : 'Cancel'}
        busy={busy}
        onConfirm={() => void confirmRemove()}
        onClose={() => setRemoveTarget(null)}
      />
    </div>
  )
}
