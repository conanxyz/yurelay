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
export type Task = {
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
  past_attempts: { number: number; translation: string; status: string }[];
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
  retention: { mode: "days", days: 7 },
  selection_shortcut: "Alt+D",
  screenshot_shortcut: "Alt+S",
  auto_hide: false,
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
