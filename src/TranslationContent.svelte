<script lang="ts">
  import { onMount, tick, untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import type { Locale } from "./locale";
  import {
    externalLink,
    readingBlocks,
    highlightedReading,
    highlightedText,
    readingTableText,
    type TextHighlight,
  } from "./reading";
  import { ensureMath } from "./math";
  import Icon from "./Icons.svelte";
  let {
    text,
    locale,
    identity = "",
    streaming = false,
    plain = false,
    controls = true,
    highlights = [],
  }: {
    text: string;
    locale: Locale;
    identity?: string;
    streaming?: boolean;
    plain?: boolean;
    controls?: boolean;
    highlights?: TextHighlight[];
  } = $props();
  const t = (zh: string, en: string) => (locale === "zh" ? zh : en);
  let mode = $state<"read" | "plain">("read");
  let mathEnabled = $state(true);
  let pendingPaint = false;
  let shown = $state(""),
    blocks = $state<string[]>([]),
    error = $state("");
  let content: HTMLDivElement;
  const highlightedBlocks = $derived(
    highlightedReading(blocks, shown, highlights),
  );
  const plainParts = $derived(highlightedText(shown, highlights));
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
    if (selecting() && !changedIdentity) {
      pendingPaint = true;
      return;
    }
    pendingPaint = false;
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
    const mathText = text;
    if (mathEnabled && text.includes("$"))
      void ensureMath(text).then(() => {
        if (text !== mathText || identity !== paintedIdentity) return;
        if (selecting()) {
          pendingPaint = true;
          return;
        }
        blocks = plain ? [] : readingBlocks(text, mathEnabled);
      });
    try {
      blocks = plain ? [] : readingBlocks(text, mathEnabled);
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
    mathEnabled;
    untrack(schedule);
  });
  async function copyTable(block: string) {
    try {
      await invoke("copy_text", { text: readingTableText(block) });
    } catch (e) {
      error = String(e);
    }
  }
  async function copyMarkdown() {
    try {
      await invoke("copy_text", { text });
    } catch (e) {
      error = String(e);
    }
  }
  onMount(() => {
    const selection = () => {
      if (!selecting() && (shown !== text || pendingPaint)) schedule();
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
  {#if !plain && controls}<div class="reader-tools">
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
      {#if text.includes("$")}<button
          class="math-toggle"
          aria-pressed={mathEnabled}
          onclick={() => (mathEnabled = !mathEnabled)}
          >{t("公式排版", "Math layout")}</button
        >{/if}
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
    {#if plain || mode === "plain"}{#each plainParts as part}{#if part.highlighted}<mark
            class="translation-word-highlight">{part.text}</mark
          >{:else}{part.text}{/if}{/each}{:else}{#each highlightedBlocks as block, i (i)}<div
          class="reading-block"
        >
          {#if block.includes("<table>")}<button
              class="table-copy"
              disabled={streaming}
              onclick={() => copyTable(block)}
              >{t("复制表格", "Copy table")}</button
            >{/if}
          {@html block}
        </div>{/each}{/if}
  </div>
  {#if error}<p class="reading-error" role="alert">{error}</p>{/if}
</div>
