import {
  AGENT_COPY,
  DEFAULT_SEED_AGENTS,
  formatBridgeModeLabel,
  formatDiscussionModeShort,
  NEWSMAN_APP_TOOLS,
  type HubSkillGroupSummary,
  type HubSkillSummary,
  type SkillAccessRequest,
} from './agentTypes.ts'
import {
  applyStreamEvent,
  boundActiveViewContext,
  buildAppAgentsRequestInit,
  buildSeatTraceId,
  computeEffectiveScope,
  createInitialStreamTurnState,
  emptyHubEffectiveBundle,
  extractMentions,
  intersectScopeLists,
  MAX_ACTIVE_VIEW_CONTEXT_BYTES,
  normalizeBridgeRouting,
  normalizeHubEffectiveBundle,
  parseComposerTrigger,
  parseSseFrame,
  parseSseStreamChunk,
  skillDisplayName,
  skillGroupDisplayName,
  stripSkillFrontmatter,
} from './useAppAgents.ts'

function assertOk(condition: unknown, message = 'Assertion failed'): asserts condition {
  if (!condition) {
    throw new Error(message)
  }
}

function assertEqual<T>(actual: T, expected: T, message = ''): void {
  if (actual !== expected) {
    throw new Error(
      `${message ? `${message}: ` : ''}Expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`,
    )
  }
}

function assertDeepEqual<T>(actual: T, expected: T, message = ''): void {
  const a = JSON.stringify(actual)
  const b = JSON.stringify(expected)
  if (a !== b) {
    throw new Error(`${message ? `${message}: ` : ''}Expected ${b}, got ${a}`)
  }
}

// 1. SSE 7-event frame parsing & merge_id tool + tool_result collapsing with Newsroom v4s scope
{
  const rawSse = [
    'event: seat_state\ndata: {"agent_slug":"macro-argus","state":"thinking"}',
    'event: reasoning\ndata: {"agent_slug":"macro-argus","delta":"Checking SBV macro series and tax calendar..."}',
    'event: tool\ndata: {"agent_slug":"macro-argus","merge_id":"call_news_1","tool":"tax_calendar","arguments":{"region":"vn","days":30},"resource_ctx":{"regions":["vn"],"topics":["tax"]},"approval_required":null}',
    'event: tool_result\ndata: {"agent_slug":"macro-argus","merge_id":"call_news_1","tool":"tax_calendar","result":"VAT Q2 declaration due (verified=false, disclaimer attached)","approval_required":null}',
    'event: text\ndata: {"agent_slug":"macro-argus","delta":"Theo lịch thuế và #article-108, tờ khai quý cần đối chiếu thêm."}',
    'event: result\ndata: {"thread_id":12,"agent_slug":"macro-argus","round_idx":1}',
    'event: error\ndata: {"agent_slug":"macro-argus","error":"Hub fallback warning"}',
    '',
  ].join('\n\n')

  const { events, remainder } = parseSseStreamChunk('', rawSse)
  assertEqual(remainder, '')
  assertEqual(events.length, 7, 'Must parse all 7 Contract C SSE event kinds')
  assertDeepEqual(
    events.map((e) => e.kind),
    ['seat_state', 'reasoning', 'tool', 'tool_result', 'text', 'result', 'error'],
  )

  let state = createInitialStreamTurnState()

  for (const ev of events.slice(0, 6)) {
    state = applyStreamEvent(state, ev)
  }

  assertEqual(state.seatStates['macro-argus'], 'done')
  assertEqual(state.completedMessages.length, 1, 'macro-argus completed message must be materialized')

  const completed = state.completedMessages[0]
  assertEqual(completed.sender_slug, 'macro-argus')
  assertEqual(completed.content, 'Theo lịch thuế và #article-108, tờ khai quý cần đối chiếu thêm.')
  assertEqual(completed.reasoning, 'Checking SBV macro series and tax calendar...')
  const toolCalls = completed.tool_calls ?? []
  assertEqual(toolCalls.length, 1, 'Matching merge_id must collapse tool + tool_result into 1 pill')
  assertEqual(toolCalls[0].merge_id, 'call_news_1')
  assertEqual(toolCalls[0].status, 'done')
  assertDeepEqual(toolCalls[0].resource_ctx?.regions, ['vn'])
  assertDeepEqual(toolCalls[0].resource_ctx?.topics, ['tax'])

  state = applyStreamEvent(state, events[6])
  assertEqual(state.lastError, 'Hub fallback warning')
}

// 2. Gated tool approval_required badge preservation on tool_result
{
  const toolFrame = parseSseFrame(
    'event: tool\ndata: {"agent_slug":"editor-minh","merge_id":"call_brief_9","tool":"brief_propose","arguments":{"period":"morning","title":"Bản tin sáng"}}',
  )
  const resultFrame = parseSseFrame(
    'event: tool_result\ndata: {"agent_slug":"editor-minh","merge_id":"call_brief_9","tool":"brief_propose","result":"Draft brief #19 queued for editorial review","approval_required":{"action_type":"newsman.brief.publish","summary":"Publishing morning brief requires human editor confirmation","request_ref":"19"}}',
  )
  assertOk(toolFrame !== null && resultFrame !== null)

  let state = createInitialStreamTurnState()
  state = applyStreamEvent(state, toolFrame)
  state = applyStreamEvent(state, resultFrame)

  assertEqual(state.toolPills.length, 1)
  const pill = state.toolPills[0]
  assertEqual(pill.status, 'approval_required')
  assertEqual(pill.approval_required?.action_type, 'newsman.brief.publish')
}

// 3. Zero-RTT active_view_context <= 16 KiB UTF-8 bounding helper (Article & Report views)
{
  const hugeParagraph = 'Ngân hàng Nhà nước điều hành tỷ giá và lợi suất trái phiếu Chính phủ. '.repeat(600)
  const rawCtx = {
    kind: 'article' as const,
    ref_id: '108',
    article_id: 108,
    title: 'Báo cáo vĩ mô và lịch thuế doanh nghiệp Q2',
    source_attribution: 'Thời báo Tài chính Việt Nam',
    selected_text: hugeParagraph,
    content_text: hugeParagraph,
    excerpt: hugeParagraph,
  }

  const bounded = boundActiveViewContext(rawCtx)
  assertOk(bounded)
  const serialized = JSON.stringify(bounded)
  const byteLen = new TextEncoder().encode(serialized).byteLength
  assertOk(
    byteLen <= MAX_ACTIVE_VIEW_CONTEXT_BYTES,
    `Serialized active_view_context (${byteLen} bytes) must not exceed ${MAX_ACTIVE_VIEW_CONTEXT_BYTES} bytes`,
  )
  assertEqual(bounded.kind, 'article')
  assertEqual(bounded.ref_id, '108')
  assertEqual(bounded.article_id, 108)
  assertEqual(bounded.source_attribution, 'Thời báo Tài chính Việt Nam')
}

// 4. CSRF X-Requested-With: XMLHttpRequest enforcement on all mutations
{
  for (const method of ['POST', 'PUT', 'PATCH', 'DELETE']) {
    const mutInit = buildAppAgentsRequestInit({ method, body: JSON.stringify({ hello: 'newsroom' }) })
    const headers = new Headers(mutInit.headers)
    assertEqual(
      headers.get('X-Requested-With'),
      'XMLHttpRequest',
      `${method} mutation must attach X-Requested-With: XMLHttpRequest`,
    )
    assertEqual(headers.get('Content-Type'), 'application/json')
    assertEqual(mutInit.credentials, 'same-origin')
  }
}

// 5. Bilingual VI / EN dictionary symmetry check (AGENT_COPY + main.tsx copy blocks)
{
  const viKeys = Object.keys(AGENT_COPY.vi).sort()
  const enKeys = Object.keys(AGENT_COPY.en).sort()
  assertDeepEqual(viKeys, enKeys, 'AGENT_COPY.vi and AGENT_COPY.en must have identical keys')

  for (const key of viKeys) {
    const k = key as keyof typeof AGENT_COPY.vi
    assertOk(AGENT_COPY.vi[k].trim().length > 0, `AGENT_COPY.vi.${key} must not be empty`)
    assertOk(AGENT_COPY.en[k].trim().length > 0, `AGENT_COPY.en.${key} must not be empty`)
  }

  const fsMod = (await import(/* @vite-ignore */ 'node:' + 'fs')) as {
    readFileSync: (p: string, enc: string) => string
  }
  const urlMod = (await import(/* @vite-ignore */ 'node:' + 'url')) as {
    fileURLToPath: (u: string) => string
  }
  const pathMod = (await import(/* @vite-ignore */ 'node:' + 'path')) as {
    dirname: (p: string) => string
    resolve: (...p: string[]) => string
  }

  const here = pathMod.dirname(urlMod.fileURLToPath(import.meta.url))
  const mainTsx = fsMod.readFileSync(pathMod.resolve(here, 'main.tsx'), 'utf8')
  assertOk(
    mainTsx.includes('...AGENT_COPY.vi') && mainTsx.includes('...AGENT_COPY.en'),
    'main.tsx must spread both AGENT_COPY.vi and AGENT_COPY.en into copy',
  )

  const viBlockMatch = mainTsx.match(/vi:\s*\{([\s\S]*?)\n\s*\},\s*\n\s*en:\s*\{([\s\S]*?)\n\s*\},\s*\n\s*\}/)
  assertOk(viBlockMatch !== null, 'Could not locate copy.vi and copy.en blocks in main.tsx')
  const extractIdentifierKeys = (block: string) => {
    const cleaned = block
      .replace(/\/\/.*$/gm, '')
      .replace(/'(?:\\.|[^'\\])*'/g, "''")
      .replace(/`(?:\\.|[^`\\])*`/g, "''")
      .replace(/\([^)]*\)\s*=>/g, 'fn')
    return Array.from(cleaned.matchAll(/([a-zA-Z0-9_]+)\s*:/g))
      .map((m) => m[1])
      .sort()
  }
  const mainViKeys = extractIdentifierKeys(viBlockMatch[1])
  const mainEnKeys = extractIdentifierKeys(viBlockMatch[2])
  assertDeepEqual(mainViKeys, mainEnKeys, 'main.tsx copy.vi and copy.en must have identical keys')
}

// 6. @slug mention extractor & Newsroom app tool catalog check
{
  const slugs = extractMentions(
    'Nhờ @macro-argus kiểm tra chuỗi lãi suất và @tax-linh rà lại tờ khai cùng @editor-minh!',
    ['macro-argus', 'tax-linh', 'editor-minh'],
  )
  assertDeepEqual(slugs, ['macro-argus', 'tax-linh', 'editor-minh'])

  const toolNames = NEWSMAN_APP_TOOLS.map((t) => t.name)
  assertDeepEqual(toolNames, [
    'latest_articles',
    'article_get',
    'documents',
    'tax_calendar',
    'series',
    'observations',
    'analyses',
    'notes',
    'brief_propose',
  ])
  const briefTool = NEWSMAN_APP_TOOLS.find((t) => t.name === 'brief_propose')
  assertEqual(briefTool?.tier, 'write_gated')
}

// 7. Adaptive UI pure helpers: stripSkillFrontmatter, buildSeatTraceId, parseComposerTrigger
{
  const withYaml = [
    '---',
    'name: statutory-tax-citation-guard',
    'category: compliance',
    '---',
    '',
    '# Statutory Tax Guard',
    'Always preserve verified flags.',
  ].join('\n')
  assertEqual(
    stripSkillFrontmatter(withYaml),
    '# Statutory Tax Guard\nAlways preserve verified flags.',
  )
  assertEqual(stripSkillFrontmatter('# Plain Playbook'), '# Plain Playbook')

  assertEqual(
    buildSeatTraceId({ id: 42, sender_slug: 'tax-linh', round_idx: 2 }),
    'trace-42-tax-linh-2',
  )

  const atTrigger = parseComposerTrigger('Hỏi @tax', 8)
  assertDeepEqual(atTrigger, { trigger: '@', query: 'tax', start: 4, end: 8 })

  const slashTrigger = parseComposerTrigger('Phân tích /macro-cross', 22)
  assertDeepEqual(slashTrigger, { trigger: '/', query: 'macro-cross', start: 10, end: 22 })

  assertEqual(parseComposerTrigger('email@domain.com', 16), null)
}

// 8. Hub 0026 contracts & Salon/Skills helpers: display_name, body_preview, SkillGroupSummary, approval metadata, Workspace ∩ Agent scope
{
  const skillItem: HubSkillSummary = {
    slug: 'locked-alpha',
    name: 'locked-alpha',
    display_name: 'Alpha Khóa',
    description: 'Gated alpha skill',
    category: 'markets',
    group_slugs: ['market-intel'],
    body_md: '',
    body_preview: 'Safe intro summary before granting.',
    granted: false,
    enabled: true,
    version: '1.0.0',
    source: 'hub',
  }
  assertEqual(skillDisplayName(skillItem), 'Alpha Khóa')
  assertEqual(
    skillDisplayName({ slug: 'plain-skill', name: 'Plain Skill' }),
    'Plain Skill',
  )
  assertEqual(skillDisplayName({ slug: 'slug-only', name: '' }), 'slug-only')

  const groupItem: HubSkillGroupSummary = {
    slug: 'market-intel',
    name: 'Market Intel',
    display_name: 'Gói Thị trường & Vĩ mô',
    description: 'Phân tích chỉ báo vĩ mô & hàng hóa.',
    category: 'markets',
    icon: '📈',
    skill_slugs: ['macro-signal', 'locked-alpha'],
    granted_count: 1,
    total_count: 2,
    granted: false,
  }
  assertEqual(skillGroupDisplayName(groupItem), 'Gói Thị trường & Vĩ mô')

  const accessItem: SkillAccessRequest = {
    id: 'req_1',
    target_kind: 'skill_group',
    target_slug: 'market-intel',
    reason: 'Need macro pack',
    status: 'approved',
    hub_request_id: 'hub-req-1',
    approval_request_id: 'appr-777',
    reviewed_by: 'owner@lnc',
    review_note: 'Approved for editorial desk',
    created_at: '2026-11-13T10:00:00Z',
    updated_at: '2026-11-13T11:00:00Z',
  }
  assertEqual(accessItem.approval_request_id, 'appr-777')
  assertEqual(accessItem.reviewed_by, 'owner@lnc')

  assertDeepEqual(intersectScopeLists(['vn', 'asean'], ['vn']), ['vn'])
  assertDeepEqual(intersectScopeLists(['vn'], ['us']), [])
  assertEqual(intersectScopeLists(undefined, undefined), undefined)

  const wsScope = computeEffectiveScope(
    {
      id: 'vn-macro-tax-desk',
      name: 'Bàn Vĩ mô & Thuế VN',
      description: 'VN macro & tax workspace',
      emoji: '🇻🇳',
      custom_instructions_md: '',
      resource_ctx: { regions: ['vn'], topics: ['tax', 'finance'] },
      default_skill_slugs: [],
      default_agent_slugs: ['tax-linh'],
      default_discussion_mode: 'sequential_build',
      is_default: false,
      updated_at: '',
    },
    [
      {
        ...DEFAULT_SEED_AGENTS[1],
        default_resource_ctx: { regions: ['vn', 'vn-hcmc'], topics: ['tax'] },
      },
    ],
  )
  assertDeepEqual(wsScope.regions, ['vn'])
  assertDeepEqual(wsScope.topics, ['tax'])
  assertEqual(wsScope.disjointRegions, false)
  assertEqual(wsScope.disjointTopics, false)

  const disjointScope = computeEffectiveScope(
    {
      id: 'vn-only',
      name: 'VN Only',
      description: 'VN only workspace',
      emoji: '🇻🇳',
      custom_instructions_md: '',
      resource_ctx: { regions: ['vn'] },
      default_skill_slugs: [],
      default_agent_slugs: [],
      default_discussion_mode: 'parallel_panel',
      is_default: false,
      updated_at: '',
    },
    [
      {
        ...DEFAULT_SEED_AGENTS[0],
        default_resource_ctx: { regions: ['us'] },
      },
    ],
  )
  assertDeepEqual(disjointScope.regions, [])
  assertEqual(disjointScope.disjointRegions, true)
}

// 9. Zero raw <select> elements across ui/src/**/*.tsx + localized short badges
{
  const fsMod = (await import(/* @vite-ignore */ 'node:' + 'fs')) as {
    readdirSync: (
      p: string,
      opts: { withFileTypes: true },
    ) => Array<{ name: string; isDirectory: () => boolean; isFile: () => boolean }>
    readFileSync: (p: string, enc: string) => string
  }
  const urlMod = (await import(/* @vite-ignore */ 'node:' + 'url')) as {
    fileURLToPath: (u: string) => string
  }
  const pathMod = (await import(/* @vite-ignore */ 'node:' + 'path')) as {
    dirname: (p: string) => string
    join: (...p: string[]) => string
    relative: (from: string, to: string) => string
  }

  const srcRoot = pathMod.dirname(urlMod.fileURLToPath(import.meta.url))
  const collectTsxFiles = (dir: string): string[] => {
    const out: string[] = []
    for (const entry of fsMod.readdirSync(dir, { withFileTypes: true })) {
      const fullPath = pathMod.join(dir, entry.name)
      if (entry.isDirectory()) {
        out.push(...collectTsxFiles(fullPath))
      } else if (entry.isFile() && entry.name.endsWith('.tsx')) {
        out.push(fullPath)
      }
    }
    return out
  }

  const tsxFiles = collectTsxFiles(srcRoot)
  assertOk(tsxFiles.length >= 6, 'Must scan all UI .tsx components under ui/src')
  const offenders: string[] = []
  for (const file of tsxFiles) {
    const content = fsMod.readFileSync(file, 'utf8')
    if (/<select\b/.test(content)) {
      offenders.push(pathMod.relative(srcRoot, file))
    }
  }
  assertDeepEqual(
    offenders,
    [],
    'Zero raw <select> elements may remain in ui/src/**/*.tsx (all must use custom Select)',
  )

  assertEqual(formatDiscussionModeShort('parallel_panel', AGENT_COPY.vi), 'Song song')
  assertEqual(formatDiscussionModeShort('parallel_panel', AGENT_COPY.en), 'Parallel')
  assertEqual(formatDiscussionModeShort('cross_debate', AGENT_COPY.vi), 'Tranh biện')
  assertEqual(formatDiscussionModeShort('cross_debate', AGENT_COPY.en), 'Debate')
  assertEqual(
    formatBridgeModeLabel({ mode: 'astraea_relay', relay_connected: true }, AGENT_COPY.en),
    AGENT_COPY.en.bridgeRelayOnline,
  )
  assertEqual(
    formatBridgeModeLabel({ mode: 'hub_standby', relay_connected: false }, AGENT_COPY.vi),
    AGENT_COPY.vi.bridgeHubStandby,
  )
}

// 10. Hub-First effective-bundle & 5-leg Star Topology routing normalizers
{
  const blankBundle = emptyHubEffectiveBundle()
  assertEqual(blankBundle.astraea_bridge_ready, false)
  assertDeepEqual(blankBundle.capability_bundles, [])
  assertDeepEqual(blankBundle.mcp_groups, [])

  const normBundle = normalizeHubEffectiveBundle({
    astraea_bridge_ready: true,
    ws_relay_path: '/v1/app-connectors/lnc-newsman/ws',
    capability_bundles: [
      {
        slug: 'bundle-lnc-newsman',
        name: 'Newsman Bundle',
        skill_group_slugs: ['vn-macro-tax'],
        mcp_group_slugs: ['newsman-editorial'],
        allowed_model_aliases: ['gpt-5-hub'],
        status: 'granted',
      },
    ],
    mcp_groups: [
      {
        slug: 'newsman-editorial',
        name: 'Newsman Editorial',
        patterns: ['newsman__*', 'obsidian__agent_*'],
        status: 'active',
      },
    ],
    allowed_mcp_tools: ['newsman__*', 'obsidian__agent_*'],
    allowed_model_aliases: ['gpt-5-hub'],
    channel_grants: ['telegram:cygnus'],
  })
  assertEqual(normBundle.astraea_bridge_ready, true)
  assertEqual(normBundle.capability_bundles[0].granted, true)
  assertEqual(normBundle.mcp_groups[0].granted, true)
  assertEqual(normBundle.mcp_groups[0].item_count, 2)
  assertDeepEqual(normBundle.allowed_mcp_tools, ['newsman__*', 'obsidian__agent_*'])
  assertDeepEqual(normBundle.channel_grants, ['telegram:cygnus'])

  const routing = normalizeBridgeRouting({
    astraea_http: 'hub',
    astraea_ws: 'hub',
    models: 'hub',
    skills_and_mcp: 'hub',
    telegram: 'hub_cygnus',
  })
  assertEqual(routing.astraea_http, 'hub')
  assertEqual(routing.astraea_ws, 'hub')
  assertEqual(routing.models, 'hub')
  assertEqual(routing.skills_and_mcp, 'hub')
  assertEqual(routing.telegram, 'hub_cygnus')
}

console.log('10/10 agentBridge tests passed')
