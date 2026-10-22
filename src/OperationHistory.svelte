<script lang="ts">
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { errorText, statusName, type Locale } from "./locale";
  import type { OperationRecord } from "./types";
  import OperationResult from "./OperationResult.svelte";
  let {
    locale,
    onOpenSource,
    revision = 0,
  }: {
    locale: Locale;
    onOpenSource: (id: string) => void;
    revision?: number;
  } = $props();
  const t = (zh: string, en: string) => (locale === "zh" ? zh : en);
  let opened = $state(false),
    busy = $state(false),
    error = $state(""),
    records = $state<OperationRecord[]>([]);
  let requestSequence = 0;
  let expanded = $state<Record<string,boolean>>({});
  function merge(record: OperationRecord) {
    const previous = records.find((item) => item.id === record.id);
    if (previous && previous.revision > record.revision) return;
    records = [...records.filter((item) => item.id !== record.id), record]
      .sort((a, b) => b.updated_at - a.updated_at)
      .slice(0, 40);
  }
  async function refresh() {
    if (!isTauri()) return;
    const ticket = ++requestSequence;
    busy = true;
    error = "";
    try {
      const list = await invoke<OperationRecord[]>("operation_list", {
        taskId: null,
      });
      if (ticket !== requestSequence) return;
      const newer = records.filter((record) =>
        list?.some(
          (item) => item.id === record.id && record.revision > item.revision,
        ),
      );
      records = list ?? [];
      newer.forEach(merge);
    } catch (e) {
      if (ticket === requestSequence) error = errorText(e, locale);
    } finally {
      if (ticket === requestSequence) busy = false;
    }
  }
  $effect(() => {
    void revision;
    if (opened) void refresh();
  });
  async function stop(id: string) {
    try {
      await invoke("cancel_operation", { id });
    } catch (e) {
      error = errorText(e, locale);
    }
  }
  onMount(() => {
    let disposed = false,
      off: (() => void) | undefined;
    listen<OperationRecord>("operation-changed", (e) => {
      if (opened) merge(e.payload);
    }).then((f) => {
      if (disposed) f();
      else off = f;
    });
    return () => {
      disposed = true;
      requestSequence++;
      off?.();
    };
  });
</script>

<details
  class="operation-history"
  ontoggle={(e) => {
    opened = e.currentTarget.open;
  }}
>
  <summary>{t("解析与回复历史", "Explanation and reply history")}</summary>
  <p class="workflow-note">
    {t(
      "最多保留 40 个操作，合计 32 MiB；每个操作保留最近 5 次历史尝试。打开或复制不会重新请求模型。",
      "Keeps up to 40 operations within 32 MiB and five previous attempts each. Opening or copying never sends a new request.",
    )}
  </p>
  {#if busy}<p role="status">
      {t("正在读取本地历史…", "Loading local history…")}
    </p>{/if}{#if error}<p class="inline-feedback" role="alert">{error}</p>{/if}
  {#if !busy && !records.length}<p class="workflow-note">
      {t("尚无已保存的解析或回复。", "No saved explanations or replies yet.")}
    </p>{/if}
  <div class="operation-history-list">
    {#each records as operation (operation.id)}<details
        class="operation-history-record"
        ontoggle={(event) => { expanded = { ...expanded, [operation.id]: event.currentTarget.open }; }}
      >
        <summary
          ><span
            >{operation.kind === "reply"
              ? t("回复", "Reply")
              : t("语境解析", "Explanation")} · {operation.profile.name}</span
          ><small
            >{new Date(operation.created_at).toLocaleString(
              locale === "zh" ? "zh-CN" : "en-US",
            )} · v{operation.source_revision} · {statusName(
              operation.status,
              locale,
            )}</small
          ></summary
        >{#if opened && expanded[operation.id]}{#if operation.task_id}<button
            class="history-operation-source"
            onclick={() => onOpenSource(operation.task_id!)}
            >{t("查看关联原文", "View associated source")}</button
          >{/if}{#if operation.status === "blocked"}<p class="workflow-note">
            {t(
              "此操作需要在原始草稿中明确确认敏感内容。",
              "Confirm sensitive content explicitly in the original draft to continue.",
            )}
          </p>{:else}<OperationResult
            {operation}
            {locale}
            onResolve={() => {}}
            onStop={() => stop(operation.id)}
          />{/if}{/if}
      </details>{/each}
  </div>
</details>
