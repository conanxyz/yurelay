<script lang="ts">
  import { onMount, tick } from "svelte";
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import type { VocabularyItem } from "./types";
  import type { Locale } from "./locale";
  import WordMeaning from "./WordMeaning.svelte";
  import Icon from "./Icons.svelte";
  let {
    locale,
    language,
    profileId,
    allowModel,
  }: {
    locale: Locale;
    language: string;
    profileId: string | null;
    allowModel: boolean;
  } = $props();
  const t = (zh: string, en: string) => (locale === "zh" ? zh : en);
  let items = $state<VocabularyItem[]>([]),
    selected = $state(""),
    query = $state(""),
    status = $state("all"),
    sort = $state("recent");
  let error = $state(""),
    busy = $state(false),
    loading = $state(true),
    noteSaving = $state(0),
    notice = $state("");
  let drafts = $state<Record<string, string>>({});
  let current = $derived(items.find((i) => i.id === selected));
  let filtered = $derived(
    items
      .filter(
        (i) =>
          (status === "all" || i.status === status) &&
          `${i.word} ${i.dict_meaning} ${i.note} ${i.contexts.map((c) => `${c.context_sentence} ${c.context_meaning} ${JSON.stringify(c.meanings ?? {})}`).join(" ")}`
            .toLowerCase()
            .includes(query.trim().toLowerCase()),
      )
      .sort((a, b) =>
        sort === "frequency"
          ? b.review_count - a.review_count || b.updated_at - a.updated_at
          : b.updated_at - a.updated_at,
      ),
  );
  let pendingDelete = $state<{ id: string; context?: string } | null>(null);
  let dialog: HTMLDialogElement;
  let exportDialog: HTMLDialogElement;
  let saveQueue: Promise<unknown> = Promise.resolve();
  let sequence = 0;
  async function refresh() {
    const seq = ++sequence;
    try {
      const data = await invoke<VocabularyItem[]>("vocabulary_list");
      if (seq === sequence) {
        items = data ?? [];
        if (selected && !items.some((i) => i.id === selected)) selected = "";
      }
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }
  onMount(() => {
    if (!isTauri()) {
      loading = false;
      return;
    }
    let disposed = false;
    let off: () => void;
    refresh();
    listen("vocabulary-changed", refresh).then((f) => {
      if (disposed) f();
      else off = f;
    });
    return () => {
      disposed = true;
      off?.();
    };
  });
  async function update(args: Record<string, unknown>) {
    error = "";
    try {
      await invoke("vocabulary_update", args);
      await refresh();
      return true;
    } catch (e) {
      error = String(e);
      return false;
    }
  }
  function saveNote(id: string, note: string) {
    drafts[id] = note;
    noteSaving++;
    saveQueue = saveQueue
      .then(() => update({ id, note }))
      .finally(() => noteSaving--);
  }
  async function askDelete(id: string, context?: string) {
    pendingDelete = { id, context };
    await tick();
    dialog.showModal();
  }
  async function remove() {
    if (!pendingDelete) return;
    busy = true;
    error = "";
    try {
      const { id, context } = pendingDelete;
      await saveQueue;
      if (context)
        await invoke("vocabulary_update", { id, removeContext: context });
      else await invoke("vocabulary_remove", { id });
      dialog.close();
      pendingDelete = null;
      await refresh();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
  function highlight(sentence: string, surface: string) {
    const at = surface
      ? sentence.toLowerCase().indexOf(surface.toLowerCase())
      : -1;
    return at < 0
      ? { before: sentence, match: "", after: "" }
      : {
          before: sentence.slice(0, at),
          match: sentence.slice(at, at + surface.length),
          after: sentence.slice(at + surface.length),
        };
  }
  async function exportBook(format: "csv" | "markdown" | "anki") {
    exportDialog.close();
    busy = true;
    error = "";
    notice = "";
    try {
      await saveQueue;
      const saved = await invoke<boolean>("vocabulary_export", {
        format,
        language,
      });
      if (saved) notice = t("生词本已导出", "Vocabulary exported");
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
  async function speak(word: string) {
    try {
      await invoke("speak_word", { word });
    } catch (e) {
      error = String(e);
    }
  }
</script>

<div class="vocabulary-toolbar">
  <label class="vocabulary-search"
    ><Icon name="search" size={17} /><input
      type="search"
      aria-label={t("搜索生词", "Search vocabulary")}
      placeholder={t(
        "搜索单词、释义或例句",
        "Search words, meanings or contexts",
      )}
      bind:value={query}
    /></label
  >
  <select aria-label={t("学习状态", "Learning status")} bind:value={status}
    ><option value="all">{t("全部生词", "All words")}</option><option
      value="learning">{t("学习中", "Learning")}</option
    ><option value="mastered">{t("已掌握", "Mastered")}</option></select
  >
  <select aria-label={t("排序", "Sort")} bind:value={sort}
    ><option value="recent">{t("按最近添加", "Most recent")}</option><option
      value="frequency">{t("按遭遇频次", "Most frequent")}</option
    ></select
  >
  <button
    class="vocabulary-export"
    disabled={busy || !items.length}
    onclick={() => exportDialog.showModal()}
    aria-haspopup="dialog"
  >
    <Icon name="down" size={16} />{t("导出", "Export")}
  </button>
</div>
{#if error}<div class="inline-feedback" role="alert">
    {error}{#if current && drafts[current.id] !== undefined}<button
        onclick={() => saveNote(current!.id, drafts[current!.id])}
        >{t("重试保存笔记", "Retry saving note")}</button
      >{/if}
  </div>{/if}
{#if notice}<p role="status">{notice}</p>{/if}
<div class="vocabulary-workspace">
  <aside class="vocabulary-list-tray">
    <div class="vocabulary-list-core">
      <div class="section-heading">
        <h2>{t("我的生词", "My words")}</h2>
        <span class="count-tag">{filtered.length}</span>
      </div>
      <div class="vocabulary-list-scroll">
        {#each filtered as item}<button
            class="vocabulary-item"
            aria-pressed={selected === item.id}
            onclick={() => (selected = item.id)}
            ><span
              class="word-initial"
              class:mastered={item.status === "mastered"}
              >{item.word.slice(0, 1).toUpperCase()}</span
            ><span class="word-list-copy"
              ><strong>{item.word}</strong><small
                >{item.pos}
                {item.contexts[0]?.meanings?.[language]?.contextual ||
                  item.contexts[0]?.meanings?.[language]?.general ||
                  t("待补充释义", "Add a meaning")}</small
              ><span class="count-tag"
                >{t(
                  `遇到 ${item.review_count} 次`,
                  `${item.review_count} encounters`,
                )}</span
              ></span
            ></button
          >{/each}
        {#if !filtered.length}<div class="word-list-empty">
            <Icon name="book" size={28} />
            <p>
              {loading
                ? t("正在载入…", "Loading…")
                : query || status !== "all"
                  ? t("没有匹配的生词", "No matching words")
                  : t(
                      "翻译后收录生词，或在原文中选词加入。",
                      "Add words after translating, or select a word in the source.",
                    )}
            </p>
          </div>{/if}
      </div>
    </div>
  </aside>
  <section class="vocabulary-detail-tray">
    <div class="vocabulary-detail-core">
      {#if current}
        <header class="word-detail-header">
          <div>
            <span class="difficulty-tag"
              >{current.difficulty || t("自选词", "Selected word")}</span
            >
            <h2>{current.word}</h2>
            <div class="word-pronunciation">
              <span>{current.phonetic}</span><button
                class="icon-button"
                aria-label={t("朗读单词", "Pronounce word")}
                onclick={() => speak(current!.word)}
                ><Icon name="speaker" size={18} /></button
              ><span>{current.pos}</span>
            </div>
          </div>
          <button
            disabled={busy}
            onclick={() =>
              update({
                id: current!.id,
                status:
                  current!.status === "mastered" ? "learning" : "mastered",
              })}
            >{current.status === "mastered"
              ? t("标记为学习中", "Mark as learning")
              : t("标记为已掌握", "Mark as mastered")}</button
          >
        </header>
        <div class="word-meanings">
          {#if current.contexts[0]}{@const context = current.contexts[0]}
            <WordMeaning
              candidate={{ ...current, ...context }}
              sourceText={context.context_sentence}
              {language}
              {profileId}
              {allowModel}
              {locale}
            />
          {:else}<p>
              {t(
                "目标语言释义暂缺",
                "Meaning unavailable in the target language",
              )}
            </p>
            {#if current.dict_meaning}<details>
                <summary
                  >{t("本地词典原文", "Original dictionary entry")}</summary
                >
                <p>{current.dict_meaning}</p>
              </details>{/if}
          {/if}
        </div>
        <label class="word-note"
          >{t("个人笔记", "Personal note")}<textarea
            rows="3"
            maxlength="8000"
            aria-label={t("个人笔记", "Personal note")}
            value={drafts[current.id] ?? current.note}
            oninput={(e) => saveNote(current!.id, e.currentTarget.value)}
            placeholder={t(
              "记下你的联想、记忆法或补充释义…",
              "Your associations, memory cues or meanings…",
            )}></textarea><small class="subtle" role="status"
            >{noteSaving
              ? t("正在保存…", "Saving…")
              : error
                ? t("保存未确认", "Save not confirmed")
                : t("自动保存", "Saved automatically")}</small
          ></label
        >
        <section class="word-contexts">
          <div class="section-heading">
            <h3>{t("语境例句库", "Contexts")}</h3>
            <span class="count-tag">{current.contexts.length}</span>
          </div>
          {#each current.contexts as context}{@const parts = highlight(
              context.context_sentence,
              context.surface,
            )}
            <article class="word-context">
              <div class="context-meta">
                <time
                  >{new Date(context.created_at).toLocaleDateString(
                    locale === "zh" ? "zh-CN" : "en-US",
                  )}</time
                ><button
                  class="icon-button"
                  aria-label={t("删除例句", "Delete context")}
                  onclick={() => askDelete(current!.id, context.id)}
                  ><Icon name="trash" size={15} /></button
                >
              </div>
              <p class="context-source">
                {parts.before}{#if parts.match}<mark>{parts.match}</mark
                  >{/if}{parts.after}
              </p>
              {#if context.meanings?.[language]?.contextual}<p
                  class="context-meaning"
                >
                  {context.meanings[language].contextual}
                </p>{/if}
              {#if context.meanings?.[language]?.sentence_translation}<p
                  class="context-translation"
                >
                  {context.meanings[language].sentence_translation}
                </p>{/if}
              {#if !context.meanings?.[language] && (context.context_meaning || context.sentence_trans)}<details
                >
                  <summary
                    >{t("原有语境记录", "Original saved context")}</summary
                  >
                  <p>{context.context_meaning}</p>
                  <p>{context.sentence_trans}</p>
                </details>{/if}
            </article>{/each}
        </section>
        <button class="word-delete" onclick={() => askDelete(current!.id)}
          >{t("删除生词", "Delete word")}</button
        >
      {:else}<div class="vocabulary-empty">
          <span class="guard-emblem"><Icon name="book" size={34} /></span>
          <h2>{t("每个词，都有它的语境", "Every word has a context")}</h2>
          <p>
            {t(
              "选择一个生词，回看它在不同句子中的含义。",
              "Choose a word to revisit its meanings across sentences.",
            )}
          </p>
        </div>{/if}
    </div>
  </section>
</div>
<dialog
  bind:this={dialog!}
  class="vocabulary-delete-dialog"
  oncancel={() => (pendingDelete = null)}
>
  <h2>
    {pendingDelete?.context
      ? t("删除这条例句？", "Delete this context?")
      : t("删除这个生词？", "Delete this word?")}
  </h2>
  <p>{t("删除后无法恢复。", "This cannot be undone.")}</p>
  <div class="guard-actions">
    <button
      disabled={busy}
      onclick={() => {
        dialog.close();
        pendingDelete = null;
      }}>{t("取消", "Cancel")}</button
    ><button disabled={busy} onclick={remove}>{t("删除", "Delete")}</button>
  </div>
</dialog>

<dialog
  bind:this={exportDialog!}
  class="vocabulary-export-dialog"
  aria-labelledby="export-title"
>
  <header>
    <h2 id="export-title">{t("导出生词本", "Export vocabulary")}</h2>
    <button
      class="icon-button"
      aria-label={t("关闭", "Close")}
      onclick={() => exportDialog.close()}
      ><Icon name="close" size={18} /></button
    >
  </header>
  <div class="export-format-options">
    {#each [{ id: "csv", name: "CSV", detail: t("电子表格", "Spreadsheets") }, { id: "markdown", name: "Markdown", detail: t("笔记文档", "Notes and documents") }, { id: "anki", name: "Anki TSV", detail: t("记忆卡片", "Flashcards") }] as option}
      <button
        onclick={() => exportBook(option.id as "csv" | "markdown" | "anki")}
      >
        <span><strong>{option.name}</strong><small>{option.detail}</small></span
        ><Icon name="arrow" size={17} />
      </button>
    {/each}
  </div>
</dialog>
