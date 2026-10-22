export type Profile = {
  id: string;
  provider:
    | "custom"
    | "deepseek"
    | "minimax"
    | "minimax_china"
    | "openai"
    | "gemini"
    | "anthropic"
    | "qwen"
    | "zhipu"
    | "siliconflow"
    | "openrouter"
    | "system_dictionary"
    | "youdao";
  name: string;
  endpoint: string;
  endpoint_mode: "base" | "exact";
  model: string;
  app_id?: string;
  custom_prompt?: string;
  custom_reply_prompt?: string;
  streaming: boolean;
  temperature: number;
  revision: number;
};
export type Settings = {
  profiles: Profile[];
  active_profile: string;
  source_language: string;
  target_language: string;
  ui_language: "follow_primary" | "zh" | "en";
  theme: "system" | "light" | "dark";
  ui_font_size: number;
  skip_same_language: boolean;
  sensitive_guard: boolean;
  auto_vocabulary: boolean;
  append_selection_shortcut?: string | null;
  append_screenshot_shortcut?: string | null;
  auto_clean?: boolean;
  translation_prompt?: string;
  reply_prompt?: string;
  preserve_structure?: boolean;
  encounter_highlight?: boolean;
  failover?: { enabled: boolean; profile_ids: string[] };
  retention: { mode: "disabled" | "days" | "forever"; days?: number };
  selection_shortcut: string;
  screenshot_shortcut: string;
  auto_hide: boolean;
  font_size: number;
  max_concurrency: number;
};
export type Configuration = {
  settings: Settings;
  has_keys: string[];
  credential_errors: Record<string, string>;
};
export type FailureSummary = {
  class:
    | "cancelled"
    | "configuration"
    | "connection"
    | "timeout"
    | "rate_limit"
    | "server"
    | "quota"
    | "authentication"
    | "protocol";
  status?: number | null;
  provider_code?: string | null;
  retry_after_seconds?: number | null;
};
export type Task = {
  failure?: FailureSummary | null;
  source_revision?: number;
  can_undo_cleanup?: boolean;
  translation_source_revision?: number | null;
  source_segments?: SourceSegment[];
  extract_vocabulary?: boolean;
  vocabulary_state?:
    "none" | "disabled" | "extracting" | "complete" | "cancelled" | "failed";
  sensitive?: "MNEMONIC" | "PRIVATE_KEY" | "API_SECRET" | null;
  vocabulary?: VocabularyCandidate[];
  vocabulary_notice?: string | null;
  id: string;
  revision: number;
  attempt: number;
  source: string;
  status: string;
  original_text: string;
  edited_text: string | null;
  translation: string;
  profile: Profile;
  target_language: string;
  created_at: number;
  error: string | null;
  save_error: string | null;
  save_state: string;
  has_image: boolean;
  detected_language: string | null;
  source_language?: string | null;
  image_pending: boolean;
  image_error: string | null;
  cache_hit: boolean;
  timings: { first_content_ms: number | null; total_ms: number | null };
  past_attempts: {
    number: number;
    translation: string;
    status: string;
    profile?: Profile;
    reason?: string;
  }[];
};
export type Snapshot = { revision: number; tasks: Task[] };
export type TaskEvent =
  | { type: "changed"; task: Task }
  | {
      type: "delta";
      id: string;
      attempt: number;
      revision: number;
      text: string;
    }
  | { type: "removed"; id: string; revision: number };
export type Wire =
  | {
      type: "snapshot";
      snapshot: Snapshot;
      selected: string | null;
      notice: string | null;
    }
  | { type: "events"; events: TaskEvent[] };
export type Bootstrap = {
  configuration: Configuration;
  permissions: { accessibility: boolean; screen: boolean };
  shortcuts: { registered: boolean; error: string | null };
  snapshot: Snapshot;
  selected: string | null;
  notice: string | null;
  pinned: boolean;
  expanded: boolean;
};
export const defaults: Settings = {
  profiles: [
    {
      id: "default",
      provider: "custom",
      name: "自定义模型",
      endpoint: "",
      endpoint_mode: "base",
      model: "",
      streaming: true,
      temperature: 0.2,
      revision: 0,
    },
  ],
  active_profile: "default",
  source_language: "",
  target_language: "zh-Hans",
  ui_language: "follow_primary",
  theme: "system",
  ui_font_size: 14,
  skip_same_language: true,
  sensitive_guard: true,
  auto_vocabulary: true,
  append_selection_shortcut: null,
  append_screenshot_shortcut: null,
  auto_clean: true,
  translation_prompt: "",
  reply_prompt: "",
  preserve_structure: false,
  encounter_highlight: true,
  failover: { enabled: false, profile_ids: [] },
  retention: { mode: "days", days: 7 },
  selection_shortcut: "Alt+D",
  screenshot_shortcut: "Alt+S",
  auto_hide: true,
  font_size: 16,
  max_concurrency: 2,
};
export const statusNames: Record<string, string> = {
  capturing: "正在取词",
  recognizing: "正在识别",
  queued: "等待翻译",
  translating: "正在翻译",
  completed: "完成",
  original_only: "仅显示原文",
  ready: "待翻译",
  failed: "失败",
  stopped: "已停止",
  interrupted: "已中断",
};
export const active = (task: Task) =>
  ["capturing", "recognizing", "queued", "translating"].includes(task.status);

export type VocabularyCandidate = {
  word: string;
  surface: string;
  phonetic: string;
  pos: string;
  dict_meaning: string;
  context_meaning: string;
  context_sentence: string;
  sentence_trans: string;
  difficulty: string;
};
export type VocabularyContext = Pick<
  VocabularyCandidate,
  "surface" | "context_meaning" | "context_sentence" | "sentence_trans"
> & { id: string; created_at: number };
export type VocabularyItem = Pick<
  VocabularyCandidate,
  "word" | "phonetic" | "pos" | "dict_meaning" | "difficulty"
> & {
  id: string;
  note: string;
  status: "learning" | "mastered";
  review_count: number;
  contexts: VocabularyContext[];
  created_at: number;
  updated_at: number;
};

export type TranslationHighlight = { start: number; end: number };
export type VocabularyPreview = {
  candidate: VocabularyCandidate;
  translation_highlights: TranslationHighlight[];
};
export type SpeechVoice = { locale: string; available: boolean };

export type SourceSegment = {
  id: string;
  kind: string;
  original_text: string;
  captured_at: number;
  asset_id?: string | null;
  ocr_layout?: unknown;
};
export type SourceAsset = { id: string; segment_id: string; legacy: boolean };
export type SourceEncounter = {
  start_utf16: number;
  end_utf16: number;
  item_id: string;
  word: string;
  surface: string;
};
export type ServiceVerification = {
  profile_id: string;
  state: "untested" | "verified" | "expired" | "changed" | "failed";
  tested_at: number | null;
  expires_at: number | null;
  tested_capability: "translation_transport";
};
export type OperationRecord = {
  id: string;
  request_id: string;
  task_id: string | null;
  source_revision: number;
  kind: "explain" | "reply";
  status:
    "queued" | "generating" | "completed" | "stopped" | "failed" | "blocked";
  primary_text: string;
  back_translation: string;
  back_translation_state:
    "none" | "pending" | "complete" | "failed" | "cancelled";
  profile: Profile;
  attempt: number;
  attempts: {
    number: number;
    profile: Profile;
    status: string;
    error: string | null;
    reason: string;
    partial_text: string;
    source_revision?: number;
    back_translation?: string;
    failure?: FailureSummary | null;
  }[];
  error: string | null;
  sensitive: Task["sensitive"];
  output_language: string;
  reference_language: string;
  created_at: number;
  updated_at: number;
  revision: number;
  save_state?: string;
  save_error?: string | null;
  failure?: FailureSummary | null;
  memory_only?: boolean;
};
export type OperationRequest = {
  request_id: string;
  previous_operation_id?: string | null;
  kind: "explain" | "reply";
  source_text: string;
  task_id?: string | null;
  source_revision?: number | null;
  profile_id?: string | null;
  source_language?: string | null;
  target_language: string;
  selection_start?: number | null;
  selection_end?: number | null;
  context_scope?: "paragraph" | "full";
  intent?: string;
  tone?: "default" | "professional" | "casual" | "concise";
  reply_language?: string | null;
  include_back_translation?: boolean;
};
