<script lang="ts">
  import { untrack } from "svelte";
  import { translationPlainText } from "./reading";
  import Icon from "./Icons.svelte";
  import type { Locale } from "./locale";
  import { playback, readAloud, stopReading } from "./speech";
  let {
    text,
    language = null,
    locale,
    kind,
    identity,
    markdown = false,
    disabled = false,
  }: {
    text: string;
    language?: string | null;
    locale: Locale;
    kind: "source" | "translation";
    identity: string;
    markdown?: boolean;
    disabled?: boolean;
  } = $props();
  const owner = crypto.randomUUID();
  const t = (zh: string, en: string) => (locale === "zh" ? zh : en);
  const playing = $derived($playback?.owner === owner && $playback.playing);
  const failed = $derived($playback?.owner === owner && $playback.error);
  const label = $derived(
    playing
      ? t(
          kind === "source" ? "停止朗读原文" : "停止朗读译文",
          kind === "source"
            ? "Stop reading source"
            : "Stop reading translation",
        )
      : t(
          kind === "source" ? "朗读原文" : "朗读译文",
          kind === "source" ? "Read source aloud" : "Read translation aloud",
        ),
  );
  $effect(() => {
    void identity;
    return () => untrack(() => stopReading(owner));
  });
</script>

<div class="read-aloud-actions">
  <button
    class="icon-button read-aloud-button"
    aria-label={label}
    title={label}
    aria-pressed={playing}
    disabled={!playing && (disabled || !text.trim())}
    onclick={() =>
      playing
        ? stopReading(owner)
        : readAloud(
            owner,
            markdown ? translationPlainText(text) : text,
            language,
          )}
  >
    <Icon name={playing ? "stop" : "speaker"} size={17} />
  </button>
  {#if playing}<span class="sr-only" role="status"
      >{t("正在使用本机语音朗读", "Reading with a local system voice")}</span
    >{/if}
  {#if failed}<span class="read-aloud-error" role="alert"
      >{t(
        "朗读失败，请检查系统声音后重试。",
        "Could not read aloud. Check installed system voices and try again.",
      )}</span
    >{/if}
</div>

<style>
  .read-aloud-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 10px 4px;
    min-height: 32px;
  }
  .read-aloud-button {
    width: 32px;
    height: 32px;
    flex-shrink: 0;
  }
  .read-aloud-button[aria-pressed="true"] {
    color: var(--accent);
    background: var(--accent-soft);
  }
  .read-aloud-error {
    font-size: 12px;
    color: var(--muted);
  }
</style>
