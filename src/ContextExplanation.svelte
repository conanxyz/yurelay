<script lang="ts">
  import type { Locale } from "./locale";
  let {
    locale,
    selection,
    scope = $bindable("paragraph"),
    busy,
    onGenerate,
  }: {
    locale: Locale;
    selection: string;
    scope?: "paragraph" | "full";
    busy: boolean;
    onGenerate: () => void;
  } = $props();
  const t = (zh: string, en: string) => (locale === "zh" ? zh : en);
</script>

<div class="explanation-compose">
  <p class="selection-excerpt">
    {selection ||
      t(
        "先在原文中选中要解析的片段。",
        "Select a passage in the source first.",
      )}
  </p>
  <label
    >{t("发送范围", "Context to send")}<select
      aria-label={t("解析上下文范围", "Explanation context scope")}
      bind:value={scope}
      ><option value="paragraph"
        >{t(
          "所在段落与相邻段落",
          "Selected paragraph and its neighbors",
        )}</option
      ><option value="full">{t("全文", "Full source")}</option></select
    ></label
  >
  <button class="primary" disabled={busy || !selection} onclick={onGenerate}
    >{t("生成语境解析", "Generate explanation")}</button
  >
</div>
