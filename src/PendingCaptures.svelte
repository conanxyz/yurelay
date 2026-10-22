<script lang="ts">
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { active, type Task } from "./types";
  import { errorText, type Locale } from "./locale";
  let {
    task,
    draft,
    locale,
    onUpdate,
  }: {
    task?: Task;
    draft: string;
    locale: Locale;
    onUpdate: (value: Task, choose?: boolean) => void;
  } = $props();
  type Pending = {
    id: string;
    task_id: string;
    expected_revision: number;
    sensitive?: string;
    reason: "sensitive" | "stale";
  };
  const t = (zh: string, en: string) => (locale === "zh" ? zh : en);
  let pending = $state<Pending[]>([]),
    busy = $state(false),
    error = $state(""),
    refreshVersion = $state(0);
  let sequence = 0;
  $effect(() => {
    const id = task?.id ?? null;
    void task?.revision;
    void refreshVersion;
    const ticket = ++sequence;
    if (!isTauri() || (task && active(task))) return;
    invoke<Pending[]>("source_pending", { taskId: id })
      .then((items) => {
        if (ticket === sequence) pending = items ?? [];
      })
      .catch(() => {});
    return () => {
      sequence++;
    };
  });
  onMount(() => {
    let disposed = false,
      off: (() => void) | undefined;
    listen("source-pending-changed", () => refreshVersion++).then((f) => {
      if (disposed) f();
      else off = f;
    });
    return () => {
      disposed = true;
      off?.();
      sequence++;
    };
  });
  async function resolve(item: Pending, action: "discard" | "append" | "new") {
    if (busy) return;
    busy = true;
    error = "";
    try {
      let target = task;
      if (
        action === "append" &&
        target &&
        draft !== (target.edited_text ?? target.original_text)
      ) {
        target = await invoke<Task>("source_update", {
          request: {
            task_id: target.id,
            expected_revision: target.source_revision ?? 0,
            text: draft,
          },
        });
        onUpdate(target);
      }
      const next = await invoke<Task | null>("resolve_source_pending", {
        id: item.id,
        action,
        taskId: action === "append" ? target?.id : null,
        expectedRevision:
          action === "append" ? (target?.source_revision ?? 0) : null,
        allowSensitive: action !== "discard" && item.reason === "sensitive",
      });
      if (next) onUpdate(next, action === "new");
      pending = pending.filter((p) => p.id !== item.id);
    } catch (e) {
      error = errorText(e, locale);
    } finally {
      busy = false;
    }
  }
</script>

{#each pending as item}<section
    class="workflow-tray pending-capture"
    aria-label={t("待确认捕获", "Pending capture")}
  >
    <div class="workflow-core" role="alert">
      <p>
        {item.reason === "sensitive"
          ? t(
              "新增内容触发本地敏感保护。当前安全原文已保留；允许后此内容仅保留在内存。",
              "New content triggered local sensitive protection. The existing safe source is retained. Allowed content remains in memory only.",
            )
          : t(
              "捕获期间接收原文已变化或不可用，请选择如何保留本次捕获。",
              "The receiving source changed or became unavailable during capture. Choose how to keep this capture.",
            )}
      </p>
      <div class="workflow-actions">
        <button disabled={busy} onclick={() => resolve(item, "discard")}
          >{t("丢弃本次捕获", "Discard capture")}</button
        ><button
          disabled={busy || !task || active(task) || !!task.sensitive}
          onclick={() => resolve(item, "append")}
          >{item.reason === "sensitive"
            ? t("仅本次允许并追加", "Allow this append only")
            : t("加入当前稿", "Add to current draft")}</button
        ><button disabled={busy} onclick={() => resolve(item, "new")}
          >{item.reason === "sensitive"
            ? t("允许本次内容作为新稿", "Allow this capture as a new draft")
            : t("作为新稿", "Keep as new draft")}</button
        >
      </div>
    </div>
  </section>{/each}
{#if error}<p class="inline-feedback" role="alert">{error}</p>{/if}
