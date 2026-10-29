<script lang="ts">
  import { onMount, tick, untrack } from "svelte";
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import type {
    VocabularyCandidate,
    VocabularyItem,
    SpeechVoice,
  } from "./types";
  import type { Locale } from "./locale";
  import { errorText } from "./locale";
  import WordMeaning from "./WordMeaning.svelte";
  import Icon from "./Icons.svelte";
  let {
    candidates: incomingCandidates,
    language,
    profileId,
    allowModel,
    locale,
    open,
    onToggle,
    sourceText,
    activeWord = "",
  }: {
    candidates: VocabularyCandidate[];
    language: string;
    profileId: string | null;
    allowModel: boolean;
    locale: Locale;
    open: boolean;
    onToggle: () => void;
    sourceText: string;
    activeWord?: string;
  } = $props();
  let enriched = $state<Record<string, VocabularyCandidate>>({});
  const candidates = $derived(
    incomingCandidates.map((c) => {
      const context = items
        .find((item) => item.word.toLowerCase() === c.word.toLowerCase())
        ?.contexts.find(
          (context) =>
            normalize(context.context_sentence) ===
            normalize(c.context_sentence),
        );
      return {
        ...c,
        meanings: {
          ...context?.meanings,
          ...enriched[`${c.word}:${c.context_sentence}`]?.meanings,
          ...c.meanings,
        },
      };
    }),
  );
  const t = (zh: string, en: string) => (locale === "zh" ? zh : en);
  const normalize = (text: string) => text.trim().replace(/\s+/gu, " ");
  const wordKey = (c: VocabularyCandidate) => c.word.trim().toLowerCase();
  let items = $state<VocabularyItem[]>([]),
    busy = $state<string | null>(null),
    error = $state("");
  let lookupError = $state("");
  let expanded = $state<string | null>(null);
  let toggleButton: HTMLButtonElement;
  let scroller = $state<HTMLDivElement>();
  let voices = $state<SpeechVoice[]>([]);
  let speaking = $state<string | null>(null);
  let firstWord = "";
  const accents = [
    { label: "UK", locale: "en-GB" },
    { label: "US", locale: "en-US" },
  ];
  $effect(() => {
    const first = candidates[0]?.word ?? "";
    const shown = open;
    untrack(() => {
      if (shown && first !== firstWord) {
        firstWord = first;
        tick().then(() => {
          if (scroller) scroller.scrollTop = 0;
        });
      }
    });
  });
  function voiceAvailable(locale: string) {
    return voices.some((voice) => voice.locale === locale && voice.available);
  }
  async function speak(
    candidate: VocabularyCandidate,
    locale: string,
    slow: boolean,
  ) {
    if (speaking || !voiceAvailable(locale)) return;
    speaking = candidate.word;
    error = "";
    try {
      await invoke("speak_word", { word: candidate.word, locale, slow });
    } catch (e) {
      error = String(e);
    } finally {
      speaking = null;
    }
  }
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
        words: incomingCandidates.map((c) => c.word),
      });
      if (seq === sequence) {
        items = result ?? [];
        lookupError = "";
      }
    } catch (e) {
      if (seq === sequence) lookupError = String(e);
    }
  }
  $effect(() => {
    const words = incomingCandidates.map((c) => c.word).join("\n");
    if (words && isTauri()) refresh();
  });
  onMount(() => {
    if (!isTauri()) return;
    let disposed = false;
    let off: () => void;
    invoke<SpeechVoice[]>("speech_voices")
      .then((result) => {
        if (!disposed) voices = result ?? [];
      })
      .catch((e) => {
        if (!disposed) error = String(e);
      });
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
  $effect(() => {
    const selected = candidates.find(
      (candidate) => wordKey(candidate) === activeWord,
    );
    if (selected && activeWord) expanded = selected.word;
  });
  async function markMastered(item: VocabularyItem) {
    if (busy) return;
    busy = item.word;
    error = "";
    try {
      await invoke("vocabulary_update", { id: item.id, status: "mastered" });
      await refresh();
    } catch (e) {
      error = String(e);
    } finally {
      busy = null;
    }
  }
  function contextDate(value: number) {
    return new Date(value).toLocaleString(locale === "zh" ? "zh-CN" : "en-US", {
      year: "numeric",
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  }
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
      Object.keys(c.meanings ?? {}).some((lang) => !context.meanings?.[lang]) ||
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
        candidate: $state.snapshot(c),
        sourceText,
      });
      const latest = enriched[`${c.word}:${c.context_sentence}`];
      if (latest)
        await invoke("vocabulary_enrich_existing", {
          candidate: $state.snapshot(latest),
          sourceText,
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
    if (
      open &&
      e.key === "Escape" &&
      !e.defaultPrevented &&
      !e.repeat &&
      !e.isComposing &&
      !e.ctrlKey &&
      !e.altKey &&
      !e.metaKey &&
      !e.shiftKey &&
      !document.querySelector("dialog[open]") &&
      !(e.target as HTMLElement)?.closest('select,[role="combobox"]') &&
      !document.activeElement?.closest('select,[role="combobox"]')
    ) {
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
  {#if open}<div class="vocabulary-sidebar-scroll" bind:this={scroller!}>
      {#if error || lookupError}<p class="drawer-error" role="alert">
          {errorText(error || lookupError, locale)}
        </p>{/if}
      {#each candidates as candidate (wordKey(candidate))}
        <article
          class="vocabulary-side-card"
          data-word={candidate.word}
          class:active={activeWord === wordKey(candidate)}
        >
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
              >{candidate.meanings?.[language]?.contextual ||
                candidate.meanings?.[language]?.general ||
                t(
                  "目标语言释义暂缺",
                  "Meaning not available in the target language",
                )}</span
            >
          </button>
          {#if phonetic(candidate)}<div class="vocabulary-pronunciation">
              <span
                class="vocabulary-ipa"
                aria-label={t("音标", "Pronunciation")}
                >{phonetic(candidate)}</span
              >
            </div>{/if}
          <div
            class="vocabulary-speech"
            aria-label={t("单词朗读", "Word pronunciation")}
          >
            {#each accents as accent}<div class="vocabulary-speech-accent">
                <span>{accent.label}</span>
                {#each [false, true] as slow}<button
                    class="vocabulary-speak"
                    aria-label={slow
                      ? t(
                          `慢速朗读 ${candidate.word}（${accent.label}）`,
                          `Play ${candidate.word} slowly (${accent.label})`,
                        )
                      : t(
                          `朗读 ${candidate.word}（${accent.label}）`,
                          `Play ${candidate.word} (${accent.label})`,
                        )}
                    title={voiceAvailable(accent.locale)
                      ? slow
                        ? t("慢速朗读", "Slow pronunciation")
                        : t("正常语速", "Normal pronunciation")
                      : t(
                          `请先安装 ${accent.label} 英语系统声音`,
                          `Install a ${accent.label} English system voice first`,
                        )}
                    disabled={speaking !== null ||
                      !voiceAvailable(accent.locale)}
                    onclick={() => speak(candidate, accent.locale, slow)}
                    >{#if slow}<span>{t("慢", "Slow")}</span>{:else}<Icon
                        name="speaker"
                        size={13}
                      />{/if}</button
                  >{/each}
              </div>{/each}
          </div>
          <div class="vocabulary-card-actions">
            <span class="vocabulary-card-meta"
              >{#if candidate.pos}<span class="vocabulary-pos"
                  >{candidate.pos}</span
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
              <WordMeaning
                {candidate}
                {sourceText}
                {language}
                {profileId}
                {allowModel}
                {locale}
                onUpdate={(value) => {
                  enriched = {
                    ...enriched,
                    [`${value.word}:${value.context_sentence}`]: value,
                  };
                }}
              />
              {#if candidate.context_sentence}<blockquote>
                  {candidate.context_sentence}
                </blockquote>{/if}
              {#if candidate.meanings?.[language]?.sentence_translation}<p>
                  {candidate.meanings[language].sentence_translation}
                </p>{/if}
              {#if saved(candidate)}{@const item = saved(candidate)!}
                <details class="saved-word-contexts">
                  <summary
                    >{t("已存语境", "Saved contexts")} · {item.contexts
                      .length}</summary
                  >{#each item.contexts as context}<div
                      class="saved-word-context"
                    >
                      <blockquote>{context.context_sentence}</blockquote>
                      {#if context.context_meaning}<p>
                          {context.context_meaning}
                        </p>{/if}{#if context.sentence_trans}<p>
                          {context.sentence_trans}
                        </p>{/if}{#if context.created_at}<time
                          datetime={new Date(context.created_at).toISOString()}
                          >{contextDate(context.created_at)}</time
                        >{/if}
                    </div>{/each}
                </details>
                {#if item.status === "learning"}<button
                    class="mastered-control"
                    disabled={busy !== null}
                    onclick={() => markMastered(item)}
                    >{t("标为已掌握", "Mark as mastered")}</button
                  >{:else}<small>{t("已掌握", "Mastered")}</small>{/if}{/if}
            </div>{/if}
        </article>
      {/each}
    </div>{/if}
</aside>
