<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { imageEntrance, historyEntrance } from "./motion";
  import { invoke, isTauri, convertFileSrc } from "@tauri-apps/api/core";
  import { active, type Task } from "./types";
  import {
    text,
    statusName,
    errorText,
    languageName,
    type Locale,
  } from "./locale";
  import Icon from "./Icons.svelte";
  import TranslationContent from "./TranslationContent.svelte";
  import { translationPlainText } from "./reading";
  let { locale }: { locale: Locale } = $props();
  const t = (zh: string, en: string) => text(locale, zh, en);
  let records = $state<Task[]>([]),
    current = $state<Task | null>(null),
    query = $state(""),
    source = $state(""),
    loading = $state(true),
    detailLoading = $state(false),
    more = $state(false),
    error = $state(""),
    confirming = $state(false);
  let deleteDialog: HTMLDialogElement;
  const day = (task: Task) =>
    new Date(task.created_at).toLocaleDateString(
      locale === "zh" ? "zh-CN" : "en-US",
      { month: "long", day: "numeric" },
    );
  const time = (task: Task) =>
    new Date(task.created_at).toLocaleTimeString(
      locale === "zh" ? "zh-CN" : "en-US",
      { hour: "2-digit", minute: "2-digit" },
    );
  const sourceName = (task: Task) =>
    task.source === "screenshot"
      ? t("截图", "Screenshot")
      : task.source === "manual"
        ? t("手动输入", "Typed text")
        : t("划词", "Selection");
  function confirmDelete() {
    confirming = true;
    deleteDialog.showModal();
  }
  function cancelDelete() {
    confirming = false;
    deleteDialog.close();
  }
  let generation = 0;
  let detailGeneration = 0;
  let disposed = false;
  async function load(reset = true) {
    const ticket = ++generation;
    loading = true;
    error = "";
    try {
      const rows = isTauri()
        ? await invoke<Task[]>("search_history", {
            query,
            source,
            offset: reset ? 0 : records.length,
          })
        : [];
      if (disposed || ticket !== generation) return;
      records = reset ? rows : [...records, ...rows];
      more = rows.length === 30;
    } catch (e) {
      if (ticket === generation) error = errorText(e, locale);
    } finally {
      if (ticket === generation) loading = false;
    }
  }
  async function choose(id: string) {
    const ticket = ++detailGeneration;
    detailLoading = true;
    confirming = false;
    deleteDialog?.close();
    error = "";
    try {
      const task = await invoke<Task>("history_detail", { id });
      if (ticket === detailGeneration && !disposed) current = task;
    } catch (e) {
      if (ticket === detailGeneration) {
        current = null;
        error = errorText(e, locale);
      }
    } finally {
      if (ticket === detailGeneration) detailLoading = false;
    }
  }
  async function remove() {
    if (!current) return;
    try {
      await invoke("remove_task", { id: current.id });
      current = null;
      confirming = false;
      deleteDialog.close();
      await load();
    } catch (e) {
      error = errorText(e, locale);
    }
  }
  async function command(name: string, args: Record<string, unknown>) {
    try {
      await invoke(name, args);
    } catch (e) {
      error = errorText(e, locale);
    }
  }
  onMount(() => {
    load();
    const refresh = () => {
      load();
      if (current) choose(current.id);
    };
    window.addEventListener("focus", refresh);
    const watcher = isTauri() ? listen("settings-focused", refresh) : null;
    return () => {
      window.removeEventListener("focus", refresh);
      watcher?.then((off) => off());
      disposed = true;
      generation++;
      detailGeneration++;
    };
  });
</script>

<div class="history-panel" use:historyEntrance>
  <form
    class="history-tools"
    onsubmit={(e) => {
      e.preventDefault();
      detailGeneration++;
      detailLoading = false;
      current = null;
      load();
    }}
  >
    <label class="history-search"
      >{t("搜索记录", "Search history")}<span class="history-search-field"
        ><Icon name="search" size={17} /><input
          type="search"
          bind:value={query}
          placeholder={t("搜索原文或译文", "Search source or translation")}
          maxlength="512"
        /></span
      ></label
    >
    <label
      >{t("来源", "Source")}<select bind:value={source}
        ><option value="">{t("全部来源", "All sources")}</option><option
          value="selection">{t("划词", "Selection")}</option
        ><option value="screenshot">{t("截图", "Screenshot")}</option><option
          value="manual">{t("手动输入", "Typed text")}</option
        ></select
      ></label
    >
    <button class="history-search-button" type="submit" disabled={loading}
      >{t("搜索", "Search")}<span class="button-island" aria-hidden="true"
        ><Icon name="arrow" size={15} /></span
      ></button
    >
  </form>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <div class="history-workspace">
    <div class="history-tray">
      <section class="history-list" aria-label={t("记录列表", "History list")}>
        <div class="history-list-heading">
          <h2>{t("最近记录", "Recent records")}</h2>
          <span>{records.length}{more ? "+" : ""}</span>
        </div>
        {#if loading && !records.length}<div
            class="history-loading"
            aria-busy="true"
            aria-label={t("正在加载记录", "Loading history")}
          >
            <div class="pref-skeleton"></div>
            <div class="pref-skeleton"></div>
            <div class="pref-skeleton"></div>
          </div>
        {:else if !records.length}<div class="history-empty">
            <span class="history-empty-symbol"
              ><Icon name="history" size={26} /></span
            >
            <h3>
              {query || source
                ? t("没有匹配的记录", "No matching records")
                : t("还没有记录", "No records yet")}
            </h3>
            <p>
              {query || source
                ? t(
                    "换个关键词或来源再试一次。",
                    "Try another keyword or source.",
                  )
                : t(
                    "划词或截图后，记录会显示在这里。",
                    "Capture text to start your history.",
                  )}
            </p>
          </div>
        {:else}<ul class="history-records">
            {#each records as task, index (task.id)}
              {#if index === 0 || day(task) !== day(records[index - 1])}<li
                  class="history-day"
                >
                  {day(task)}
                </li>{/if}
              <li>
                <button
                  class="history-row"
                  aria-pressed={current?.id === task.id}
                  onclick={() => choose(task.id)}
                >
                  <span class="history-row-meta"
                    ><span
                      ><Icon
                        name={task.source === "screenshot"
                          ? "image"
                          : "translate"}
                        size={14}
                      />{sourceName(task)}</span
                    ><time datetime={new Date(task.created_at).toISOString()}
                      >{time(task)}</time
                    ></span
                  >
                  <span class="history-row-text"
                    >{task.original_text ||
                      t("未取得原文", "No source text")}</span
                  >
                  <span class="history-row-bottom"
                    ><span
                      class="history-status"
                      class:failed={task.status === "failed"}
                      class:complete={task.status === "completed"}
                      >{statusName(task.status, locale)}</span
                    ><Icon name="chevron" size={13} /></span
                  >
                </button>
              </li>
            {/each}
          </ul>{/if}
        {#if more}<button
            class="history-more"
            disabled={loading}
            onclick={() => load(false)}
            >{loading
              ? t("正在加载…", "Loading…")
              : t("加载更多", "Load more")}</button
          >{/if}
      </section>
    </div>
    <div class="history-tray detail-tray">
      <section
        class="history-detail"
        aria-label={t("记录详情", "Record details")}
      >
        {#if detailLoading}<div class="history-loading" aria-busy="true">
            <div class="pref-skeleton"></div>
            <div class="pref-skeleton"></div>
          </div>
        {:else if current}
          <header class="history-detail-heading">
            <div>
              <span class="history-detail-date"
                >{day(current)} · {time(current)}</span
              >
              <h2>{t("记录详情", "Record details")}</h2>
            </div>
            <button
              class="history-open"
              onclick={() => command("show_task", { id: current!.id })}
              >{t("打开翻译", "Open translation")}<span
                class="button-island"
                aria-hidden="true"><Icon name="arrow" size={15} /></span
              ></button
            >
          </header>
          <div class="history-context">
            <span>{sourceName(current)}</span><span
              >{#if current.profile.provider === "system_dictionary"}{t(
                  "本机词典",
                  "Local dictionary",
                )}{:else}{languageName(
                  current.source_language || current.detected_language || "und",
                  locale,
                )}<Icon name="arrow" size={12} />{languageName(
                  current.target_language,
                  locale,
                )}{/if}</span
            >
          </div>
          {#if current.has_image}<details class="history-image">
              <summary>{t("查看截图", "View screenshot")}</summary><img
                use:imageEntrance
                src={convertFileSrc(current.id, "glossa-image")}
                alt={t("保存的截图选区", "Saved screenshot selection")}
              />
            </details>{/if}
          <section class="history-content-section">
            <h3>{t("原文", "Source")}</h3>
            <p class="history-text">
              {current.original_text ||
                t("没有识别到文字", "No text was recognized")}
            </p>
          </section>
          {#if current.edited_text && current.edited_text !== current.original_text}<section
              class="history-content-section"
            >
              <h3>{t("修正后的原文", "Edited source")}</h3>
              <p class="history-text">{current.edited_text}</p>
            </section>{/if}
          <section class="history-content-section detail-translation">
            <h3>
              {current.profile.provider === "system_dictionary"
                ? t("释义", "Definition")
                : t("译文", "Translation")}
            </h3>
            {#if current.translation}<TranslationContent
                text={current.translation}
                {locale}
                identity={`${current.id}:${current.attempt}`}
                streaming={active(current)}
                plain={current.profile.provider === "system_dictionary"}
              />{:else}<p class="history-text">
                {t("暂无译文", "No translation yet")}
              </p>{/if}
          </section>
          {#if current.error}<p class="error history-detail-error" role="alert">
              {errorText(current.error, locale)}
            </p>{/if}
          {#if current.past_attempts.length}<details class="history-attempts">
              <summary
                >{t("之前的翻译", "Previous translations")}<span
                  >{current.past_attempts.length}</span
                ></summary
              >{#each current.past_attempts as attempt}<div
                  class="history-attempt"
                >
                  <span
                    >#{attempt.number} · {statusName(
                      attempt.status,
                      locale,
                    )}</span
                  >
                  {#if attempt.translation}<TranslationContent
                      text={attempt.translation}
                      {locale}
                      identity={`${current.id}:past:${attempt.number}`}
                    />{:else}<p class="history-text">
                      {t("无译文", "No translation")}
                    </p>{/if}
                </div>{/each}
            </details>{/if}
          <footer class="history-detail-actions">
            <div>
              <button
                disabled={!current.original_text}
                onclick={() =>
                  command("copy_text", { text: current!.original_text })}
                >{t("复制原文", "Copy source")}</button
              ><button
                disabled={!current.translation}
                onclick={() =>
                  command("copy_text", {
                    text:
                      current!.profile.provider === "system_dictionary"
                        ? current!.translation
                        : translationPlainText(current!.translation),
                  })}>{t("复制译文", "Copy translation")}</button
              >
            </div>
            <div>
              {#if active(current)}<button
                  onclick={async () => {
                    await command("stop_task", { id: current!.id });
                    await choose(current!.id);
                  }}>{t("停止翻译", "Stop translation")}</button
                >{:else}<button
                  onclick={async () => {
                    await command("retry_task", {
                      id: current!.id,
                      editedText: null,
                    });
                    await command("show_task", { id: current!.id });
                  }}>{t("重新翻译", "Translate again")}</button
                >{/if}<button class="history-delete" onclick={confirmDelete}
                >{t("删除记录", "Delete record")}</button
              >
            </div>
          </footer>
        {:else}<div class="history-detail-empty">
            <span class="history-empty-symbol"
              ><Icon name="history" size={32} /></span
            >
            <h2>{t("选择一条记录", "Choose a record")}</h2>
            <p>
              {t(
                "选择记录，查看原文与译文。",
                "Select a record to read its source and translation.",
              )}
            </p>
          </div>{/if}
      </section>
    </div>
  </div>
</div>
<dialog
  bind:this={deleteDialog!}
  class="history-delete-dialog"
  aria-labelledby="delete-record-title"
  oncancel={(e) => {
    e.preventDefault();
    cancelDelete();
  }}
>
  <div class="close-dialog-core">
    <h2 id="delete-record-title">
      {t("删除这条记录？", "Delete this record?")}
    </h2>
    <p>
      {t(
        "原文、截图和译文将一并删除，无法撤销。",
        "Its source, screenshot and translation will be permanently removed.",
      )}
    </p>
    {#if error}<p class="dialog-error" role="alert">{error}</p>{/if}
    <div class="close-dialog-actions">
      <button onclick={cancelDelete}>{t("取消", "Cancel")}</button><button
        class="danger"
        onclick={remove}>{t("确认删除", "Delete permanently")}</button
      >
    </div>
  </div>
</dialog>
