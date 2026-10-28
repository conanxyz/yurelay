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
  <textarea
    aria-label={t("回复意图", "Reply intent")}
    bind:value={intent}
    maxlength="50000"
    rows="3"
    placeholder={t(
      "写下想表达的意思或要点…",
      "Write your meaning or key points…",
    )}></textarea>
  <button class="primary" disabled={busy || !intent.trim()} onclick={onGenerate}
    >{hasOutput
      ? t("重新生成回复", "Regenerate reply")
      : t("生成回复", "Generate reply")}</button
  >
</div>
