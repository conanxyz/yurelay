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
  {#if kind === "reply" && replyLoading}<p class="subtle" role="status">
      {t("正在读取内置提示词…", "Loading built-in instructions…")}
    </p>{/if}
  {#if kind === "reply" && replyError}<p class="error" role="alert">
      {replyError}
    </p>{/if}
  <textarea
    aria-describedby="global-prompt-help"
    class:default-prompt={!custom?.trim()}
    aria-label={kind === "translation"
      ? t("翻译提示词", "Translation prompt")
      : t("回复翻译提示词", "Reply prompt")}
    value={custom?.trim() ? custom : builtin}
    oninput={(e) => change(e.currentTarget.value)}
    rows="15"
    maxlength="32000"
    spellcheck="false"></textarea>
  <small id="global-prompt-help" class="subtle prompt-help">
    {kind === "translation"
      ? t(
          "模型服务默认使用，可按服务自定义。修改后保存，清空恢复内置。",
          "Default for model services; override per service. Save edits; clear to restore built-in.",
        )
      : t(
          "结合原文自然表达你的意思，不新增事实或承诺。修改后保存，清空恢复内置。",
          "Use context to express your intent naturally, without added facts or promises. Save edits; clear to restore built-in.",
        )}
  </small>
</section>
