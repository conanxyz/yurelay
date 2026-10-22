<script lang="ts">
  import { onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { active, type Task } from "./types";
  import { errorText, type Locale } from "./locale";
  let {
    task,
    draft,
    sourceLanguage,
    targetLanguage,
    locale,
    onUpdate,
  }: {
    task: Task;
    draft: string;
    sourceLanguage: string;
    targetLanguage: string;
    locale: Locale;
    onUpdate: (task: Task) => void;
  } = $props();
  const t = (zh: string, en: string) => (locale === "zh" ? zh : en);
  let append = $state(false),
    busy = $state(false),
    error = $state("");
  let identity = "",
    boundTaskId = "";
  let appendPaused = $state(false);
  const disabled = $derived(active(task) || busy || !!task.sensitive);
  const dirty = $derived(
    draft !== (task.edited_text ?? task.original_text) ||
      sourceLanguage !== (task.source_language ?? "") ||
      targetLanguage !== task.target_language,
  );
  $effect(() => {
    const id = task.id,
      rev = task.source_revision ?? 0;
    const key = `${id}:${rev}`;
    if (boundTaskId !== id) {
      boundTaskId = id;
      identity = key;
      append = false;
      appendPaused = false;
      void invoke("set_append_mode", { enabled: false }).catch(() => {});
      return;
    }
    if (identity !== key) {
      identity = key;
      if (append)
        invoke("set_append_mode", {
          enabled: true,
          taskId: id,
          expectedRevision: rev,
        }).catch(() => {});
    }
  });
  $effect(() => {
    if (append && dirty && !busy) {
      append = false;
      appendPaused = true;
      void invoke("set_append_mode", { enabled: false }).catch(() => {});
    }
  });
  async function run(fn: () => Promise<void>) {
    if (busy) return;
    busy = true;
    error = "";
    try {
      await fn();
    } catch (e) {
      error = errorText(e, locale);
    } finally {
      busy = false;
    }
  }
  async function checkpoint() {
    if (!dirty) return task;
    const requestedId = task.id;
    const updated = await invoke<Task>("source_update", {
      request: {
        task_id: task.id,
        expected_revision: task.source_revision ?? 0,
        text: draft,
        source_language: sourceLanguage,
        target_language: targetLanguage,
      },
    });
    onUpdate(updated);
    if (task.id !== requestedId)
      throw new Error(
        t(
          "原文已变化，请重新操作。",
          "The source changed. Try the action again.",
        ),
      );
    return updated;
  }
  onDestroy(() => {
    void invoke("set_append_mode", { enabled: false }).catch(() => {});
  });
  const request = (value: Task) => ({
    task_id: value.id,
    expected_revision: value.source_revision ?? 0,
  });
  async function toggleAppend() {
    await run(async () => {
      const current = await checkpoint();
      await invoke("set_append_mode", {
        enabled: !append,
        taskId: current.id,
        expectedRevision: current.source_revision ?? 0,
      });
      if (task.id !== current.id) {
        append = false;
        await invoke("set_append_mode", { enabled: false });
        return;
      }
      append = !append;
      appendPaused = false;
    });
  }
  async function undoCleanup() {
    await run(async () => {
      onUpdate(
        await invoke<Task>("source_undo_cleanup", { request: request(task) }),
      );
    });
  }
</script>

<div class="source-workflow-toolbar">
  <button
    aria-pressed={append}
    {disabled}
    title={t(
      "把后续划词或截图加入当前原文",
      "Append subsequent selection or screenshot captures to this source",
    )}
    onclick={toggleAppend}
    >{append ? t("追加中", "Appending") : t("追加", "Append")}</button
  >
  {#if task.can_undo_cleanup}<button
      class="undo-cleanup"
      disabled={disabled || dirty}
      onclick={undoCleanup}>{t("撤销整理", "Undo cleanup")}</button
    >{/if}
  {#if append}<span class="append-hint"
      >{t("下次捕获加入此处", "Next capture joins this source")}</span
    >{/if}
</div>
{#if appendPaused}<p class="workflow-note" role="status">
    {t(
      "编辑后追加已暂停；重新开启会先应用当前编辑。",
      "Append paused after editing. Turn it on again to apply the current edits.",
    )}
  </p>{/if}
{#if error}<p class="inline-feedback" role="alert">{error}</p>{/if}
