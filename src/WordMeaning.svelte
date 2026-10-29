<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { untrack, onDestroy } from "svelte";
  import type { VocabularyCandidate } from "./types";
  import { languageName, type Locale } from "./locale";
  let {
    candidate,
    sourceText,
    language,
    profileId,
    allowModel,
    locale,
    onUpdate = () => {},
  }: {
    candidate: VocabularyCandidate;
    sourceText: string;
    language: string;
    profileId: string | null;
    allowModel: boolean;
    locale: Locale;
    onUpdate?: (c: VocabularyCandidate) => void;
  } = $props();
  const t = (zh: string, en: string) => (locale === "zh" ? zh : en);
  const identity = $derived(
    JSON.stringify([
      candidate.word,
      candidate.context_sentence,
      candidate.meanings?.[language],
      language,
      profileId,
      allowModel,
    ]),
  );
  let result = $state<VocabularyCandidate | null>(null),
    busy = $state(false),
    error = $state("");
  let last = "",
    ticket = 0;
  const shown = $derived(result ?? candidate);
  const meaning = $derived(shown.meanings?.[language]);
  async function enrich() {
    if (busy || !allowModel) return;
    const key = identity,
      seq = ++ticket;
    busy = true;
    error = "";
    try {
      const value = await invoke<VocabularyCandidate>("vocabulary_meaning", {
        candidate: $state.snapshot(candidate),
        sourceText,
        language,
        profileId,
      });
      if (seq === ticket && key === identity && value) {
        result = value;
        onUpdate(value);
      }
    } catch (e) {
      if (seq === ticket && key === identity)
        error =
          String(e) === "正文翻译进行中，请稍后补充释义。"
            ? t(
                "正文翻译进行中，请稍后补充释义。",
                "The main translation is running. Try adding the meaning afterwards.",
              )
            : t("释义暂未补充，可重试。", "Meaning unavailable. Please retry.");
    } finally {
      if (seq === ticket && key === identity) busy = false;
    }
  }
  $effect(() => {
    const key = identity;
    untrack(() => {
      if (key !== last) {
        last = key;
        ticket++;
        result = null;
        busy = false;
        error = "";
        if (allowModel && !candidate.meanings?.[language]?.contextual)
          void enrich();
      }
    });
  });
  onDestroy(() => {
    ticket++;
  });
</script>

<div class="localized-word-meaning">
  {#if meaning?.contextual || meaning?.general}<small
      >{languageName(language, locale)} · {t(
        "模型释义",
        "Model meaning",
      )}</small
    >{/if}
  {#if meaning?.contextual}<p>
      <small>{t("语境义", "In this context")}</small><br />{meaning.contextual}
    </p>{/if}
  {#if meaning?.general}<p>
      <small>{t("通用词义", "General meaning")}</small><br />{meaning.general}
    </p>{/if}
  {#if !meaning?.contextual && !meaning?.general}<p role="status">
      {busy
        ? t(
            "正在补充目标语言释义…",
            "Preparing meaning in the target language…",
          )
        : t("目标语言释义暂缺。", "No meaning in the target language yet.")}
    </p>{/if}
  {#if error}<p role="status">{error}</p>{/if}
  {#if allowModel && !busy && !meaning?.contextual}<button onclick={enrich}
      >{t("补充释义", "Get meaning")}</button
    >{/if}
  {#if candidate.dict_meaning}<details>
      <summary>{t("本地词典原文", "Original dictionary entry")}</summary>
      <p>{candidate.dict_meaning}</p>
    </details>{/if}
</div>

<style>
  .localized-word-meaning {
    overflow-wrap: anywhere;
    font-size: inherit;
    line-height: 1.65;
  }
  p {
    margin: 8px 0;
  }
  small,
  summary {
    color: var(--muted);
  }
  details {
    margin: 10px 0;
  }
  summary {
    cursor: pointer;
  }
  details p {
    white-space: pre-wrap;
  }
</style>
