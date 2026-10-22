<script lang="ts">
  import type { Profile, Settings } from "./types";
  import type { Locale } from "./locale";
  let {
    profile,
    settings,
    locale,
    defaultTranslation,
    defaultReply,
    onOpenPrompts,
    onChange,
  }: {
    profile: Profile;
    settings: Settings;
    locale: Locale;
    defaultTranslation: string;
    defaultReply: string;
    onOpenPrompts: () => void;
    onChange: (
      field: "custom_prompt" | "custom_reply_prompt",
      value: string,
    ) => void;
  } = $props();
  const t = (zh: string, en: string) => (locale === "zh" ? zh : en);
  let translationCustom = $state(false),
    replyCustom = $state(false);
  let displayedProfile: Profile | undefined;
  $effect(() => {
    // Save/revert replace the draft object even when the service ID is unchanged.
    // In-place edits keep a custom editor open while its text is temporarily empty.
    if (displayedProfile !== profile) {
      displayedProfile = profile;
      translationCustom = !!profile.custom_prompt?.trim();
      replyCustom = !!profile.custom_reply_prompt?.trim();
    }
  });
  function chooseTranslation(value: string) {
    translationCustom = value === "custom";
    onChange(
      "custom_prompt",
      translationCustom
        ? profile.custom_prompt ||
            settings.translation_prompt ||
            defaultTranslation
        : "",
    );
  }
  function chooseReply(value: string) {
    replyCustom = value === "custom";
    onChange(
      "custom_reply_prompt",
      replyCustom
        ? profile.custom_reply_prompt || settings.reply_prompt || defaultReply
        : "",
    );
  }
</script>

<section
  class="service-prompt-references"
  aria-label={t("服务提示词", "Service prompts")}
>
  <label
    >{t("翻译提示词", "Translation prompt")}<select
      aria-label={t("翻译提示词来源", "Translation prompt source")}
      value={translationCustom ? "custom" : "global"}
      onchange={(e) => chooseTranslation(e.currentTarget.value)}
      ><option value="global">{t("使用全局提示词", "Use global prompt")}</option
      ><option value="custom"
        >{t("此服务自定义", "Custom for this service")}</option
      ></select
    ></label
  >
  {#if translationCustom}<textarea
      aria-label={t("此服务的翻译提示词", "Service translation prompt")}
      value={profile.custom_prompt ?? ""}
      oninput={(e) => onChange("custom_prompt", e.currentTarget.value)}
      rows="5"
      maxlength="32000"
      spellcheck="false"></textarea>{/if}
  <label
    >{t("回复翻译提示词", "Reply prompt")}<select
      aria-label={t("回复提示词来源", "Reply prompt source")}
      value={replyCustom ? "custom" : "global"}
      onchange={(e) => chooseReply(e.currentTarget.value)}
      ><option value="global">{t("使用全局提示词", "Use global prompt")}</option
      ><option value="custom"
        >{t("此服务自定义", "Custom for this service")}</option
      ></select
    ></label
  >
  {#if replyCustom}<textarea
      aria-label={t("此服务的回复提示词", "Service reply prompt")}
      value={profile.custom_reply_prompt ?? ""}
      oninput={(e) => onChange("custom_reply_prompt", e.currentTarget.value)}
      rows="5"
      maxlength="32000"
      spellcheck="false"></textarea>{/if}
  <button class="inline-action" type="button" onclick={onOpenPrompts}
    >{t("编辑全局提示词", "Edit global prompts")}</button
  >
</section>
