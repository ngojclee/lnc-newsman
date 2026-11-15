/**
 * Canonical TypeScript Contract for `lnc-app-agents v2.2` adapted for `lnc-newsman`.
 * Preserves all Newsman domain shapes while exporting the full `v2.1` + `v2.2` kit contract.
 */

export interface AppFeatureFlags {
  voice_tts: boolean;
  voice_stt: boolean;
  image_gen: boolean;
  co_writer: boolean;
  group_rooms: boolean;
  telegram: boolean;
  memory_graph: boolean;
  thread_branching?: boolean;
  ask_user?: boolean;
  custom_providers?: boolean;
  model_catalog?: boolean;
  mcp_namespaces?: boolean;
}

export interface ManifestQuickAction {
  id: string;
  label?: string;
  label_en?: string;
  label_vi?: string;
  prompt?: string;
  prompt_template?: string;
  target_surface?: string;
  chat_mode?: string;
}

export type CapabilityTriState = 'auto' | 'enabled' | 'disabled';

export interface SkillOptionV2 {
  slug: string;
  name: string;
  description: string;
  source: 'app' | 'hub' | 'custom';
  pinned?: boolean;
  tags?: string[];
  slash_command?: string;
  /** Optional markdown preview rendered inside the skill picker drawer. */
  skill_md?: string;
}

export interface CustomSkillItemV2 {
  slug: string;
  name: string;
  description: string;
  skill_md: string;
  tags: string[];
  slash_command: string;
  pinned: boolean;
  source: 'custom';
  updated_at: string;
}

export type WorkspaceTabKindV2 =
  | 'activity'
  | 'chat'
  | 'notes'
  | 'preview'
  | 'artifact'
  | 'memory'
  | 'custom';

export interface AppCapabilitiesManifest {
  app_id: string;
  app_name?: string;
  display_name: string;
  protocol_version: '2.2' | '2.1' | '2.0';
  app_version?: string;
  ui_defaults: {
    interface_language: 'en' | 'vi';
    model_output_language: 'vi' | 'en' | 'auto' | 'follow_content';
    send_key_mode: 'enter' | 'ctrl_enter';
  };
  capabilities: AppFeatureFlags;
  features?: AppFeatureFlags;
  surfaces?: string[];
  scope_kinds: string[];
  quick_actions: ManifestQuickAction[];
  workspace_tabs?: WorkspaceTabKindV2[];
  skills?: SkillOptionV2[];
}

export interface AppCapabilitiesManifestV21
  extends Omit<AppCapabilitiesManifest, 'protocol_version'> {
  protocol_version: '2.2' | '2.1';
  skills?: SkillOptionV2[];
  workspace_tabs?: WorkspaceTabKindV2[];
}

export interface CatalogModelItem {
  model_id: string;
  display_name: string;
  provider: string;
  provider_id?: string;
  capabilities: string[];
  context_window: number | null;
  detected_context_window?: number | null;
  capability_overrides_json?: string;
  capability_overrides?: Record<string, CapabilityTriState>;
  last_test_fingerprint?: string;
  enabled: boolean;
  source: string;
  last_fetched_at: string;
  supports_tools?: boolean;
  supports_vision?: boolean;
  supports_reasoning?: boolean;
  is_fast_tier?: boolean;
  default_effort?: 'fast' | 'low' | 'medium' | 'high';
  last_latency_ms?: number | null;
}

export interface ModelRolesConfig {
  default_smart_model: string;
  fast_background_model: string;
  resolved_fast_model?: string;
}

export interface BusinessMcpNamespaceItem {
  namespace: string;
  display_name: string;
  description: string;
  enabled: boolean;
  tool_count: number;
  source: string;
  updated_at?: string;
}

export interface CustomProviderItem {
  id: string;
  name: string;
  base_url: string;
  capabilities: string[];
  enabled: boolean;
  has_secret: boolean;
  created_at: string;
  updated_at: string;
}

export interface ExecutionTraceStep {
  step?: number;
  kind?: 'thinking' | 'tool';
  merge_id?: string;
  tool: string;
  origin?: 'builtin' | 'app_domain' | 'hub_mcp' | 'app_v4s' | 'hub_business_mcp';
  status?: string;
  summary?: string;
  duration_ms?: number;
  is_error?: boolean;
}

export interface AskUserPrompt {
  prompt_id: string;
  thread_id: string;
  agent_slug: string;
  question: string;
  options: string[];
  allow_custom: boolean;
  status: 'pending' | 'answered';
  answer?: string;
  created_at?: string;
  answered_at?: string;
}

export interface CoWriterDraftRow {
  id: string;
  thread_id?: string | null;
  article_id?: number | null;
  report_id?: number | null;
  target_kind: string;
  title: string;
  before_text: string;
  after_text: string;
  rationale: string;
  agent_slug: string;
  status: 'pending' | 'accepted' | 'rejected';
  created_at: string;
  decided_at: string;
}

export interface MemoryGraphNode {
  id: string;
  layer: 'L1' | 'L2' | 'L3';
  node_kind: string;
  title: string;
  content: string;
  agent_slug: string;
  thread_id?: string | null;
  scope?: Record<string, unknown>;
  confidence: number;
  source_ref: string;
  created_at: string;
  updated_at: string;
}

export interface MemoryGraphEdge {
  id: string;
  from_node_id: string;
  to_node_id: string;
  relation: string;
  created_at: string;
}

export interface MemoryGraphPayload {
  nodes: MemoryNodeOrRow[];
  edges: MemoryGraphEdge[];
  l1_scratchpad?: string | null;
  stats: {
    total_nodes: number;
    total_edges: number;
    l1_count: number;
    l2_count: number;
    l3_count: number;
  };
}

type MemoryNodeOrRow = MemoryGraphNode;

// ============================================================================
// lnc-app-agents v2.1 / v2.2 Additions (Dock, Skill Picker, Activity, Budget,
// Queued Messages, Turn Navigator, Selection Toolbar & Bulk Models)
// ============================================================================

/**
 * Accent-insensitive Vietnamese + Latin text folder for client-side pickers
 * and search boxes (maps `đ`/`Đ` -> `d` in addition to Unicode NFD stripping).
 */
export function foldVi(input: string): string {
  return (input || '')
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
    .replace(/[đĐ]/g, 'd')
    .toLowerCase()
    .trim();
}

export interface SessionToolUsageV2 {
  name: string;
  count: number;
}

export type SessionToolCountV2 = SessionToolUsageV2;

export interface SessionSpaceItemV2 {
  id: string;
  title: string;
  subtitle?: string;
}

export interface SessionSpaceSubsectionV2 {
  key: 'persona' | 'regions' | 'topics' | 'books' | 'notebooks' | 'chat_history' | 'question_bank' | 'memory' | string;
  label: string;
  href?: string;
  items: SessionSpaceItemV2[];
}

export interface SessionArtifactFileV2 {
  id: string;
  filename: string;
  ext?: string;
  sizeBytes?: number;
  mimeType?: string;
  uri?: string;
  messageIndex?: number;
  previewText?: string;
}

export interface SessionActivityV2 {
  tools: SessionToolUsageV2[];
  knowledgeBases: string[];
  space: SessionSpaceSubsectionV2[];
  generatedFiles: SessionArtifactFileV2[];
  attachments: SessionArtifactFileV2[];
}

export interface QueuedMessageV2 {
  id: string;
  content: string;
  steerable?: boolean;
}

export interface TurnOutlineEntryV2 {
  key: string;
  prompt: string;
  mode?: string;
  weight?: number;
}

export interface ContextBudgetSegmentV2 {
  key: string;
  label?: string;
  tokens: number;
}

export interface ContextBudgetV2 {
  window: number;
  window_estimated?: boolean;
  used_tokens: number;
  free_tokens: number;
  model?: string;
  deferred_tool_count?: number;
  segments: ContextBudgetSegmentV2[];
}

export interface WorkspaceTabV2 {
  id: string;
  kind: WorkspaceTabKindV2;
  title: string;
  targetId?: string;
  closable?: boolean;
  pinned?: boolean;
  /** Set when a tab is rebound onto a new thread key after thread switch. */
  reboundFromKey?: string;
  meta?: Record<string, unknown>;
}

export interface SelectionAnchorV2 {
  id: string;
  quote: string;
  sourceTitle?: string;
  sourceUri?: string;
  pageOrChapter?: string;
  offsetStart?: number;
  offsetEnd?: number;
}

export interface SideChatResponderV2 {
  slug: string;
  displayName: string;
  emoji?: string;
  accent?: string;
  roleHint?: string;
}

export interface ModelTestFingerprintV2 {
  provider_id: string;
  model_id: string;
  sha256: string;
  tested_at: number;
  ok: boolean;
  latency_ms?: number | null;
  detected_context_window?: number | null;
}

export interface BulkModelImportItemV2 {
  model_id: string;
  display_name?: string;
  context_window?: number;
  detected_context_window?: number | null;
  supports_tools?: boolean;
  supports_vision?: boolean;
  supports_reasoning?: boolean;
  is_fast_tier?: boolean;
  default_effort?: 'fast' | 'low' | 'medium' | 'high';
  enabled?: boolean;
  capability_overrides?: Record<string, CapabilityTriState>;
  capability_overrides_json?: string;
  last_test_fingerprint?: string;
}

export interface BulkModelImportRequestV2 {
  provider_id: string;
  models: BulkModelImportItemV2[];
  preserve_existing_enabled?: boolean;
}

export function estimateApproxTokens(text: string): number {
  const trimmed = (text || '').trim();
  if (!trimmed) return 0;
  return Math.max(1, Math.ceil(trimmed.length / 3.8));
}

export interface BuildContextBudgetInputV2 {
  window?: number | null;
  model?: string;
  systemText?: string;
  appToolCount?: number;
  mcpToolCount?: number;
  skillsText?: string;
  memoryText?: string;
  sourcesText?: string;
  conversationText?: string;
  composerDraft?: string;
}

export function buildContextBudgetV2(input: BuildContextBudgetInputV2): ContextBudgetV2 {
  const windowSize =
    input.window && Number.isFinite(input.window) && input.window > 0
      ? Math.round(input.window)
      : 128_000;
  const windowEstimated = !(input.window && Number.isFinite(input.window) && input.window > 0);
  const rawSegments: ContextBudgetSegmentV2[] = [
    {
      key: 'system_prompt',
      label: 'System',
      tokens: Math.max(240, estimateApproxTokens(input.systemText || '')),
    },
    {
      key: 'system_tools',
      label: 'App Tools',
      tokens: Math.max(0, Math.round((input.appToolCount ?? 0) * 95)),
    },
    {
      key: 'mcp_tools',
      label: 'MCP Tools',
      tokens: Math.max(0, Math.round((input.mcpToolCount ?? 0) * 110)),
    },
    {
      key: 'skills',
      label: 'Skills',
      tokens: estimateApproxTokens(input.skillsText || ''),
    },
    {
      key: 'memory',
      label: 'Memory',
      tokens: estimateApproxTokens(input.memoryText || ''),
    },
    {
      key: 'sources',
      label: 'Sources',
      tokens: estimateApproxTokens(input.sourcesText || ''),
    },
    {
      key: 'messages',
      label: 'Conversation',
      tokens:
        estimateApproxTokens(input.conversationText || '') +
        estimateApproxTokens(input.composerDraft || ''),
    },
  ].filter((s) => s.tokens > 0);

  const usedTokens = rawSegments.reduce((acc, s) => acc + s.tokens, 0);
  const freeTokens = Math.max(0, windowSize - usedTokens);
  return {
    window: windowSize,
    window_estimated: windowEstimated,
    used_tokens: usedTokens,
    free_tokens: freeTokens,
    model: input.model,
    segments: rawSegments,
  };
}

export interface BuildSessionActivityInputV2 {
  toolNames: string[];
  knowledgeBases: string[];
  personas?: Array<{ slug: string; name: string; role?: string }>;
  regions?: string[];
  topics?: string[];
  memoryItems?: Array<{ id: string; title: string; layer?: string }>;
  generatedFiles?: SessionArtifactFileV2[];
  attachments?: SessionArtifactFileV2[];
}

export function buildSessionActivityV2(input: BuildSessionActivityInputV2): SessionActivityV2 {
  const counts = new Map<string, number>();
  for (const raw of input.toolNames) {
    const name = (raw || '').trim();
    if (!name) continue;
    counts.set(name, (counts.get(name) ?? 0) + 1);
  }
  const tools: SessionToolUsageV2[] = Array.from(counts.entries())
    .map(([name, count]) => ({ name, count }))
    .sort((a, b) => b.count - a.count || a.name.localeCompare(b.name));

  const space: SessionSpaceSubsectionV2[] = [];
  if (input.personas && input.personas.length > 0) {
    space.push({
      key: 'persona',
      label: 'Desk Personas',
      href: '#studio',
      items: input.personas.map((p) => ({
        id: p.slug,
        title: p.name,
        subtitle: p.role,
      })),
    });
  }
  const regionTopicItems: SessionSpaceItemV2[] = [
    ...(input.regions ?? []).map((r) => ({ id: `region:${r}`, title: r, subtitle: 'region' })),
    ...(input.topics ?? []).map((tp) => ({ id: `topic:${tp}`, title: tp, subtitle: 'topic' })),
  ];
  if (regionTopicItems.length > 0) {
    space.push({
      key: 'regions',
      label: 'Regions & Topics',
      href: '#regions',
      items: regionTopicItems,
    });
  }
  if (input.memoryItems && input.memoryItems.length > 0) {
    space.push({
      key: 'memory',
      label: 'Memory Nodes',
      href: '#memory',
      items: input.memoryItems.map((m) => ({
        id: m.id,
        title: m.title,
        subtitle: m.layer,
      })),
    });
  }

  return {
    tools,
    knowledgeBases: Array.from(new Set(input.knowledgeBases.filter(Boolean))),
    space,
    generatedFiles: input.generatedFiles ?? [],
    attachments: input.attachments ?? [],
  };
}
