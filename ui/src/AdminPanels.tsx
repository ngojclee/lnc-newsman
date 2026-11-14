import { FormEvent, useCallback, useEffect, useMemo, useState } from 'react'
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
  hash_algo: string
  session_epoch: number
  oidc_enabled: boolean
  vault_configured: boolean
  env_pinned: boolean
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
  if (!trimmed || trimmed === MASK_SENTINEL || /^[•*]{6,}$/.test(trimmed)) {
    return undefined
  }
  return trimmed
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
          envTip: 'Khóa từ biến môi trường (chỉ đọc)',
          rotateToCopy: 'Xoay vòng để bật Sao chép',
        }
      : {
          copy: 'Copy',
          rotate: 'Rotate',
          revoke: isRevoked ? 'Reactivate' : 'Revoke',
          remove: 'Remove',
          envTip: 'Env-managed (read-only)',
          rotateToCopy: 'Rotate to enable Copy',
        }

  const copyTitle = envManaged
    ? labels.envTip
    : copyAvailable
      ? labels.copy
      : labels.rotateToCopy

  return (
    <div className="bp-row-actions">
      <button
        type="button"
        className="btn secondary bp-action-btn"
        disabled={busy || envManaged || !copyAvailable}
        title={copyTitle}
        onClick={onCopy}
      >
        {labels.copy}
      </button>
      <button
        type="button"
        className="btn secondary bp-action-btn"
        disabled={busy || envManaged}
        title={envManaged ? labels.envTip : labels.rotate}
        onClick={onRotate}
      >
        {labels.rotate}
      </button>
      {showRevoke && onRevoke && (
        <button
          type="button"
          className="btn secondary bp-action-btn"
          disabled={busy || envManaged}
          title={envManaged ? labels.envTip : labels.revoke}
          onClick={onRevoke}
        >
          {labels.revoke}
        </button>
      )}
      <button
        type="button"
        className="btn secondary bp-action-btn bp-action-danger"
        disabled={busy || envManaged || removeDisabled}
        title={removeTooltip || (envManaged ? labels.envTip : labels.remove)}
        onClick={onRemove}
      >
        {labels.remove}
      </button>
    </div>
  )
}

function SecretBanner({
  lang,
  label,
  secret,
  onDismiss,
  onCopied,
}: {
  lang: Lang
  label: string
  secret: string
  onDismiss: () => void
  onCopied: (msg: string) => void
}) {
  const [revealFallback, setRevealFallback] = useState(false)
  if (!secret) return null
  return (
    <div className="bp-card" style={{ borderColor: 'var(--accent)' }} role="status">
      <div className="bp-card-head">
        <div>
          <strong>{label}</strong>
          <p className="bp-card-sub">
            {lang === 'vi'
              ? 'Giá trị khóa được niêm phong trên máy chủ; sao chép ngay hoặc đóng thông báo này.'
              : 'Sealed on the server with AES-256-GCM; copy now or dismiss this notice.'}
          </p>
        </div>
        <div className="bp-row-actions">
          <button
            type="button"
            className="btn primary bp-action-btn"
            onClick={() =>
              void copyText(secret, () => setRevealFallback(true)).then((ok) => {
                if (ok) {
                  onCopied(lang === 'vi' ? 'Đã sao chép khóa vào clipboard.' : 'Copied secret to clipboard.')
                }
              })
            }
          >
            {lang === 'vi' ? 'Sao chép khóa' : 'Copy secret'}
          </button>
          <button type="button" className="btn secondary bp-action-btn" onClick={onDismiss}>
            {lang === 'vi' ? 'Đóng' : 'Dismiss'}
          </button>
        </div>
      </div>
      <code style={{ wordBreak: 'break-all', fontSize: 12.5 }}>
        {revealFallback ? secret : `${secret.slice(0, 10)}••••••••${secret.slice(-4)}`}
      </code>
    </div>
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

  const load = useCallback(async () => {
    try {
      const [keysRes, auditRes] = await Promise.all([
        api<{ ok: boolean; items: ApiKeyRow[]; mgmt: MgmtStateInfo }>('/api/admin/api-keys'),
        api<{ ok: boolean; items: AuditEventRow[] }>('/api/admin/audit-events'),
      ])
      setItems(keysRes.items ?? [])
      setMgmt(keysRes.mgmt ?? null)
      setEvents(auditRes.items ?? [])
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load API keys')
      setItems([])
    }
  }, [api])

  useEffect(() => {
    void load()
  }, [load])

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
      setNewLabel('')
      setCustomSecret('')
      const sec = res.secret ?? res.key ?? ''
      if (sec) setRevealed({ label: lang === 'vi' ? 'API Key mới' : 'New API Key', secret: sec })
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
      await copyText(res.secret, () => setRevealed({ label: row.label, secret: res.secret }))
      setNotice(lang === 'vi' ? `Đã sao chép khóa ${row.label}.` : `Copied ${row.label} to clipboard.`)
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
      const res = await api<{ ok: boolean; secret: string; session_epoch: number }>(
        '/api/admin/mgmt/rotate',
        {
          method: 'POST',
          body: JSON.stringify(customMgmtKey.trim() ? { new_key: customMgmtKey.trim() } : {}),
        },
      )
      setCustomMgmtKey('')
      setRevealed({ label: 'MANAGEMENT_KEY (Argon2id)', secret: res.secret })
      setNotice(
        lang === 'vi'
          ? `Đã xoay vòng MANAGEMENT_KEY (epoch ${res.session_epoch}).`
          : `Rotated MANAGEMENT_KEY (epoch ${res.session_epoch}).`,
      )
      await load()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Rotate management key failed')
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
      <section className="bp-card">
        <div className="bp-card-head">
          <div>
            <span className="eyebrow">{lang === 'vi' ? 'Bảo mật & SSO' : 'Auth & Break-Glass'}</span>
            <h2 className="bp-card-title">
              {lang === 'vi' ? 'Phiên đăng nhập & MANAGEMENT_KEY' : 'Sessions & MANAGEMENT_KEY'}
            </h2>
            <p className="bp-card-sub">
              {lang === 'vi'
                ? 'Khóa quản trị được băm bằng Argon2id. Xoay vòng khóa hoặc nâng session_epoch sẽ vô hiệu hóa tức thì các phiên cũ.'
                : 'Break-glass MANAGEMENT_KEY is hashed with Argon2id. Rotating or bumping session_epoch immediately invalidates prior sessions.'}
            </p>
          </div>
          {mgmt && (
            <div className="chip-row">
              <span className={mgmt.oidc_enabled ? 'bp-chip ok' : 'bp-chip muted'}>
                SSO OIDC: {mgmt.oidc_enabled ? 'ON' : 'OFF'}
              </span>
              <span className={mgmt.configured ? 'bp-chip ok' : 'bp-chip warn'}>
                {mgmt.hash_algo.toUpperCase()} · {mgmt.source}
              </span>
              <span className="bp-chip">epoch #{mgmt.session_epoch}</span>
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
              placeholder={lang === 'vi' ? 'Bỏ trống để tự sinh nm_mgmt_…' : 'Leave blank to generate nm_mgmt_…'}
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
              {lang === 'vi' ? 'Xoay vòng MANAGEMENT_KEY' : 'Rotate MANAGEMENT_KEY'}
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

      {error && <div className="form-error" role="alert">{error}</div>}
      {notice && <div className="save-state" role="status">✓ {notice}</div>}
      {revealed && (
        <SecretBanner
          lang={lang}
          label={revealed.label}
          secret={revealed.secret}
          onDismiss={() => setRevealed(null)}
          onCopied={setNotice}
        />
      )}

      <section className="bp-card">
        <div className="bp-card-head">
          <div>
            <span className="eyebrow">Bearer / HMAC-SHA256 + AES-256-GCM</span>
            <h2 className="bp-card-title">{lang === 'vi' ? 'Khóa API truy cập' : 'Inbound API Keys'}</h2>
            <p className="bp-card-sub">
              {lang === 'vi'
                ? 'Khóa API dùng để đọc/ghi qua /api/v1/*. Mọi thao tác Copy đều được ghi nhật ký kiểm toán và giới hạn tốc độ.'
                : 'API keys grant scoped Bearer access to /api/v1/*. Copy actions are rate-limited and recorded in the audit log.'}
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
              placeholder={lang === 'vi' ? 'Tự sinh nm_key_ nếu để trống' : 'Auto-generated nm_key_ if blank'}
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
                      {row.env_managed && (
                        <div>
                          <span className="bp-chip muted" title="Env-managed (read-only)">
                            🔒 Env-managed
                          </span>
                        </div>
                      )}
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

      <section className="bp-card">
        <div className="bp-card-head">
          <div>
            <span className="eyebrow">Audit Log</span>
            <h3 className="bp-card-title" style={{ fontSize: 18 }}>
              {lang === 'vi' ? 'Nhật ký bảo mật gần đây' : 'Recent Security Audit Events'}
            </h3>
          </div>
        </div>
        <div className="bp-table-wrap">
          <table className="bp-table">
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
                    {lang === 'vi' ? 'Chưa có sự kiện nào.' : 'No security events yet.'}
                  </td>
                </tr>
              ) : (
                events.slice(0, 20).map((ev) => (
                  <tr key={ev.id}>
                    <td>{formatTs(lang, ev.at)}</td>
                    <td>
                      <strong>{ev.actor}</strong> <small className="muted">({ev.auth_method})</small>
                    </td>
                    <td><code>{ev.action}</code></td>
                    <td>{ev.target_type}:{ev.target_id}</td>
                    <td><code>{ev.client_ip}</code></td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </section>

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
            ? 'Tăng session_epoch và đăng xuất mọi trình duyệt hiện đang mở Newsman, bao gồm cả phiên hiện tại.'
            : 'Bump session_epoch and sign out every active Newsman browser session, including this one.'
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
      await copyText(res.secret, () => setRevealed({ label: prov.name, secret: res.secret }))
      setNotice(lang === 'vi' ? `Đã sao chép API key của ${prov.name}.` : `Copied key for ${prov.name}.`)
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
      {error && <div className="form-error" role="alert">{error}</div>}
      {notice && <div className="save-state" role="status">✓ {notice}</div>}
      {revealed && (
        <SecretBanner
          lang={lang}
          label={revealed.label}
          secret={revealed.secret}
          onDismiss={() => setRevealed(null)}
          onCopied={setNotice}
        />
      )}

      {hubProvider && (
        <section className="bp-card">
          <div className="bp-card-head">
            <div>
              <span className="eyebrow">Default Provider Hub</span>
              <h2 className="bp-card-title">⚡ {hubProvider.name}</h2>
              <p className="bp-card-sub">
                {lang === 'vi'
                  ? 'Nhà cung cấp AI mặc định cho tóm tắt tin, phân loại, Agent Salon và Companion. Bỏ trống trường khóa khi sửa để giữ nguyên bí mật cũ.'
                  : 'Primary AI provider for Newsman brief synthesis, classification, Agent Salon, and Companion. Leave the secret blank on edit to preserve the stored key.'}
              </p>
            </div>
            <div className="chip-row">
              <span className={hubProvider.enabled ? 'bp-chip ok' : 'bp-chip warn'}>
                {hubProvider.status}
              </span>
              <span className="bp-chip">{hubProvider.model_count} models</span>
              {hubProvider.env_managed && <span className="bp-chip muted">🔒 Env-managed</span>}
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
                          {lang === 'vi' ? 'Nạp Model' : 'Fetch'}
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
              ? `Tác động dây chuyền: ${removeTarget.model_count} model trong danh mục sẽ bị xóa theo (CASCADE) và ${removeTarget.consumer_refs} cấu hình gán model sẽ chuyển về mặc định (SET NULL).`
              : `Blast radius: ${removeTarget.model_count} catalog model(s) will be cascade-deleted and ${removeTarget.consumer_refs} consumer binding(s) will fall back to default (SET NULL).`
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
      const [connRes, wRes, aRes] = await Promise.all([
        api<{ ok: boolean; items: McpConnectorRow[] }>('/api/admin/mcp-connectors'),
        api<{ data: AgentWriteRow[] }>('/api/admin/connector/writes'),
        api<{ data: AgentAnalysisRow[] }>('/api/admin/connector/analyses'),
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
      setNotice(lang === 'vi' ? 'Đã lưu cấu hình Inbound /mcp.' : 'Inbound /mcp settings saved.')
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
      await copyText(res.secret, () =>
        setRevealed({ label: `Inbound /mcp (${field})`, secret: res.secret }),
      )
      setNotice(
        lang === 'vi'
          ? `Đã sao chép khóa ${field} của /mcp.`
          : `Copied Inbound /mcp ${field} secret.`,
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
      setRevealed({ label: `Inbound /mcp (${field} rotated)`, secret: res.secret })
      setNotice(
        lang === 'vi' ? `Đã xoay vòng khóa ${field} /mcp.` : `Rotated Inbound /mcp ${field}.`,
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
      await copyText(res.secret, () => setRevealed({ label: row.name, secret: res.secret }))
      setNotice(lang === 'vi' ? `Đã sao chép khóa ${row.name}.` : `Copied secret for ${row.name}.`)
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
      setNotice(lang === 'vi' ? `Đã xoay vòng ${row.name}.` : `Rotated ${row.name}.`)
      await load()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Rotate failed')
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
            ? 'Đã xóa cặp khóa quản trị của /mcp.'
            : 'Cleared runtime /mcp secrets.'
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
      {error && <div className="form-error" role="alert">{error}</div>}
      {notice && <div className="save-state" role="status">✓ {notice}</div>}
      {revealed && (
        <SecretBanner
          lang={lang}
          label={revealed.label}
          secret={revealed.secret}
          onDismiss={() => setRevealed(null)}
          onCopied={setNotice}
        />
      )}

      {inbound && (
        <section className="bp-card">
          <div className="bp-card-head">
            <div>
              <span className="eyebrow">Inbound Business MCP</span>
              <h2 className="bp-card-title">{inbound.name}</h2>
              <p className="bp-card-sub">
                {lang === 'vi'
                  ? 'Cổng MCP của Newsman nhận kết nối từ Business MCP Hub bằng connector-bearer và delegation-secret (HMAC HS256).'
                  : 'Newsman /mcp server endpoint authenticated via connector-bearer and HS256 delegation-secret.'}
              </p>
            </div>
            <div className="chip-row">
              <span className={inbound.enabled ? 'bp-chip ok' : 'bp-chip warn'}>
                {inbound.status}
              </span>
              <span className="bp-chip">{inbound.tool_count ?? 0} tools</span>
              {inbound.env_managed && <span className="bp-chip muted">🔒 Env-managed</span>}
            </div>
          </div>

          <div className="bp-split-two" style={{ alignItems: 'center' }}>
            <div>
              <small className="muted">Endpoint URL ({lang === 'vi' ? 'bấm để chép' : 'click to copy'})</small>
              <div>
                <button
                  type="button"
                  className="bp-copy-chip"
                  onClick={() =>
                    void copyText(inbound.endpoint_url).then((ok) => {
                      if (ok) setNotice(lang === 'vi' ? 'Đã chép URL /mcp.' : 'Copied /mcp URL.')
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
                {lang === 'vi' ? 'Kiểm tra /mcp' : 'Test /mcp'}
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
                    ? 'Thu hồi cổng /mcp'
                    : 'Revoke /mcp'}
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
                    <span className={inbound.bearer_source === 'env' ? 'bp-chip muted' : 'bp-chip'}>
                      {inbound.bearer_source === 'env' ? '🔒 Env-managed' : inbound.bearer_source ?? 'unset'}
                    </span>
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
                      envManaged={inbound.bearer_source === 'env'}
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
                    <span className={inbound.delegation_source === 'env' ? 'bp-chip muted' : 'bp-chip'}>
                      {inbound.delegation_source === 'env'
                        ? '🔒 Env-managed'
                        : inbound.delegation_source ?? 'unset'}
                    </span>
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
                      envManaged={inbound.delegation_source === 'env'}
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
              placeholder="••••••••"
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

      <div className="bp-split-two">
        <section className="bp-card">
          <h3 className="bp-card-title" style={{ fontSize: 18 }}>
            {lang === 'vi' ? 'Bản ghi từ MCP gần đây' : 'Recent /mcp Writes'}
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
              ? 'Xóa cặp khóa ghép nối runtime của cổng /mcp? Business MCP Hub sẽ không gọi được /mcp cho tới khi lưu khóa mới.'
              : 'Clear runtime pairing secrets on /mcp? Business MCP Hub will receive 401 until new secrets are provisioned.'
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
