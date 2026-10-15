<script lang="ts">
  import { onMount, tick, untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import type { Locale } from "./locale";
  import { externalLink, readingBlocks } from "./reading";
  import Icon from "./Icons.svelte";
  let {
    text,
    locale,
    identity = "",
    streaming = false,
    plain = false,
  }: {
    text: string;
    locale: Locale;
    identity?: string;
    streaming?: boolean;
    plain?: boolean;
  } = $props();
  const t = (zh: string, en: string) => (locale === "zh" ? zh : en);
  let mode = $state<"read" | "plain">("read");
  let shown = $state(""),
    blocks = $state<string[]>([]),
    error = $state("");
  let content: HTMLDivElement;
  let renderedIdentity = "";
  let timer: ReturnType<typeof setTimeout> | undefined;
  function selecting() {
    const s = window.getSelection();
    return (
      !!s &&
      !s.isCollapsed &&
      !!content &&
      (content.contains(s.anchorNode) || content.contains(s.focusNode))
    );
  }
  function paint() {
    timer = undefined;
    const changedIdentity = renderedIdentity !== identity;
    if (selecting() && !changedIdentity) return;
    if (changedIdentity) {
      if (selecting()) window.getSelection()?.removeAllRanges();
      renderedIdentity = identity;
      error = "";
    }
    let scroller: Element | null = content;
    while (
      scroller &&
      !(
        scroller.scrollHeight > scroller.clientHeight &&
        /auto|scroll/.test(getComputedStyle(scroller).overflowY)
      )
    )
      scroller = scroller.parentElement;
    scroller ??= document.scrollingElement;
    const top = scroller?.scrollTop || 0;
    const preserve =
      streaming &&
      !changedIdentity &&
      scroller &&
      scroller.scrollHeight - top - scroller.clientHeight > 12;
    const paintedIdentity = identity;
    shown = text;
    try {
      blocks = plain ? [] : readingBlocks(text);
    } catch {
      blocks = [];
      mode = "plain";
      error = t(
        "阅读排版暂不可用，已显示纯文本",
        "Reading layout is unavailable; showing plain text",
      );
    }
    if (preserve)
      tick().then(() => {
        if (identity === paintedIdentity) scroller!.scrollTop = top;
      });
  }
  function schedule() {
    // Background WKWebView windows can suspend animation frames indefinitely.
    // Completed text must commit independently of frame delivery.
    if (!streaming) {
      clearTimeout(timer);
      timer = undefined;
      queueMicrotask(paint);
      return;
    }
    if (timer !== undefined) return;
    timer = setTimeout(paint, Math.min(200, Math.max(32, text.length / 400)));
  }
  $effect(() => {
    text;
    identity;
    plain;
    streaming;
    untrack(schedule);
  });
  async function copyMarkdown() {
    try {
      await invoke("copy_text", { text });
    } catch (e) {
      error = String(e);
    }
  }
  onMount(() => {
    const selection = () => {
      if (!selecting() && shown !== text) schedule();
    };
    const open = async (e: Event) => {
      const anchor = (e.target as Element)?.closest("a");
      if (!anchor || !content.contains(anchor)) return;
      e.preventDefault();
      if (e.type !== "click") return;
      const url = externalLink(anchor.getAttribute("href") || "");
      if (url)
        try {
          await invoke("open_translation_link", { url });
        } catch (err) {
          error = String(err);
        }
    };
    document.addEventListener("selectionchange", selection);
    content.addEventListener("click", open);
    content.addEventListener("auxclick", open);
    content.addEventListener("contextmenu", open);
    return () => {
      document.removeEventListener("selectionchange", selection);
      content.removeEventListener("click", open);
      content.removeEventListener("auxclick", open);
      content.removeEventListener("contextmenu", open);
      clearTimeout(timer);
    };
  });
</script>

<div class="translation-reader">
  {#if !plain}<div class="reader-tools">
      <div
        class="reader-mode"
        aria-label={t("译文显示方式", "Translation display")}
      >
        <button aria-pressed={mode === "read"} onclick={() => (mode = "read")}
          >{t("阅读", "Read")}</button
        >
        <button aria-pressed={mode === "plain"} onclick={() => (mode = "plain")}
          >{t("纯文本", "Plain text")}</button
        >
      </div>
      <button
        class="reader-copy-markdown icon-button"
        disabled={streaming}
        title={t("复制 Markdown", "Copy Markdown")}
        aria-label={t("复制 Markdown", "Copy Markdown")}
        onclick={copyMarkdown}><Icon name="copy" size={14} /></button
      >
    </div>{/if}
  <div
    class="translation-prose"
    class:markdown-reading={!plain && mode === "read"}
    bind:this={content}
  >
    {#if plain || mode === "plain"}{shown}{:else}{#each blocks as block, i (i)}<div
          class="reading-block"
        >
          {@html block}
        </div>{/each}{/if}
  </div>
  {#if error}<p class="reading-error" role="alert">{error}</p>{/if}
</div>
