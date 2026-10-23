import type { Profile } from "./types";
import type { Locale } from "./locale";

export type ServicePreset = {
  provider: Profile["provider"];
  name: string;
  zh: string;
  symbol: string;
  endpoint: string;
  model: string;
  models: string[];
};
export const servicePresets: ServicePreset[] = [
  {
    provider: "custom",
    name: "Custom service",
    zh: "自定义服务",
    symbol: "C",
    endpoint: "",
    model: "",
    models: [],
  },
  {
    provider: "deepseek",
    name: "DeepSeek",
    zh: "DeepSeek",
    symbol: "D",
    endpoint: "https://api.deepseek.com/v1",
    model: "deepseek-flash",
    models: ["deepseek-flash", "deepseek-v4-pro"],
  },
  {
    provider: "minimax",
    name: "MiniMax",
    zh: "MiniMax",
    symbol: "M",
    endpoint: "https://api.minimax.io/v1",
    model: "MiniMax-M3",
    models: [
      "MiniMax-M3",
      "MiniMax-M2.7-highspeed",
      "MiniMax-M2.7",
      "MiniMax-M2.5-highspeed",
      "MiniMax-M2.5",
    ],
  },
  {
    provider: "gemini",
    name: "Gemini",
    zh: "Gemini",
    symbol: "G",
    endpoint: "https://generativelanguage.googleapis.com/v1beta/openai",
    model: "gemini-2.5-flash-lite",
    models: ["gemini-2.5-flash-lite", "gemini-2.5-flash"],
  },
  {
    provider: "openai",
    name: "OpenAI",
    zh: "OpenAI",
    symbol: "O",
    endpoint: "https://api.openai.com/v1",
    model: "",
    models: [],
  },
  {
    provider: "anthropic",
    name: "Claude",
    zh: "Claude",
    symbol: "A",
    endpoint: "https://api.anthropic.com/v1",
    model: "",
    models: [],
  },
  {
    provider: "qwen",
    name: "Qwen",
    zh: "通义千问",
    symbol: "Q",
    endpoint: "https://dashscope.aliyuncs.com/compatible-mode/v1",
    model: "",
    models: [],
  },
  {
    provider: "zhipu",
    name: "Zhipu",
    zh: "智谱",
    symbol: "Z",
    endpoint: "https://open.bigmodel.cn/api/paas/v4",
    model: "glm-4.7-flash",
    models: ["glm-4-flash-250414", "glm-4.7-flash", "glm-4.6v-flash"],
  },
  {
    provider: "siliconflow",
    name: "SiliconFlow",
    zh: "硅基流动",
    symbol: "S",
    endpoint: "https://api.siliconflow.cn/v1",
    model: "Qwen/Qwen2.5-7B-Instruct",
    models: [
      "Qwen/Qwen2.5-7B-Instruct",
      "Qwen/Qwen3-8B",
      "THUDM/GLM-4-9B-0414",
    ],
  },
  {
    provider: "openrouter",
    name: "OpenRouter",
    zh: "OpenRouter",
    symbol: "R",
    endpoint: "https://openrouter.ai/api/v1",
    model: "",
    models: [],
  },
  {
    provider: "system_dictionary",
    name: "System dictionary",
    zh: "系统词典",
    symbol: "Aa",
    endpoint: "",
    model: "",
    models: [],
  },
  {
    provider: "youdao",
    name: "Youdao",
    zh: "有道翻译",
    symbol: "Y",
    endpoint: "https://openapi.youdao.com/api",
    model: "",
    models: [],
  },
];
export const serviceFamily = (provider: Profile["provider"]) =>
  provider === "minimax_china" ? "minimax" : provider;
export const presetFor = (provider: Profile["provider"]) =>
  servicePresets.find((p) => p.provider === serviceFamily(provider))!;
export function serviceName(profile: Profile, locale: Locale) {
  const preset = presetFor(profile.provider);
  return [preset.name, preset.zh, "自定义模型"].includes(profile.name)
    ? locale === "zh"
      ? preset.zh
      : preset.name
    : profile.name;
}
export type ServiceEntry = { profile: Profile; preset?: never } | { profile?: never; preset: ServicePreset };
export function serviceEntries(profiles: Profile[], active: string, query: string): ServiceEntry[] {
  const entries: ServiceEntry[] = profiles.map(profile => ({ profile }));
  for (const preset of servicePresets) {
    if (!profiles.some(p => serviceFamily(p.provider) === preset.provider)) entries.push({ preset });
  }
  const provider = (entry: ServiceEntry) => entry.profile?.provider ?? entry.preset!.provider;
  const rank = (entry: ServiceEntry) => entry.profile?.id === active ? -1
    : provider(entry) === "custom" ? servicePresets.length
    : servicePresets.findIndex(p => p.provider === serviceFamily(provider(entry)));
  return entries.filter(entry => matchesService(provider(entry), entry.profile?.name ?? entry.preset!.name, query))
    .sort((a, b) => rank(a) - rank(b));
}
export const usesModel = (provider: Profile["provider"]) =>
  !["system_dictionary", "youdao"].includes(provider);

const searchAliases: Record<string, string> = {
  custom: "zidingyi zdy 自定义 模型 custom",
  deepseek: "深度求索 shenduqiusuo sdqs",
  minimax: "海螺 hailuo hl",
  gemini: "谷歌 guge gg 双子星 shuangzixing szx",
  openai: "chatgpt gpt",
  anthropic: "克劳德 kelaode kld claude",
  qwen: "通义千问 tongyiqianwen tyqw 阿里 ali",
  zhipu: "智谱 zhipu zp glm",
  siliconflow: "硅基流动 guijiliudong gjld",
  openrouter: "路由 luyou ly",
  system_dictionary:
    "系统词典 xitongcidian xtcd 系统字典 苹果 pingguo apple dictionary",
  youdao: "有道 youdao yd 词典 翻译",
};
export function matchesService(
  provider: Profile["provider"],
  name: string,
  query: string,
) {
  const p = presetFor(provider),
    needle = query.toLowerCase().replace(/\s/g, "");
  return `${name} ${p.name} ${p.zh} ${searchAliases[serviceFamily(provider)]}`
    .toLowerCase()
    .replace(/\s/g, "")
    .includes(needle);
}

export type RemoteModel = { id: string; free: boolean | null };
export type ModelChoice = {
  id: string;
  pricing: "free" | "free_tier" | "paid" | "unknown";
  vision: boolean;
};
export function modelChoices(
  provider: Profile["provider"],
  remote: RemoteModel[],
): ModelChoice[] {
  const preset = presetFor(provider);
  const choices = new Map<string, ModelChoice>();
  for (const id of preset.models) {
    choices.set(id, {
      id,
      pricing:
        provider === "gemini"
          ? "free_tier"
          : ["zhipu", "siliconflow"].includes(provider)
            ? "free"
            : "unknown",
      vision: id === "glm-4.6v-flash",
    });
  }
  for (const model of remote) {
    const prior = choices.get(model.id);
    choices.set(model.id, {
      id: model.id,
      pricing:
        model.free === true
          ? "free"
          : model.free === false
            ? "paid"
            : (prior?.pricing ?? "unknown"),
      vision: prior?.vision ?? false,
    });
  }
  return [...choices.values()];
}
export function modelPriceLabel(
  choice: ModelChoice | undefined,
  locale: Locale,
) {
  if (!choice) return "";
  const zh = locale === "zh";
  const label =
    choice.pricing === "free"
      ? zh
        ? "免费"
        : "Free"
      : choice.pricing === "free_tier"
        ? zh
          ? "免费层"
          : "Free tier"
        : choice.pricing === "paid"
          ? zh
            ? "付费"
            : "Paid"
          : "";
  return choice.vision ? `${label} · ${zh ? "视觉" : "Vision"}` : label;
}
