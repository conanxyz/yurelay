<script lang="ts">
  import { onMount, tick } from "svelte";
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import type { VocabularyCandidate, VocabularyItem } from "./types";
  import type { Locale } from "./locale";
  import Icon from "./Icons.svelte";
  import { panelEntrance } from "./motion";
  let {
    candidates,
    locale,
    open,
    onToggle,
  }: {
    candidates: VocabularyCandidate[];
    locale: Locale;
    open: boolean;
    onToggle: () => void;
  } = $props();
  const t = (zh: string, en: string) => (locale === "zh" ? zh : en);
  const normalize = (text: string) => text.trim().replace(/\s+/gu, " ");
  const wordKey = (c: VocabularyCandidate) => c.word.trim().toLowerCase();
  let items = $state<VocabularyItem[]>([]),
    busy = $state<string | null>(null),
    error = $state("");
  let expanded = $state<string | null>(null);
  let toggleButton: HTMLButtonElement;
  function toggleSide() {
    onToggle();
    tick().then(() => toggleButton?.focus({ preventScroll: true }));
  }
  function phonetic(c: VocabularyCandidate) {
    return c.difficulty?.toLowerCase() === "term" ||
      /[0-9_]|[A-Z].*[A-Z]|[a-z][A-Z]/.test(c.word)
      ? ""
      : c.phonetic;
  }

  let sequence = 0;
  async function refresh() {
    const seq = ++sequence;
    try {
      const result = await invoke<VocabularyItem[]>("vocabulary_lookup", {
        words: candidates.map((c) => c.word),
      });
      if (seq === sequence) items = result ?? [];
    } catch (e) {
      error = String(e);
    }
  }
  $effect(() => {
    const words = candidates.map((c) => c.word).join("\n");
    if (words && isTauri()) refresh();
  });
  onMount(() => {
    if (!isTauri()) return;
    let disposed = false;
    let off: () => void;
    listen("vocabulary-changed", refresh).then((f) => {
      if (disposed) f();
      else off = f;
    });
    return () => {
      disposed = true;
      sequence++;
      off?.();
    };
  });
  function saved(c: VocabularyCandidate) {
    return items.find((i) => i.word.trim().toLowerCase() === wordKey(c));
  }
  function canCollect(c: VocabularyCandidate) {
    const item = saved(c);
    if (!item || item.status === "mastered") return true;
    const context = item.contexts.find(
      (v) => normalize(v.context_sentence) === normalize(c.context_sentence),
    );
    if (!context) return true;
    return (
      (!!phonetic(c) && !item.phonetic) ||
      (!!c.pos && !item.pos) ||
      (!!c.dict_meaning && !item.dict_meaning) ||
      (!!c.difficulty && !item.difficulty) ||
      (!!c.context_meaning && !context.context_meaning) ||
      (!!c.sentence_trans && !context.sentence_trans)
    );
  }
  function label(c: VocabularyCandidate) {
    const item = saved(c);
    return item?.status === "mastered"
      ? t("重新学习", "Learn again")
      : item
        ? t(`已收录 (×${item.review_count})`, `Saved (×${item.review_count})`)
        : t("收录", "Add");
  }
  async function collect(c: VocabularyCandidate) {
    if (busy || !canCollect(c)) return;
    busy = c.word;
    error = "";
    try {
      const item = await invoke<VocabularyItem>("vocabulary_collect", {
        candidate: c,
      });
      sequence++;
      items = [...items.filter((i) => i.id !== item.id), item];
    } catch (e) {
      error = String(e);
    } finally {
      busy = null;
    }
  }
</script>

<svelte:window
  onkeydown={(e) => {
    if (open && e.key === "Escape") {
      e.preventDefault();
      toggleSide();
    }
  }}
/>
<aside
  class="vocabulary-sidebar"
  class:collapsed={!open}
  aria-label={t("生词", "Vocabulary")}
>
  <header class="vocabulary-sidebar-heading">
    {#if open}<Icon name="book" size={16} /><strong
        >{t("生词", "Vocabulary")}</strong
      ><span class="count-tag">{candidates.length}</span>{/if}
    <button
      bind:this={toggleButton!}
      class="vocabulary-side-toggle icon-button"
      aria-label={open
        ? t("收起生词", "Collapse vocabulary")
        : t("展开生词", "Expand vocabulary")}
      title={open
        ? t("收起生词", "Collapse vocabulary")
        : t(`生词 ${candidates.length}`, `Vocabulary ${candidates.length}`)}
      aria-expanded={open}
      onclick={toggleSide}
    >
      <Icon name={open ? "chevron" : "book"} size={16} />{#if !open}<span
          >{candidates.length}</span
        >{/if}
    </button>
  </header>
  {#if open}<div class="vocabulary-sidebar-scroll" use:panelEntrance>
      {#if error}<p class="drawer-error" role="alert">{error}</p>{/if}
      {#each candidates as candidate}
        <article class="vocabulary-side-card">
          <button
            class="vocabulary-row-summary"
            aria-label={t(
              `查看 ${candidate.word} 释义`,
              `View ${candidate.word} meaning`,
            )}
            aria-expanded={expanded === candidate.word}
            onclick={() =>
              (expanded = expanded === candidate.word ? null : candidate.word)}
          >
            <strong>{candidate.word}</strong><span
              class="vocabulary-row-meaning"
              >{candidate.context_meaning || candidate.dict_meaning}</span
            >
          </button>
          <div class="vocabulary-card-actions">
            <span class="vocabulary-card-meta"
              >{#if candidate.pos}<span class="vocabulary-pos"
                  >{candidate.pos}</span
                >{/if}{#if phonetic(candidate)}<span class="vocabulary-ipa"
                  >{phonetic(candidate)}</span
                >{/if}</span
            >
            <button
              class="collect-word"
              disabled={busy !== null || !canCollect(candidate)}
              onclick={() => collect(candidate)}
              title={t(
                "收录当前语境，同一句不会重复计数",
                "Collect this context; the same sentence is counted once",
              )}
              ><Icon
                name={saved(candidate) ? "check" : "plus"}
                size={12}
              />{label(candidate)}</button
            >
          </div>
          {#if expanded === candidate.word}<div class="vocabulary-row-detail">
              {#if candidate.dict_meaning && candidate.dict_meaning !== candidate.context_meaning}<p
                >
                  {candidate.dict_meaning}
                </p>{/if}
              {#if candidate.context_sentence}<blockquote>
                  {candidate.context_sentence}
                </blockquote>{/if}
              {#if candidate.sentence_trans}<p>
                  {candidate.sentence_trans}
                </p>{/if}
            </div>{/if}
        </article>
      {/each}
    </div>{/if}
</aside>
