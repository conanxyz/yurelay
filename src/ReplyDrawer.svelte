<script lang="ts">
  import type { Locale } from "./locale";
  let {
    locale,
    intent = $bindable(""),
    busy,
    hasOutput,
    onGenerate,
  }: {
    locale: Locale;
    intent?: string;
    busy: boolean;
    hasOutput: boolean;
    onGenerate: () => void;
  } = $props();
  const t = (zh: string, en: string) => (locale === "zh" ? zh : en);
</script>

<div class="reply-compose">
  <label
    >{t("想表达 / 回复什么", "What would you like to say?")}<textarea
      aria-label={t("回复意图", "Reply intent")}
      bind:value={intent}
      maxlength="50000"
      rows="3"
      placeholder={t(
        "写下意思或要点，会结合原文优化成自然的回复。",
        "Write your meaning or key points. The source context helps shape a natural reply.",
      )}></textarea></label
  >
  <button class="primary" disabled={busy || !intent.trim()} onclick={onGenerate}
    >{hasOutput
      ? t("重新生成回复", "Regenerate reply")
      : t("生成回复", "Generate reply")}</button
  >
</div>
