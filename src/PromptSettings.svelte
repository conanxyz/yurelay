<script lang="ts">
  import type { Settings } from "./types";
  import type { Locale } from "./locale";
  let {
    settings = $bindable(),
    locale,
    defaultTranslation,
    defaultReply,
    replyLoading = false,
    replyError = "",
  }: {
    settings: Settings;
    locale: Locale;
    defaultTranslation: string;
    defaultReply: string;
    replyLoading?: boolean;
    replyError?: string;
  } = $props();
  const t = (zh: string, en: string) => (locale === "zh" ? zh : en);
  let kind = $state<"translation" | "reply">("translation");
  const custom = $derived(
    kind === "translation"
      ? settings.translation_prompt
      : settings.reply_prompt,
  );
  const builtin = $derived(
    kind === "translation" ? defaultTranslation : defaultReply,
  );
  function change(value: string) {
    if (kind === "translation") settings.translation_prompt = value;
    else settings.reply_prompt = value;
  }
</script>

<section
  class="form-section global-prompts"
  aria-label={t("提示词设置", "Prompt settings")}
>
  <div class="prompt-kind-switch" aria-label={t("提示词类型", "Prompt type")}>
    <button
      type="button"
      aria-pressed={kind === "translation"}
      onclick={() => (kind = "translation")}>{t("翻译", "Translation")}</button
    ><button
      type="button"
      aria-pressed={kind === "reply"}
      onclick={() => (kind = "reply")}>{t("回复翻译", "Reply writing")}</button
    >
  </div>
  <div class="section-heading">
    <h2>
      {kind === "translation"
        ? t("翻译提示词", "Translation prompt")
        : t("回复翻译提示词", "Reply prompt")}
    </h2>
    <button type="button" disabled={!custom?.trim()} onclick={() => change("")}
      >{t("恢复内置", "Restore built-in")}</button
    >
  </div>
  <p class="subtle">
    {kind === "translation"
      ? t(
          "所有模型服务默认引用这套翻译提示词；服务中可单独自定义。",
          "Model services use these translation instructions by default. Each service can override them.",
        )
      : t(
          "结合原文理解你的意思，把回复优化成目标读者习惯的自然表达，不添加你未提供的承诺或事实。",
          "Use source context to express your intent naturally for the reader, without adding facts or commitments you did not provide.",
        )}
  </p>
  {#if kind === "reply" && replyLoading}<p class="subtle" role="status">
      {t("正在读取内置提示词…", "Loading built-in instructions…")}
    </p>{/if}
  {#if kind === "reply" && replyError}<p class="error" role="alert">
      {replyError}
    </p>{/if}
  <textarea
    class:default-prompt={!custom?.trim()}
    aria-label={kind === "translation"
      ? t("翻译提示词", "Translation prompt")
      : t("回复翻译提示词", "Reply prompt")}
    value={custom?.trim() ? custom : builtin}
    oninput={(e) => change(e.currentTarget.value)}
    rows="15"
    maxlength="32000"
    spellcheck="false"></textarea>
  <small class="subtle"
    >{t(
      "修改后点击保存。清空内容会恢复内置提示词。",
      "Click Save after editing. Empty content uses the built-in instructions.",
    )}</small
  >
</section>
