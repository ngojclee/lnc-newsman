/**
 * Canonical TypeScript Contract for `lnc-app-agents v2.0` adapted for `lnc-newsman`.
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

export interface AppCapabilitiesManifest {
  app_id: string;
  app_name?: string;
  display_name: string;
  protocol_version: '2.0';
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
}

export interface CatalogModelItem {
  model_id: string;
  display_name: string;
  provider: string;
  provider_id?: string;
  capabilities: string[];
  context_window: number | null;
  enabled: boolean;
  source: string;
  last_fetched_at: string;
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
