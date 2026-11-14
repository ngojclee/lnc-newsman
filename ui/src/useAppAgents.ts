import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import {
  DEFAULT_SEED_AGENTS,
  type AccessRequestTargetKind,
  type ActiveViewContext,
  type AgentMessage,
  type AgentSchedule,
  type AgentThread,
  type AgentWorkspace,
  type AppAgentConfig,
  type ApprovalRequiredMeta,
  type BridgeRoutingSummary,
  type BridgeStatus,
  type ConnectionProbeReport,
  type DiscussionMode,
  type HubCapabilityBundleSummary,
  type HubEffectiveBundleCache,
  type HubMcpGroupSummary,
  type HubSkillGroupSummary,
  type HubSkillSummary,
  type MergedToolInvocation,
  type ResourceCtx,
  type SkillAccessRequest,
  type TaskModelsConfig,
  type TelegramBridgeConfig,
  type TelegramRouteConfig,
  type ThreadStreamEvent,
  type ThreadStreamEventKind,
  type WhiteboardNote,
} from './agentTypes.ts'

export class AppAgentsApiError extends Error {
  readonly status: number
  constructor(status: number, message: string) {
    super(message)
    this.status = status
  }
}

export const MAX_ACTIVE_VIEW_CONTEXT_BYTES = 16 * 1024

const VALID_STREAM_KINDS: ReadonlySet<ThreadStreamEventKind> = new Set<ThreadStreamEventKind>([
  'seat_state',
  'text',
  'reasoning',
  'tool',
  'tool_result',
  'result',
  'error',
])

/**
 * Ensures every `/api/app-agents/*` request carries `X-Requested-With: XMLHttpRequest`
 * (CSRF guard required by `lnc-newsman` for all mutations) and `credentials: 'same-origin'`.
 */
export function buildAppAgentsRequestInit(init: RequestInit = {}): RequestInit {
  const headers = new Headers(init.headers)
  if (!headers.has('Content-Type')) {
    headers.set('Content-Type', 'application/json')
  }
  headers.set('X-Requested-With', 'XMLHttpRequest')
  return {
    ...init,
    credentials: 'same-origin',
    headers,
  }
}

export async function appAgentsApi<T>(path: string, init: RequestInit = {}): Promise<T> {
  const reqInit = buildAppAgentsRequestInit(init)
  const response = await fetch(path, reqInit)
  if (!response.ok) {
    if (response.status === 401 && typeof window !== 'undefined') {
      window.dispatchEvent(new Event('newsman:session-expired'))
    }
    const body = await response.json().catch(() => null)
    throw new AppAgentsApiError(
      response.status,
      (body && typeof body === 'object' && ('error' in body || 'message' in body)
        ? String((body as Record<string, unknown>).error ?? (body as Record<string, unknown>).message)
        : null) ?? `Request failed (${response.status})`,
    )
  }
  return (await response.json()) as T
}

function utf8ByteLength(value: string): number {
  return new TextEncoder().encode(value).byteLength
}

function truncateStringToUtf8Bytes(input: string, maxBytes: number): string {
  if (maxBytes <= 0 || !input) return ''
  const encoder = new TextEncoder()
  if (encoder.encode(input).byteLength <= maxBytes) return input
  let low = 0
  let high = input.length
  while (low < high) {
    const mid = Math.ceil((low + high) / 2)
    let candidate = input.slice(0, mid)
    if (candidate.length > 0) {
      const code = candidate.charCodeAt(candidate.length - 1)
      if (code >= 0xd800 && code <= 0xdbff) {
        candidate = candidate.slice(0, -1)
      }
    }
    if (encoder.encode(candidate).byteLength <= maxBytes) {
      low = mid
    } else {
      high = mid - 1
    }
  }
  let out = input.slice(0, low)
  if (out.length > 0) {
    const code = out.charCodeAt(out.length - 1)
    if (code >= 0xd800 && code <= 0xdbff) {
      out = out.slice(0, -1)
    }
  }
  return out
}

/**
 * Bounds `ActiveViewContext` so `JSON.stringify(ctx)` never exceeds `maxBytes` (default 16 KiB).
 */
export function boundActiveViewContext(
  raw: Partial<ActiveViewContext> & { ref_id?: string; article_id?: number | null; report_id?: number | null },
  maxBytes: number = MAX_ACTIVE_VIEW_CONTEXT_BYTES,
): ActiveViewContext {
  const articleId =
    typeof raw.article_id === 'number' && Number.isFinite(raw.article_id) ? raw.article_id : null
  const reportId =
    typeof raw.report_id === 'number' && Number.isFinite(raw.report_id) ? raw.report_id : null
  const kind = raw.kind ?? (reportId !== null ? 'report' : 'article')
  const refId =
    raw.ref_id ??
    (articleId !== null ? String(articleId) : reportId !== null ? `report:${reportId}` : 'active')

  const base: ActiveViewContext = {
    kind,
    ref_id: refId,
    ...(articleId !== null ? { article_id: articleId } : {}),
    ...(reportId !== null ? { report_id: reportId } : {}),
    ...(raw.title ? { title: truncateStringToUtf8Bytes(raw.title, 512) } : {}),
    ...(raw.source_attribution
      ? { source_attribution: truncateStringToUtf8Bytes(raw.source_attribution, 512) }
      : {}),
  }

  let selectedText = raw.selected_text ? raw.selected_text.trim() : ''
  let contentText = raw.content_text ? raw.content_text.trim() : ''
  let excerpt = raw.excerpt ? raw.excerpt.trim() : ''

  const candidateWith = (sel: string, body: string, exc: string): ActiveViewContext => ({
    ...base,
    ...(sel ? { selected_text: sel } : {}),
    ...(body ? { content_text: body } : {}),
    ...(exc ? { excerpt: exc } : {}),
  })

  let result = candidateWith(selectedText, contentText, excerpt)
  if (utf8ByteLength(JSON.stringify(result)) <= maxBytes) {
    return result
  }

  const baseBytes = utf8ByteLength(JSON.stringify(base)) + 96
  const remainingBudget = Math.max(0, maxBytes - baseBytes)

  if (selectedText && contentText) {
    const selBudget = Math.floor(remainingBudget * 0.35)
    const bodyBudget = Math.max(0, remainingBudget - selBudget - (excerpt ? 512 : 0))
    selectedText = truncateStringToUtf8Bytes(selectedText, selBudget)
    contentText = truncateStringToUtf8Bytes(contentText, bodyBudget)
    if (excerpt) excerpt = truncateStringToUtf8Bytes(excerpt, 512)
  } else if (contentText) {
    const excBudget = excerpt ? Math.min(1024, Math.floor(remainingBudget * 0.2)) : 0
    const bodyBudget = Math.max(0, remainingBudget - excBudget)
    contentText = truncateStringToUtf8Bytes(contentText, bodyBudget)
    if (excerpt) excerpt = truncateStringToUtf8Bytes(excerpt, excBudget)
  } else if (selectedText && excerpt) {
    const selBudget = Math.floor(remainingBudget * 0.4)
    const excBudget = Math.max(0, remainingBudget - selBudget)
    selectedText = truncateStringToUtf8Bytes(selectedText, selBudget)
    excerpt = truncateStringToUtf8Bytes(excerpt, excBudget)
  } else if (selectedText) {
    selectedText = truncateStringToUtf8Bytes(selectedText, remainingBudget)
  } else if (excerpt) {
    excerpt = truncateStringToUtf8Bytes(excerpt, remainingBudget)
  }

  result = candidateWith(selectedText, contentText, excerpt)
  while (
    utf8ByteLength(JSON.stringify(result)) > maxBytes &&
    (contentText.length > 0 || excerpt.length > 0 || selectedText.length > 0)
  ) {
    if (contentText.length >= excerpt.length && contentText.length >= selectedText.length && contentText.length > 0) {
      contentText = contentText.slice(0, Math.max(0, contentText.length - 64))
    } else if (excerpt.length >= selectedText.length && excerpt.length > 0) {
      excerpt = excerpt.slice(0, Math.max(0, excerpt.length - 64))
    } else {
      selectedText = selectedText.slice(0, Math.max(0, selectedText.length - 64))
    }
    result = candidateWith(selectedText, contentText, excerpt)
  }

  return result
}

/**
 * Parses a single SSE block (`event: ...`, `data: ...`) into one of the 7 canonical
 * `ThreadStreamEvent` payloads (`seat_state | text | reasoning | tool | tool_result | result | error`).
 */
export function parseSseFrame(rawBlock: string): ThreadStreamEvent | null {
  const trimmed = rawBlock.trim()
  if (!trimmed) return null

  let eventHeader = ''
  const dataLines: string[] = []

  for (const rawLine of trimmed.split(/\r?\n/)) {
    const line = rawLine.trimEnd()
    if (!line || line.startsWith(':')) continue
    if (line.startsWith('event:')) {
      eventHeader = line.slice(6).trim()
    } else if (line.startsWith('data:')) {
      dataLines.push(line.slice(5).trimStart())
    }
  }

  if (dataLines.length === 0) return null
  const dataPayload = dataLines.join('\n').trim()
  if (!dataPayload || dataPayload === '[DONE]') return null

  let parsed: Record<string, unknown>
  try {
    const json = JSON.parse(dataPayload)
    if (!json || typeof json !== 'object') return null
    parsed = json as Record<string, unknown>
  } catch {
    return null
  }

  const rawKind = String(parsed.kind ?? parsed.type ?? eventHeader ?? '').trim() as ThreadStreamEventKind
  if (!VALID_STREAM_KINDS.has(rawKind)) {
    return null
  }

  const agentSlug =
    typeof parsed.agent_slug === 'string'
      ? parsed.agent_slug
      : typeof parsed.sender_slug === 'string'
        ? parsed.sender_slug
        : undefined

  const mergeId =
    typeof parsed.merge_id === 'string'
      ? parsed.merge_id
      : typeof parsed.call_id === 'string'
        ? parsed.call_id
        : undefined

  let approvalRequired: ApprovalRequiredMeta | null = null
  if (parsed.approval_required && typeof parsed.approval_required === 'object') {
    const ar = parsed.approval_required as Record<string, unknown>
    approvalRequired = {
      action_type: String(ar.action_type ?? 'newsman.brief.publish'),
      summary: String(ar.summary ?? ''),
      ...(typeof ar.request_ref === 'string' ? { request_ref: ar.request_ref } : {}),
    }
  }

  const resultText =
    typeof parsed.result === 'string'
      ? parsed.result
      : typeof parsed.output === 'string'
        ? parsed.output
        : Array.isArray(parsed.content)
          ? (parsed.content as Array<Record<string, unknown>>)
              .map((c) => (typeof c?.text === 'string' ? c.text : ''))
              .filter(Boolean)
              .join('\n')
          : parsed.result !== undefined
            ? JSON.stringify(parsed.result)
            : undefined

  return {
    kind: rawKind,
    type: rawKind,
    thread_id: (parsed.thread_id as number | string | undefined) ?? undefined,
    round_idx: typeof parsed.round_idx === 'number' ? parsed.round_idx : undefined,
    agent_slug: agentSlug,
    seat_state:
      typeof parsed.seat_state === 'string'
        ? parsed.seat_state
        : typeof parsed.state === 'string'
          ? parsed.state
          : undefined,
    delta: typeof parsed.delta === 'string' ? parsed.delta : undefined,
    text:
      typeof parsed.text === 'string'
        ? parsed.text
        : typeof parsed.content === 'string'
          ? parsed.content
          : undefined,
    content: typeof parsed.content === 'string' ? parsed.content : undefined,
    reasoning: typeof parsed.reasoning === 'string' ? parsed.reasoning : undefined,
    merge_id: mergeId,
    call_id: typeof parsed.call_id === 'string' ? parsed.call_id : mergeId,
    tool: typeof parsed.tool === 'string' ? parsed.tool : typeof parsed.name === 'string' ? parsed.name : undefined,
    arguments: (parsed.arguments as Record<string, unknown> | string | undefined) ?? undefined,
    result: resultText ?? parsed.result,
    is_error: Boolean(parsed.is_error),
    approval_required: approvalRequired,
    resource_ctx: (parsed.resource_ctx as ResourceCtx | null | undefined) ?? null,
    message: (parsed.message as AgentMessage | undefined) ?? undefined,
    error: typeof parsed.error === 'string' ? parsed.error : undefined,
  }
}

export function parseSseStreamChunk(
  buffer: string,
  chunk: string,
): { events: ThreadStreamEvent[]; remainder: string } {
  const combined = buffer + chunk
  const parts = combined.split(/\r?\n\r?\n/)
  const remainder = parts.pop() ?? ''
  const events: ThreadStreamEvent[] = []
  for (const block of parts) {
    const ev = parseSseFrame(block)
    if (ev) events.push(ev)
  }
  return { events, remainder }
}

export interface AgentDraftState {
  agent_slug: string
  text: string
  reasoning: string
  round_idx?: number
}

export interface StreamTurnState {
  seatStates: Record<string, string>
  drafts: Record<string, AgentDraftState>
  toolPills: MergedToolInvocation[]
  completedMessages: AgentMessage[]
  lastError: string | null
}

export function createInitialStreamTurnState(): StreamTurnState {
  return {
    seatStates: {},
    drafts: {},
    toolPills: [],
    completedMessages: [],
    lastError: null,
  }
}

/**
 * Reducer for the 7 SSE event kinds. Collapses matching `tool` + `tool_result`
 * events by `merge_id` into live `MergedToolInvocation` pills.
 */
export function applyStreamEvent(state: StreamTurnState, event: ThreadStreamEvent): StreamTurnState {
  const slug = event.agent_slug || 'assistant'

  switch (event.kind) {
    case 'seat_state': {
      return {
        ...state,
        seatStates: {
          ...state.seatStates,
          [slug]: event.seat_state ?? 'thinking',
        },
      }
    }

    case 'text': {
      const piece = event.delta ?? event.text ?? event.content ?? ''
      const prev = state.drafts[slug] ?? { agent_slug: slug, text: '', reasoning: '', round_idx: event.round_idx }
      return {
        ...state,
        seatStates: {
          ...state.seatStates,
          [slug]: 'speaking',
        },
        drafts: {
          ...state.drafts,
          [slug]: {
            ...prev,
            text: prev.text + piece,
            round_idx: event.round_idx ?? prev.round_idx,
          },
        },
      }
    }

    case 'reasoning': {
      const piece = event.delta ?? event.reasoning ?? event.text ?? ''
      const prev = state.drafts[slug] ?? { agent_slug: slug, text: '', reasoning: '', round_idx: event.round_idx }
      return {
        ...state,
        seatStates: {
          ...state.seatStates,
          [slug]: 'thinking',
        },
        drafts: {
          ...state.drafts,
          [slug]: {
            ...prev,
            reasoning: prev.reasoning + piece,
            round_idx: event.round_idx ?? prev.round_idx,
          },
        },
      }
    }

    case 'tool': {
      const mergeId = event.merge_id || event.call_id || `${slug}:${event.tool ?? 'tool'}:${state.toolPills.length + 1}`
      const existingIdx = state.toolPills.findIndex((p) => p.merge_id === mergeId)
      const nextPill: MergedToolInvocation = {
        merge_id: mergeId,
        call_id: event.call_id ?? mergeId,
        tool: event.tool ?? 'unknown_tool',
        agent_slug: slug,
        arguments: event.arguments,
        status: 'running',
        resource_ctx: event.resource_ctx ?? null,
      }
      const nextPills =
        existingIdx >= 0
          ? state.toolPills.map((p, idx) => (idx === existingIdx ? { ...p, ...nextPill } : p))
          : [...state.toolPills, nextPill]

      return {
        ...state,
        seatStates: {
          ...state.seatStates,
          [slug]: 'tool_calling',
        },
        toolPills: nextPills,
      }
    }

    case 'tool_result': {
      const mergeId = event.merge_id || event.call_id || `${slug}:${event.tool ?? 'tool'}`
      const existingIdx = state.toolPills.findIndex((p) => p.merge_id === mergeId)
      const status: MergedToolInvocation['status'] = event.approval_required
        ? 'approval_required'
        : event.is_error
          ? 'error'
          : 'done'
      const resultText =
        typeof event.result === 'string'
          ? event.result
          : event.result !== undefined
            ? JSON.stringify(event.result)
            : undefined

      let nextPills: MergedToolInvocation[]
      if (existingIdx >= 0) {
        nextPills = state.toolPills.map((pill, idx) =>
          idx === existingIdx
            ? {
                ...pill,
                status,
                is_error: Boolean(event.is_error),
                result_text: resultText ?? pill.result_text,
                approval_required: event.approval_required ?? pill.approval_required ?? null,
                resource_ctx: event.resource_ctx ?? pill.resource_ctx ?? null,
              }
            : pill,
        )
      } else {
        nextPills = [
          ...state.toolPills,
          {
            merge_id: mergeId,
            call_id: event.call_id ?? mergeId,
            tool: event.tool ?? 'tool',
            agent_slug: slug,
            arguments: event.arguments,
            status,
            is_error: Boolean(event.is_error),
            result_text: resultText,
            approval_required: event.approval_required ?? null,
            resource_ctx: event.resource_ctx ?? null,
          },
        ]
      }

      return {
        ...state,
        toolPills: nextPills,
      }
    }

    case 'result': {
      const draft = state.drafts[slug]
      const agentTools = state.toolPills.filter((p) => p.agent_slug === slug)
      const finalContent = event.message?.content ?? event.text ?? event.content ?? draft?.text ?? ''
      const finalReasoning = event.message?.reasoning ?? event.reasoning ?? draft?.reasoning ?? null
      const completedMsg: AgentMessage | null =
        event.message || finalContent || agentTools.length > 0
          ? {
              id: event.message?.id ?? `stream-${slug}-${Date.now()}`,
              thread_id: event.message?.thread_id ?? event.thread_id ?? 0,
              role: event.message?.role ?? 'assistant',
              sender_slug: event.message?.sender_slug ?? slug,
              content: finalContent,
              reasoning: finalReasoning,
              tool_calls:
                event.message?.tool_calls && event.message.tool_calls.length > 0
                  ? event.message.tool_calls
                  : agentTools,
              round_idx: event.message?.round_idx ?? event.round_idx ?? draft?.round_idx ?? null,
              created_at: event.message?.created_at ?? new Date().toISOString(),
            }
          : null

      const nextDrafts = { ...state.drafts }
      delete nextDrafts[slug]

      return {
        ...state,
        seatStates: {
          ...state.seatStates,
          [slug]: 'done',
        },
        drafts: nextDrafts,
        completedMessages: completedMsg ? [...state.completedMessages, completedMsg] : state.completedMessages,
      }
    }

    case 'error': {
      const errMsg = event.error ?? event.text ?? 'stream_error'
      return {
        ...state,
        seatStates: event.agent_slug
          ? {
              ...state.seatStates,
              [slug]: 'error',
            }
          : state.seatStates,
        lastError: errMsg,
      }
    }
  }
}

export function extractMentions(text: string, knownSlugs?: string[]): string[] {
  const regex = /@([a-zA-Z0-9_-]+)/g
  const found: string[] = []
  const allowed = knownSlugs && knownSlugs.length > 0 ? new Set(knownSlugs) : null
  let match: RegExpExecArray | null
  while ((match = regex.exec(text)) !== null) {
    const slug = match[1]
    if ((!allowed || allowed.has(slug)) && !found.includes(slug)) {
      found.push(slug)
    }
  }
  return found
}

export function stripSkillFrontmatter(md: string): string {
  const trimmed = md.trimStart()
  if (!trimmed.startsWith('---')) return md
  const afterOpen = trimmed.slice(3)
  if (!afterOpen.startsWith('\n') && !afterOpen.startsWith('\r\n')) return md
  const body = afterOpen.replace(/^\r?\n/, '')
  const closeIdx = body.search(/\r?\n---(?:\r?\n|$)/)
  if (closeIdx === -1) return md
  const afterFence = body.slice(closeIdx).replace(/^\r?\n---/, '')
  return afterFence.replace(/^(?:\r?\n)+/, '')
}

export function skillDisplayName(sk: Pick<HubSkillSummary, 'slug' | 'name' | 'display_name'>): string {
  return sk.display_name?.trim() || sk.name?.trim() || sk.slug
}

export function skillGroupDisplayName(
  grp: Pick<HubSkillGroupSummary, 'slug' | 'name' | 'display_name'>,
): string {
  return grp.display_name?.trim() || grp.name?.trim() || grp.slug
}

function dedupScopeList(items: readonly string[]): string[] {
  const out: string[] = []
  for (const item of items) {
    const clean = item.trim()
    if (clean && !out.includes(clean)) out.push(clean)
  }
  return out
}

export function intersectScopeLists(
  wsScope: readonly string[] | null | undefined,
  agentScope: readonly string[] | null | undefined,
): string[] | undefined {
  if (wsScope == null && agentScope == null) return undefined
  if (wsScope != null && agentScope == null) return dedupScopeList(wsScope)
  if (wsScope == null && agentScope != null) return dedupScopeList(agentScope)
  const cleanB = dedupScopeList(agentScope!)
  return dedupScopeList(wsScope!).filter((item) => cleanB.includes(item))
}

export interface EffectiveScopeSummary {
  regions?: string[]
  topics?: string[]
  disjointRegions: boolean
  disjointTopics: boolean
}

export function computeEffectiveScope(
  workspace: AgentWorkspace | null | undefined,
  agents: readonly AppAgentConfig[],
): EffectiveScopeSummary {
  const wsRegions = workspace?.resource_ctx?.regions
  const wsTopics = workspace?.resource_ctx?.topics
  if (agents.length === 0) {
    return {
      regions: wsRegions != null ? dedupScopeList(wsRegions) : undefined,
      topics: wsTopics != null ? dedupScopeList(wsTopics) : undefined,
      disjointRegions: false,
      disjointTopics: false,
    }
  }
  const perSeatRegions = agents.map((ag) =>
    intersectScopeLists(wsRegions, ag.default_resource_ctx?.regions),
  )
  const perSeatTopics = agents.map((ag) =>
    intersectScopeLists(wsTopics, ag.default_resource_ctx?.topics),
  )
  const regions = perSeatRegions.some((r) => r === undefined)
    ? undefined
    : dedupScopeList(perSeatRegions.flatMap((r) => r ?? []))
  const topics = perSeatTopics.some((t) => t === undefined)
    ? undefined
    : dedupScopeList(perSeatTopics.flatMap((t) => t ?? []))
  return {
    regions,
    topics,
    disjointRegions: regions !== undefined && regions.length === 0,
    disjointTopics: topics !== undefined && topics.length === 0,
  }
}

export function buildSeatTraceId(msg: {
  id: number | string
  sender_slug?: string | null
  round_idx?: number | null
}): string {
  const slug = (msg.sender_slug ?? 'seat').replace(/[^a-zA-Z0-9_-]/g, '_')
  const round = msg.round_idx ?? 0
  return `trace-${String(msg.id)}-${slug}-${round}`
}

export function parseComposerTrigger(
  text: string,
  caretIndex: number = text.length,
): { trigger: '@' | '/'; query: string; start: number; end: number } | null {
  const end = Math.max(0, Math.min(caretIndex, text.length))
  const before = text.slice(0, end)
  const match = /(^|\s)([@/])([a-zA-Z0-9_-]*)$/.exec(before)
  if (!match) return null
  const trigger = match[2] as '@' | '/'
  const query = match[3] ?? ''
  const start = end - query.length - 1
  return { trigger, query, start, end }
}

function normalizeAgent(raw: Record<string, unknown>): AppAgentConfig {
  const parseArr = (val: unknown): string[] => {
    if (Array.isArray(val)) return val.map(String)
    if (typeof val === 'string' && val.trim()) {
      try {
        const p = JSON.parse(val)
        return Array.isArray(p) ? p.map(String) : []
      } catch {
        return []
      }
    }
    return []
  }
  const parseObj = <T extends object>(val: unknown, fallback: T): T => {
    if (val && typeof val === 'object' && !Array.isArray(val)) return val as T
    if (typeof val === 'string' && val.trim()) {
      try {
        const p = JSON.parse(val)
        return p && typeof p === 'object' ? (p as T) : fallback
      } catch {
        return fallback
      }
    }
    return fallback
  }

  return {
    slug: String(raw.slug ?? ''),
    display_name: String(raw.display_name ?? raw.slug ?? ''),
    description: String(raw.description ?? ''),
    emoji: String(raw.emoji ?? '📈'),
    color: String(raw.color ?? '#236466'),
    avatar_url: typeof raw.avatar_url === 'string' ? raw.avatar_url : null,
    reply_language: (raw.reply_language as AppAgentConfig['reply_language']) ?? 'vi',
    soul_md: String(raw.soul_md ?? ''),
    app_role_addendum_md: String(raw.app_role_addendum_md ?? ''),
    sync_blueprint_to_hub: Boolean(raw.sync_blueprint_to_hub ?? true),
    runtime_kind: (raw.runtime_kind as AppAgentConfig['runtime_kind']) ?? 'partner_fast',
    primary_model: String(raw.primary_model ?? 'gpt-5.4-mini'),
    backup_model: String(raw.backup_model ?? ''),
    reasoning_effort: String(raw.reasoning_effort ?? 'medium'),
    remote_endpoint_cfg: parseObj(raw.remote_endpoint_cfg ?? raw.remote_endpoint_cfg_json, {}),
    max_tool_steps: typeof raw.max_tool_steps === 'number' ? raw.max_tool_steps : Number(raw.max_tool_steps ?? 8) || 8,
    enabled_app_tools: parseArr(raw.enabled_app_tools ?? raw.enabled_app_tools_json),
    enabled_mcp_tools: parseArr(raw.enabled_mcp_tools ?? raw.enabled_mcp_tools_json),
    enabled_skills: parseArr(raw.enabled_skills ?? raw.enabled_skills_json),
    enabled_skill_groups: parseArr(raw.enabled_skill_groups ?? raw.enabled_skill_groups_json),
    default_resource_ctx: parseObj<ResourceCtx>(raw.default_resource_ctx ?? raw.default_resource_ctx_json, {
      regions: ['global', 'vn'],
    }),
    memory_mode: (raw.memory_mode as AppAgentConfig['memory_mode']) ?? 'app_isolated',
    updated_at: typeof raw.updated_at === 'string' ? raw.updated_at : null,
  }
}

function normalizeThread(raw: Record<string, unknown>): AgentThread {
  const parseArr = (val: unknown): string[] => {
    if (Array.isArray(val)) return val.map(String)
    if (typeof val === 'string' && val.trim()) {
      try {
        const p = JSON.parse(val)
        return Array.isArray(p) ? p.map(String) : []
      } catch {
        return []
      }
    }
    return []
  }
  const memberSlugs = parseArr(raw.member_slugs ?? raw.member_slugs_json)
  const pinnedSkills = parseArr(raw.pinned_skills ?? raw.pinned_skills_json)
  return {
    id: (raw.id as number | string) ?? 0,
    kind: raw.kind === 'group' ? 'group' : 'direct',
    title: String(raw.title ?? ''),
    article_id: typeof raw.article_id === 'number' ? raw.article_id : null,
    report_id: typeof raw.report_id === 'number' ? raw.report_id : null,
    member_slugs: memberSlugs,
    discussion_mode: (raw.discussion_mode as DiscussionMode) ?? 'parallel_panel',
    lead_slug: typeof raw.lead_slug === 'string' ? raw.lead_slug : null,
    workspace_id: typeof raw.workspace_id === 'string' && raw.workspace_id ? raw.workspace_id : null,
    pinned_skills: pinnedSkills,
    model_override:
      typeof raw.model_override === 'string' && raw.model_override ? raw.model_override : null,
    created_at: typeof raw.created_at === 'string' ? raw.created_at : undefined,
    updated_at: typeof raw.updated_at === 'string' ? raw.updated_at : undefined,
  }
}

function normalizeMessage(raw: Record<string, unknown>): AgentMessage {
  let toolCalls: MergedToolInvocation[] = []
  const rawTools = raw.tool_calls ?? raw.tool_calls_json
  if (Array.isArray(rawTools)) {
    toolCalls = rawTools as MergedToolInvocation[]
  } else if (typeof rawTools === 'string' && rawTools.trim()) {
    try {
      const parsed = JSON.parse(rawTools)
      if (Array.isArray(parsed)) toolCalls = parsed as MergedToolInvocation[]
    } catch {
      /* ignore */
    }
  }

  let context: ActiveViewContext | null = null
  const rawCtx = raw.context ?? raw.context_json
  if (rawCtx && typeof rawCtx === 'object') {
    context = rawCtx as ActiveViewContext
  } else if (typeof rawCtx === 'string' && rawCtx.trim()) {
    try {
      const parsed = JSON.parse(rawCtx)
      if (parsed && typeof parsed === 'object') context = parsed as ActiveViewContext
    } catch {
      /* ignore */
    }
  }

  return {
    id: (raw.id as number | string) ?? `${Date.now()}-${Math.random()}`,
    thread_id: (raw.thread_id as number | string) ?? 0,
    role: String(raw.role ?? 'assistant'),
    sender_slug: typeof raw.sender_slug === 'string' ? raw.sender_slug : null,
    content: String(raw.content ?? raw.body ?? ''),
    reasoning: typeof raw.reasoning === 'string' && raw.reasoning ? raw.reasoning : null,
    tool_calls: toolCalls,
    context,
    round_idx: typeof raw.round_idx === 'number' ? raw.round_idx : null,
    created_at: String(raw.created_at ?? new Date().toISOString()),
  }
}

export function emptyHubEffectiveBundle(): HubEffectiveBundleCache {
  return {
    astraea_bridge_ready: false,
    ws_relay_path: '',
    app_agents_proxy_path: '',
    mcp_endpoint_path: '',
    llm_base_path: '',
    telegram_send_path: '',
    telegram_replies_path: '',
    capability_bundles: [],
    mcp_groups: [],
    allowed_mcp_tools: [],
    allowed_model_aliases: [],
    channel_grants: [],
    synced_at: '',
  }
}

export function normalizeBridgeRouting(raw: unknown): BridgeRoutingSummary {
  const obj = raw && typeof raw === 'object' ? (raw as Record<string, unknown>) : {}
  return {
    astraea_http: String(obj.astraea_http ?? 'none'),
    astraea_ws: String(obj.astraea_ws ?? 'none'),
    models: String(obj.models ?? 'none'),
    skills_and_mcp: String(obj.skills_and_mcp ?? 'none'),
    telegram: String(obj.telegram ?? 'disabled'),
  }
}

export function normalizeHubEffectiveBundle(raw: unknown): HubEffectiveBundleCache {
  if (!raw || typeof raw !== 'object') {
    return emptyHubEffectiveBundle()
  }
  const obj = raw as Record<string, unknown>
  const nested =
    obj.hub_bundle && typeof obj.hub_bundle === 'object'
      ? (obj.hub_bundle as Record<string, unknown>)
      : obj
  const toStrArr = (v: unknown): string[] =>
    Array.isArray(v) ? v.map((x) => String(x ?? '').trim()).filter(Boolean) : []
  const rawBundles = nested.capability_bundles ?? obj.capability_bundles
  const capability_bundles: HubCapabilityBundleSummary[] = Array.isArray(rawBundles)
    ? rawBundles
        .filter((b): b is Record<string, unknown> => Boolean(b && typeof b === 'object'))
        .map((b) => {
          const st = String(b.status ?? '')
          return {
            slug: String(b.slug ?? b.bundle_slug ?? b.id ?? '').trim(),
            name: String(b.name ?? b.display_name ?? b.slug ?? '').trim(),
            display_name: b.display_name ? String(b.display_name) : undefined,
            description: b.description ? String(b.description) : undefined,
            icon: b.icon ? String(b.icon) : undefined,
            skill_group_slugs: toStrArr(b.skill_group_slugs),
            mcp_group_slugs: toStrArr(b.mcp_group_slugs),
            allowed_model_aliases: toStrArr(b.allowed_model_aliases),
            allowed_channels: toStrArr(b.allowed_channels),
            status: st,
            granted:
              Boolean(b.granted) ||
              ['granted', 'approved', 'active', 'enabled'].includes(st.trim().toLowerCase()),
          }
        })
        .filter((b) => b.slug.length > 0)
    : []
  const rawMcpGroups = nested.mcp_groups ?? obj.mcp_groups
  const mcp_groups: HubMcpGroupSummary[] = Array.isArray(rawMcpGroups)
    ? rawMcpGroups
        .filter((g): g is Record<string, unknown> => Boolean(g && typeof g === 'object'))
        .map((g) => {
          const patterns = toStrArr(g.patterns)
          const st = String(g.status ?? '')
          return {
            slug: String(g.slug ?? g.group_slug ?? g.id ?? '').trim(),
            name: String(g.name ?? g.display_name ?? g.slug ?? '').trim(),
            display_name: g.display_name ? String(g.display_name) : undefined,
            description: g.description ? String(g.description) : undefined,
            category: g.category ? String(g.category) : undefined,
            icon: g.icon ? String(g.icon) : undefined,
            status: st,
            patterns,
            item_count: typeof g.item_count === 'number' ? g.item_count : patterns.length,
            granted:
              Boolean(g.granted) ||
              ['granted', 'approved', 'active', 'enabled'].includes(st.trim().toLowerCase()),
          }
        })
        .filter((g) => g.slug.length > 0)
    : []

  return {
    astraea_bridge_ready: Boolean(nested.astraea_bridge_ready ?? obj.astraea_bridge_ready),
    ws_relay_path: String(nested.ws_relay_path ?? ''),
    app_agents_proxy_path: String(nested.app_agents_proxy_path ?? ''),
    mcp_endpoint_path: String(nested.mcp_endpoint_path ?? ''),
    llm_base_path: String(nested.llm_base_path ?? ''),
    telegram_send_path: String(nested.telegram_send_path ?? ''),
    telegram_replies_path: String(nested.telegram_replies_path ?? ''),
    capability_bundles,
    mcp_groups,
    allowed_mcp_tools: toStrArr(nested.allowed_mcp_tools ?? obj.allowed_mcp_tools),
    allowed_model_aliases: toStrArr(nested.allowed_model_aliases ?? obj.allowed_model_aliases),
    channel_grants: toStrArr(nested.channel_grants ?? obj.channel_grants),
    synced_at: String(nested.synced_at ?? ''),
  }
}

export function useAppAgents() {
  const [bridgeStatus, setBridgeStatus] = useState<BridgeStatus>({
    mode: 'hub_standby',
    relay_connected: false,
    astraea_reachable: false,
    relay_route: 'disabled',
    routing: normalizeBridgeRouting(null),
    hub_bundle: emptyHubEffectiveBundle(),
    astraea_url: '',
    hub_url: '',
    default_model: 'gpt-5.4-mini',
    has_astraea_token: false,
    has_hub_token: false,
    has_telegram_token: false,
    has_cygnus_key: false,
  })
  const [agents, setAgents] = useState<AppAgentConfig[]>(() => [...DEFAULT_SEED_AGENTS])
  const [models, setModels] = useState<string[]>([
    'gpt-5.4-mini',
    'gpt-5.4',
    'claude-3-7-sonnet',
    'gemini-2.5-pro',
    'deepseek-r1',
  ])
  const [threads, setThreads] = useState<AgentThread[]>([])
  const [schedules, setSchedules] = useState<AgentSchedule[]>([])
  const [telegramConfig, setTelegramConfig] = useState<TelegramBridgeConfig>({
    transport_mode: 'direct_poll',
    telegram_route: 'disabled',
    bot_name: '',
    cygnus_url: '',
    chat_policy: 'mention',
    has_bot_token: false,
    has_webhook_secret: false,
    has_cygnus_key: false,
  })
  const [telegramRoutes, setTelegramRoutes] = useState<TelegramRouteConfig[]>([])
  const [workspaces, setWorkspaces] = useState<AgentWorkspace[]>([])
  const [activeWorkspaceId, setActiveWorkspaceId] = useState<string | null>(null)
  const [skills, setSkills] = useState<HubSkillSummary[]>([])
  const [skillGroups, setSkillGroups] = useState<HubSkillGroupSummary[]>([])
  const [hubBundle, setHubBundle] = useState<HubEffectiveBundleCache>(() => emptyHubEffectiveBundle())
  const [accessRequests, setAccessRequests] = useState<SkillAccessRequest[]>([])
  const [probeReport, setProbeReport] = useState<ConnectionProbeReport | null>(null)
  const [probingConnections, setProbingConnections] = useState(false)
  const [syncingSkills, setSyncingSkills] = useState(false)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const loadBridgeStatus = useCallback(async () => {
    try {
      const r = await appAgentsApi<Record<string, unknown>>('/api/app-agents/bridge-config')
      const statusObj = (r.status && typeof r.status === 'object' ? r.status : r) as Record<string, unknown>
      const rawMode = String(statusObj.mode ?? r.mode ?? 'hub_standby')
      const mode: BridgeStatus['mode'] =
        rawMode === 'astraea_relay' || rawMode === 'hub_standby' || rawMode === 'disconnected'
          ? rawMode
          : 'hub_standby'
      const parsedBundle = normalizeHubEffectiveBundle(r)
      const routing = normalizeBridgeRouting(r.routing)
      setHubBundle(parsedBundle)
      setBridgeStatus({
        mode,
        relay_connected: Boolean(statusObj.relay_connected ?? r.relay_connected),
        astraea_reachable: Boolean(statusObj.astraea_reachable ?? r.astraea_reachable),
        relay_route: String(statusObj.relay_route ?? r.relay_route ?? routing.astraea_ws ?? 'disabled'),
        routing,
        hub_bundle: parsedBundle,
        secret_store_open: Boolean(r.secret_store_open ?? true),
        astraea_url: String(r.astraea_url ?? ''),
        hub_url: String(r.hub_url ?? ''),
        default_model: String(r.default_model ?? 'gpt-5.4-mini'),
        llm_base_url: String(r.llm_base_url ?? ''),
        allowed_hosts: Array.isArray(r.allowed_hosts) ? r.allowed_hosts.map(String) : [],
        task_models: (r.task_models && typeof r.task_models === 'object'
          ? (r.task_models as TaskModelsConfig)
          : {}) as TaskModelsConfig,
        has_astraea_token: Boolean(r.has_astraea_token),
        has_hub_token: Boolean(r.has_hub_token),
        has_llm_api_key: Boolean(r.has_llm_api_key),
        has_telegram_token: Boolean(r.has_telegram_token),
        has_cygnus_key: Boolean(r.has_cygnus_key),
      })
    } catch {
      /* keep standby defaults when offline */
    }
  }, [])

  const loadModels = useCallback(async () => {
    try {
      const r = await appAgentsApi<{ models?: string[]; items?: string[] }>('/api/app-agents/models')
      const list = r.models ?? r.items ?? []
      if (Array.isArray(list) && list.length > 0) {
        setModels(list.map(String))
      }
    } catch {
      /* keep default model aliases */
    }
  }, [])

  const loadAgents = useCallback(async () => {
    try {
      const r = await appAgentsApi<{ items?: Record<string, unknown>[]; agents?: Record<string, unknown>[] }>(
        '/api/app-agents/agents',
      )
      const list = r.items ?? r.agents ?? (Array.isArray(r) ? r : [])
      if (list.length > 0) {
        setAgents(list.map(normalizeAgent))
      }
    } catch {
      /* fallback to seed agents */
    }
  }, [])

  const loadThreads = useCallback(async (articleId?: number | null) => {
    try {
      const qs = typeof articleId === 'number' ? `?article_id=${articleId}` : ''
      const r = await appAgentsApi<{ items?: Record<string, unknown>[]; threads?: Record<string, unknown>[] }>(
        `/api/app-agents/threads${qs}`,
      )
      const list = r.items ?? r.threads ?? (Array.isArray(r) ? r : [])
      setThreads(list.map(normalizeThread))
    } catch {
      /* ignore */
    }
  }, [])

  const loadSchedules = useCallback(async () => {
    try {
      const r = await appAgentsApi<{ items?: AgentSchedule[]; schedules?: AgentSchedule[] }>(
        '/api/app-agents/schedules',
      )
      setSchedules(r.items ?? r.schedules ?? [])
    } catch {
      /* ignore */
    }
  }, [])

  const loadTelegram = useCallback(async () => {
    try {
      const [cfgRes, routesRes] = await Promise.all([
        appAgentsApi<Record<string, unknown>>('/api/app-agents/telegram/config').catch(() => null),
        appAgentsApi<{ items?: TelegramRouteConfig[]; routes?: TelegramRouteConfig[] }>(
          '/api/app-agents/telegram/routes',
        ).catch(() => null),
      ])
      if (cfgRes) {
        const c = (cfgRes.config && typeof cfgRes.config === 'object' ? cfgRes.config : cfgRes) as Record<
          string,
          unknown
        >
        setTelegramConfig({
          transport_mode: (c.transport_mode as TelegramBridgeConfig['transport_mode']) ?? 'direct_poll',
          telegram_route: String(c.telegram_route ?? cfgRes.telegram_route ?? 'disabled'),
          bot_name: String(c.bot_name ?? ''),
          cygnus_url: String(c.cygnus_url ?? ''),
          chat_policy: (c.chat_policy as TelegramBridgeConfig['chat_policy']) ?? 'mention',
          has_bot_token: Boolean(c.has_bot_token ?? cfgRes.has_telegram_token),
          has_webhook_secret: Boolean(c.has_webhook_secret),
          has_cygnus_key: Boolean(c.has_cygnus_key ?? cfgRes.has_cygnus_key),
        })
      }
      if (routesRes) {
        setTelegramRoutes(routesRes.items ?? routesRes.routes ?? [])
      }
    } catch {
      /* ignore */
    }
  }, [])

  const loadWorkspaces = useCallback(async () => {
    try {
      const r = await appAgentsApi<{ items?: AgentWorkspace[]; workspaces?: AgentWorkspace[] }>(
        '/api/app-agents/workspaces',
      )
      const list = r.items ?? r.workspaces ?? []
      setWorkspaces(list)
      setActiveWorkspaceId((prev) => {
        if (prev && list.some((w) => w.id === prev)) return prev
        const def = list.find((w) => w.is_default) ?? list[0]
        return def ? def.id : null
      })
    } catch {
      /* ignore when offline */
    }
  }, [])

  const loadSkills = useCallback(async () => {
    try {
      const r = await appAgentsApi<Record<string, unknown> & {
        items?: HubSkillSummary[]
        skills?: HubSkillSummary[]
        groups?: HubSkillGroupSummary[]
        access_requests?: SkillAccessRequest[]
      }>('/api/app-agents/skills')
      setSkills(r.items ?? r.skills ?? [])
      setSkillGroups(r.groups ?? [])
      if (Array.isArray(r.access_requests)) {
        setAccessRequests(r.access_requests)
      }
      setHubBundle(normalizeHubEffectiveBundle(r))
    } catch {
      /* ignore */
    }
  }, [])

  const loadAccessRequests = useCallback(async () => {
    try {
      const r = await appAgentsApi<{ items?: SkillAccessRequest[]; requests?: SkillAccessRequest[] }>(
        '/api/app-agents/skills/access-requests',
      )
      setAccessRequests(r.items ?? r.requests ?? [])
    } catch {
      /* ignore */
    }
  }, [])

  const refreshAll = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      await Promise.all([
        loadBridgeStatus(),
        loadModels(),
        loadAgents(),
        loadThreads(),
        loadWorkspaces(),
        loadSkills(),
        loadAccessRequests(),
        loadSchedules(),
        loadTelegram(),
      ])
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load app agents')
    } finally {
      setLoading(false)
    }
  }, [
    loadAccessRequests,
    loadAgents,
    loadBridgeStatus,
    loadModels,
    loadSchedules,
    loadSkills,
    loadTelegram,
    loadThreads,
    loadWorkspaces,
  ])

  useEffect(() => {
    void refreshAll()
  }, [refreshAll])

  const saveBridgeConfig = useCallback(
    async (patch: {
      astraea_url?: string
      hub_url?: string
      default_model?: string
      llm_base_url?: string
      allowed_hosts?: string[]
      task_models?: TaskModelsConfig
      astraea_token?: string
      hub_token?: string
      llm_api_key?: string
    }) => {
      await appAgentsApi('/api/app-agents/bridge-config', {
        method: 'PUT',
        body: JSON.stringify(patch),
      })
      await Promise.all([loadBridgeStatus(), loadModels(), loadSkills(), loadTelegram()])
    },
    [loadBridgeStatus, loadModels, loadSkills, loadTelegram],
  )

  const probeConnections = useCallback(async (): Promise<ConnectionProbeReport> => {
    setProbingConnections(true)
    try {
      const report = await appAgentsApi<ConnectionProbeReport>('/api/app-agents/bridge-config/probe', {
        method: 'POST',
        body: JSON.stringify({}),
      })
      setProbeReport(report)
      await loadBridgeStatus()
      return report
    } finally {
      setProbingConnections(false)
    }
  }, [loadBridgeStatus])

  const saveAgent = useCallback(
    async (agent: AppAgentConfig) => {
      const slug = agent.slug.trim()
      if (!slug) throw new Error('Agent slug is required')
      if (
        agent.remote_endpoint_cfg?.secret_ref &&
        !agent.remote_endpoint_cfg.secret_ref.trim().startsWith('vault://')
      ) {
        throw new Error('secret_ref must use a vault:// URI')
      }
      const res = await appAgentsApi<{ agent?: Record<string, unknown>; item?: Record<string, unknown> }>(
        `/api/app-agents/agents/${encodeURIComponent(slug)}`,
        {
          method: 'PUT',
          body: JSON.stringify(agent),
        },
      )
      const saved = res.agent ?? res.item
      if (saved) {
        const norm = normalizeAgent(saved)
        setAgents((prev) =>
          prev.some((a) => a.slug === norm.slug) ? prev.map((a) => (a.slug === norm.slug ? norm : a)) : [...prev, norm],
        )
      } else {
        await loadAgents()
      }
    },
    [loadAgents],
  )

  const deleteAgent = useCallback(async (slug: string) => {
    await appAgentsApi(`/api/app-agents/agents/${encodeURIComponent(slug)}`, {
      method: 'DELETE',
    })
    setAgents((prev) => prev.filter((a) => a.slug !== slug))
  }, [])

  const createThread = useCallback(
    async (input: {
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
    }): Promise<AgentThread> => {
      const res = await appAgentsApi<
        { thread?: Record<string, unknown>; item?: Record<string, unknown> } & Record<string, unknown>
      >('/api/app-agents/threads', {
        method: 'POST',
        body: JSON.stringify(input),
      })
      const raw = res.thread ?? res.item ?? res
      const created = normalizeThread(raw)
      setThreads((prev) => [created, ...prev.filter((t) => String(t.id) !== String(created.id))])
      return created
    },
    [],
  )

  const updateThread = useCallback(
    async (
      threadId: number | string,
      patch: Partial<
        Pick<
          AgentThread,
          'title' | 'member_slugs' | 'discussion_mode' | 'lead_slug' | 'workspace_id' | 'pinned_skills' | 'model_override'
        >
      >,
    ) => {
      const res = await appAgentsApi<
        { thread?: Record<string, unknown>; item?: Record<string, unknown> } & Record<string, unknown>
      >(`/api/app-agents/threads/${encodeURIComponent(String(threadId))}`, {
        method: 'PATCH',
        body: JSON.stringify(patch),
      })
      const raw = res.thread ?? res.item ?? res
      const updated = normalizeThread(raw)
      setThreads((prev) => prev.map((t) => (String(t.id) === String(threadId) ? updated : t)))
      return updated
    },
    [],
  )

  const deleteThread = useCallback(async (threadId: number | string) => {
    await appAgentsApi(`/api/app-agents/threads/${encodeURIComponent(String(threadId))}`, {
      method: 'DELETE',
    })
    setThreads((prev) => prev.filter((t) => String(t.id) !== String(threadId)))
  }, [])

  const createSchedule = useCallback(
    async (input: {
      name: string
      cron_or_interval_secs: string
      agent_slug: string
      thread_id?: number | string | null
      prompt: string
      enabled?: boolean
    }) => {
      await appAgentsApi('/api/app-agents/schedules', {
        method: 'POST',
        body: JSON.stringify({ ...input, enabled: input.enabled ?? true }),
      })
      await loadSchedules()
    },
    [loadSchedules],
  )

  const updateSchedule = useCallback(
    async (id: number | string, patch: Partial<AgentSchedule>) => {
      await appAgentsApi(`/api/app-agents/schedules/${encodeURIComponent(String(id))}`, {
        method: 'PATCH',
        body: JSON.stringify(patch),
      })
      await loadSchedules()
    },
    [loadSchedules],
  )

  const deleteSchedule = useCallback(async (id: number | string) => {
    await appAgentsApi(`/api/app-agents/schedules/${encodeURIComponent(String(id))}`, {
      method: 'DELETE',
    })
    setSchedules((prev) => prev.filter((s) => String(s.id) !== String(id)))
  }, [])

  const runScheduleNow = useCallback(
    async (id: number | string) => {
      await appAgentsApi(`/api/app-agents/schedules/${encodeURIComponent(String(id))}/run-now`, {
        method: 'POST',
        body: JSON.stringify({}),
      })
      await loadSchedules()
    },
    [loadSchedules],
  )

  const saveTelegramConfig = useCallback(
    async (input: {
      transport_mode: TelegramBridgeConfig['transport_mode']
      bot_name: string
      cygnus_url: string
      chat_policy: TelegramBridgeConfig['chat_policy']
      bot_token?: string
      webhook_secret?: string
      cygnus_key?: string
    }) => {
      await appAgentsApi('/api/app-agents/telegram/config', {
        method: 'PUT',
        body: JSON.stringify(input),
      })
      await Promise.all([loadTelegram(), loadBridgeStatus()])
    },
    [loadBridgeStatus, loadTelegram],
  )

  const createTelegramRoute = useCallback(
    async (input: Omit<TelegramRouteConfig, 'id'>) => {
      await appAgentsApi('/api/app-agents/telegram/routes', {
        method: 'POST',
        body: JSON.stringify(input),
      })
      await loadTelegram()
    },
    [loadTelegram],
  )

  const deleteTelegramRoute = useCallback(async (id: number | string) => {
    await appAgentsApi(`/api/app-agents/telegram/routes/${encodeURIComponent(String(id))}`, {
      method: 'DELETE',
    })
    setTelegramRoutes((prev) => prev.filter((r) => String(r.id) !== String(id)))
  }, [])

  const saveWorkspace = useCallback(
    async (ws: Partial<AgentWorkspace> & { name: string }): Promise<AgentWorkspace> => {
      const hasId = Boolean(ws.id && ws.id.trim())
      const url = hasId
        ? `/api/app-agents/workspaces/${encodeURIComponent(ws.id!.trim())}`
        : '/api/app-agents/workspaces'
      const res = await appAgentsApi<{ workspace?: AgentWorkspace; item?: AgentWorkspace }>(url, {
        method: hasId ? 'PUT' : 'POST',
        body: JSON.stringify(ws),
      })
      const saved = res.workspace ?? res.item
      await loadWorkspaces()
      if (!saved) throw new Error('Workspace response missing item')
      return saved
    },
    [loadWorkspaces],
  )

  const deleteWorkspace = useCallback(
    async (id: string) => {
      await appAgentsApi(`/api/app-agents/workspaces/${encodeURIComponent(id)}`, {
        method: 'DELETE',
      })
      await loadWorkspaces()
    },
    [loadWorkspaces],
  )

  const syncSkills = useCallback(async () => {
    setSyncingSkills(true)
    try {
      const res = await appAgentsApi<Record<string, unknown> & {
        synced?: boolean
        bundle_synced?: boolean
        agents_synced?: number
        sync_mode?: string
        offline?: boolean
        items?: HubSkillSummary[]
        skills?: HubSkillSummary[]
        groups?: HubSkillGroupSummary[]
        access_requests?: SkillAccessRequest[]
      }>('/api/app-agents/skills/sync', {
        method: 'POST',
        body: JSON.stringify({}),
      })
      setSkills(res.items ?? res.skills ?? [])
      setSkillGroups(res.groups ?? [])
      if (Array.isArray(res.access_requests)) {
        setAccessRequests(res.access_requests)
      }
      setHubBundle(normalizeHubEffectiveBundle(res))
      await Promise.all([loadBridgeStatus(), loadModels(), loadTelegram()])
      return res
    } finally {
      setSyncingSkills(false)
    }
  }, [loadBridgeStatus, loadModels, loadTelegram])

  const toggleSkillEnabled = useCallback(async (slug: string, enabled: boolean) => {
    const res = await appAgentsApi<{ skill?: HubSkillSummary; item?: HubSkillSummary }>(
      `/api/app-agents/skills/${encodeURIComponent(slug)}`,
      {
        method: 'PATCH',
        body: JSON.stringify({ enabled }),
      },
    )
    const updated = res.skill ?? res.item
    if (updated) {
      setSkills((prev) => prev.map((s) => (s.slug === slug ? updated : s)))
    } else {
      setSkills((prev) => prev.map((s) => (s.slug === slug ? { ...s, enabled } : s)))
    }
  }, [])

  const requestSkillAccess = useCallback(
    async (input: {
      target_kind: AccessRequestTargetKind | string
      target_slug: string
      reason: string
    }) => {
      const res = await appAgentsApi<{ request?: SkillAccessRequest; item?: SkillAccessRequest }>(
        '/api/app-agents/skills/access-requests',
        {
          method: 'POST',
          body: JSON.stringify(input),
        },
      )
      const created = res.request ?? res.item
      if (created) {
        setAccessRequests((prev) => [created, ...prev.filter((r) => r.id !== created.id)])
      } else {
        await loadAccessRequests()
      }
      return created ?? null
    },
    [loadAccessRequests],
  )

  return {
    bridgeStatus,
    agents,
    models,
    threads,
    workspaces,
    activeWorkspaceId,
    setActiveWorkspaceId,
    skills,
    skillGroups,
    hubBundle,
    capabilityBundles: hubBundle.capability_bundles,
    mcpGroups: hubBundle.mcp_groups,
    allowedMcpTools: hubBundle.allowed_mcp_tools,
    allowedModelAliases: hubBundle.allowed_model_aliases,
    channelGrants: hubBundle.channel_grants,
    accessRequests,
    probeReport,
    probingConnections,
    syncingSkills,
    schedules,
    telegramConfig,
    telegramRoutes,
    loading,
    error,
    refreshAll,
    loadThreads,
    loadWorkspaces,
    loadSkills,
    loadAccessRequests,
    saveBridgeConfig,
    probeConnections,
    saveAgent,
    deleteAgent,
    createThread,
    updateThread,
    deleteThread,
    saveWorkspace,
    deleteWorkspace,
    syncSkills,
    toggleSkillEnabled,
    requestSkillAccess,
    createSchedule,
    updateSchedule,
    deleteSchedule,
    runScheduleNow,
    saveTelegramConfig,
    createTelegramRoute,
    deleteTelegramRoute,
  }
}

export function useThreadStream(threadId: number | string | null) {
  const [messages, setMessages] = useState<AgentMessage[]>([])
  const [whiteboard, setWhiteboard] = useState<WhiteboardNote[]>([])
  const [streamState, setStreamState] = useState<StreamTurnState>(() => createInitialStreamTurnState())
  const [streamConnected, setStreamConnected] = useState(false)
  const [sending, setSending] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const abortRef = useRef<AbortController | null>(null)

  const reloadMessages = useCallback(async (id: number | string) => {
    try {
      const r = await appAgentsApi<{ items?: Record<string, unknown>[]; messages?: Record<string, unknown>[] }>(
        `/api/app-agents/threads/${encodeURIComponent(String(id))}/messages`,
      )
      const list = r.items ?? r.messages ?? []
      setMessages(list.map(normalizeMessage))
    } catch {
      /* ignore */
    }
  }, [])

  const reloadWhiteboard = useCallback(async (id: number | string) => {
    try {
      const r = await appAgentsApi<{ items?: WhiteboardNote[]; notes?: WhiteboardNote[] }>(
        `/api/app-agents/threads/${encodeURIComponent(String(id))}/whiteboard`,
      )
      setWhiteboard(r.items ?? r.notes ?? [])
    } catch {
      /* ignore */
    }
  }, [])

  useEffect(() => {
    if (streamState.completedMessages.length === 0) return
    setMessages((prev) => {
      const existingIds = new Set(prev.map((m) => String(m.id)))
      const additions = streamState.completedMessages.filter((m) => !existingIds.has(String(m.id)))
      return additions.length > 0 ? [...prev, ...additions] : prev
    })
  }, [streamState.completedMessages])

  useEffect(() => {
    abortRef.current?.abort()
    abortRef.current = null
    setStreamState(createInitialStreamTurnState())
    setStreamConnected(false)
    setError(null)

    if (threadId === null || threadId === '') {
      setMessages([])
      setWhiteboard([])
      return
    }

    void reloadMessages(threadId)
    void reloadWhiteboard(threadId)

    let cancelled = false
    let retryTimer: number | undefined
    const controller = new AbortController()
    abortRef.current = controller

    const connectStream = async (attempt: number) => {
      if (cancelled) return
      try {
        const res = await fetch(`/api/app-agents/threads/${encodeURIComponent(String(threadId))}/stream`, {
          method: 'GET',
          credentials: 'same-origin',
          headers: { Accept: 'text/event-stream' },
          signal: controller.signal,
        })
        if (!res.ok || !res.body) {
          setStreamConnected(false)
          return
        }
        setStreamConnected(true)
        const reader = res.body.getReader()
        const decoder = new TextDecoder()
        let buffer = ''

        while (!cancelled) {
          const { value, done } = await reader.read()
          if (done) break
          const chunk = decoder.decode(value, { stream: true })
          const parsed = parseSseStreamChunk(buffer, chunk)
          buffer = parsed.remainder
          if (parsed.events.length > 0) {
            setStreamState((prev) => {
              let next = prev
              for (const ev of parsed.events) {
                next = applyStreamEvent(next, ev)
              }
              return next
            })
          }
        }
      } catch {
        if (!cancelled) {
          setStreamConnected(false)
          const delay = Math.min(8000, 1000 * Math.pow(1.6, attempt))
          retryTimer = window.setTimeout(() => void connectStream(attempt + 1), delay)
        }
      }
    }

    void connectStream(0)

    return () => {
      cancelled = true
      if (retryTimer !== undefined) window.clearTimeout(retryTimer)
      controller.abort()
    }
  }, [threadId, reloadMessages, reloadWhiteboard])

  const sendTurn = useCallback(
    async (
      text: string,
      opts?: {
        mentions?: string[]
        activeViewContext?: ActiveViewContext | null
        skillSlugs?: string[]
        modelOverride?: string | null
      },
    ) => {
      const trimmed = text.trim()
      if (!trimmed || threadId === null || sending) return
      setSending(true)
      setError(null)
      setStreamState((prev) => ({ ...prev, toolPills: [], lastError: null }))

      const mentions = opts?.mentions ?? extractMentions(trimmed)
      const boundedCtx = opts?.activeViewContext
        ? boundActiveViewContext(opts.activeViewContext, MAX_ACTIVE_VIEW_CONTEXT_BYTES)
        : undefined

      const idempotencyKey = `turn_${threadId}_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`
      try {
        const res = await appAgentsApi<{
          user_message?: Record<string, unknown>
          messages?: Record<string, unknown>[]
        }>(`/api/app-agents/threads/${encodeURIComponent(String(threadId))}/turns`, {
          method: 'POST',
          body: JSON.stringify({
            text: trimmed,
            idempotency_key: idempotencyKey,
            mentions,
            ...(opts?.skillSlugs && opts.skillSlugs.length > 0 ? { skill_slugs: opts.skillSlugs } : {}),
            ...(opts?.modelOverride && opts.modelOverride.trim()
              ? { model_override: opts.modelOverride.trim() }
              : {}),
            ...(boundedCtx ? { active_view_context: boundedCtx } : {}),
          }),
        })
        if (Array.isArray(res.messages) && res.messages.length > 0) {
          const normalized = res.messages.map(normalizeMessage)
          setMessages((prev) => {
            const seen = new Set(prev.map((m) => String(m.id)))
            const fresh = normalized.filter((m) => !seen.has(String(m.id)))
            return [...prev, ...fresh]
          })
        } else {
          await reloadMessages(threadId)
        }
      } catch (err) {
        const msg = err instanceof Error ? err.message : 'Failed to send turn'
        setError(msg)
        throw err
      } finally {
        setSending(false)
      }
    },
    [reloadMessages, sending, threadId],
  )

  const addWhiteboardNote = useCallback(
    async (content: string, authorSlug?: string) => {
      if (!threadId || !content.trim()) return
      const res = await appAgentsApi<{ item?: WhiteboardNote; note?: WhiteboardNote }>(
        `/api/app-agents/threads/${encodeURIComponent(String(threadId))}/whiteboard`,
        {
          method: 'POST',
          body: JSON.stringify({
            content: content.trim(),
            author_slug: authorSlug ?? 'owner',
            pinned: true,
          }),
        },
      )
      const note = res.item ?? res.note
      if (note) {
        setWhiteboard((prev) => [note, ...prev])
      } else {
        await reloadWhiteboard(threadId)
      }
    },
    [reloadWhiteboard, threadId],
  )

  const deleteWhiteboardNote = useCallback(
    async (noteId: number | string) => {
      if (!threadId) return
      await appAgentsApi(
        `/api/app-agents/threads/${encodeURIComponent(String(threadId))}/whiteboard/${encodeURIComponent(String(noteId))}`,
        { method: 'DELETE' },
      ).catch(() =>
        appAgentsApi(
          `/api/app-agents/threads/${encodeURIComponent(String(threadId))}/whiteboard?id=${encodeURIComponent(String(noteId))}`,
          { method: 'DELETE' },
        ),
      )
      setWhiteboard((prev) => prev.filter((n) => String(n.id) !== String(noteId)))
    },
    [threadId],
  )

  const summarizeRound = useCallback(async () => {
    if (!threadId) return null
    const res = await appAgentsApi<{ item?: WhiteboardNote; note?: WhiteboardNote; summary?: string }>(
      `/api/app-agents/threads/${encodeURIComponent(String(threadId))}/round-summary`,
      {
        method: 'POST',
        body: JSON.stringify({}),
      },
    )
    await reloadWhiteboard(threadId)
    return res.item ?? res.note ?? null
  }, [reloadWhiteboard, threadId])

  return {
    messages,
    whiteboard,
    streamState,
    streamConnected,
    sending,
    error,
    reloadMessages,
    reloadWhiteboard,
    sendTurn,
    addWhiteboardNote,
    deleteWhiteboardNote,
    summarizeRound,
  }
}

export function useCompanionContext(params: {
  kind?: 'article' | 'report'
  articleId?: number | null
  reportId?: number | null
  title?: string | null
  sourceAttribution?: string | null
  selectedText?: string | null
  contentText?: string | null
  excerpt?: string | null
}): ActiveViewContext | null {
  const { kind, articleId, reportId, title, sourceAttribution, selectedText, contentText, excerpt } = params
  return useMemo(() => {
    const validArticle = typeof articleId === 'number' && Number.isFinite(articleId) && articleId > 0
    const validReport = typeof reportId === 'number' && Number.isFinite(reportId) && reportId > 0
    if (!validArticle && !validReport) {
      return null
    }
    return boundActiveViewContext(
      {
        kind: kind ?? (validReport ? 'report' : 'article'),
        ref_id: validArticle ? String(articleId) : `report:${reportId}`,
        article_id: validArticle ? articleId : null,
        report_id: validReport ? reportId : null,
        title: title ?? undefined,
        source_attribution: sourceAttribution ?? null,
        selected_text: selectedText ?? null,
        content_text: contentText ?? undefined,
        excerpt: excerpt ?? undefined,
      },
      MAX_ACTIVE_VIEW_CONTEXT_BYTES,
    )
  }, [kind, articleId, reportId, title, sourceAttribution, selectedText, contentText, excerpt])
}
