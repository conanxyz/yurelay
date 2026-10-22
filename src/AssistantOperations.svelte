<script lang="ts">
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount, tick } from "svelte";
  import { errorText, type Locale } from "./locale";
  import type {
    OperationRecord,
    OperationRequest,
    Settings,
    Task,
  } from "./types";
  import ContextExplanation from "./ContextExplanation.svelte";
  import ReplyDrawer from "./ReplyDrawer.svelte";
  import OperationResult from "./OperationResult.svelte";
  import ServicePicker from "./ServicePicker.svelte";
  import Icon from "./Icons.svelte";
  import { panelEntrance } from "./motion";
  let {
    locale,
    task,
    sourceText,
    sourceLanguage,
    targetLanguage,
    detectedLanguage,
    settings,
    profileId,
    selection,
    onProfileChange,
  }: {
    locale: Locale;
    task?: Task;
    sourceText: string;
    sourceLanguage: string;
    targetLanguage: string;
    detectedLanguage?: string | null;
    settings: Settings;
    profileId: string;
    selection: { start: number; end: number } | null;
    onProfileChange: (id: string) => void;
  } = $props();
  const t = (zh: string, en: string) => (locale === "zh" ? zh : en);
  let open = $state<"explain" | "reply" | null>(null),
    records = $state<OperationRecord[]>([]),
    localIds = $state<string[]>([]),
    error = $state(""),
    submitting = $state(false);
  let intent = $state(""),
    scope = $state<"paragraph" | "full">("paragraph");
  const replyLanguage = $derived.by(() => {
    const unchanged =
      sourceText === (task?.edited_text ?? task?.original_text ?? "");
    const language =
      sourceLanguage ||
      detectedLanguage ||
      (unchanged ? task?.detected_language : "") ||
      "";
    return ["und", "mul", "unknown", "auto"].includes(language.toLowerCase())
      ? ""
      : language;
  });
  const referenceLanguage = $derived(
    targetLanguage || settings.target_language,
  );
  let panel = $state<HTMLElement>(),
    opener: HTMLButtonElement | null = null;
  const snapshots = new Map<string, string>();
  let sourceIdentity = "";
  const activeRecords = $derived(
    records.filter((r) =>
      task
        ? r.task_id === task.id
        : localIds.includes(r.id) && r.task_id === null,
    ),
  );
  const operation = $derived(
    activeRecords
      .filter((r) => r.kind === open)
      .sort((a, b) => b.updated_at - a.updated_at)[0],
  );
  const operationBusy = $derived(
    !!operation &&
      ["queued", "generating", "blocked"].includes(operation.status),
  );
  const selectedText = $derived(
    selection ? sourceText.slice(selection.start, selection.end) : "",
  );
  const compatible = $derived(
    settings.profiles.find(
      (p) =>
        p.id === profileId &&
        !["system_dictionary", "youdao"].includes(p.provider),
    ),
  );
  const signature = () =>
    JSON.stringify([
      open,
      sourceText,
      sourceLanguage,
      profileId,
      referenceLanguage,
      open === "explain" ? [scope, selection] : [intent, replyLanguage],
    ]);
  const stale = $derived(
    !!operation &&
      ((!!task && operation.source_revision !== (task.source_revision ?? 0)) ||
        (snapshots.has(operation.request_id) &&
          snapshots.get(operation.request_id) !== signature()) ||
        sourceText !==
          (task?.edited_text ?? task?.original_text ?? sourceText)),
  );
  function merge(record: OperationRecord) {
    if (!record?.id) return;
    const old = records.find((r) => r.id === record.id);
    if (old && old.revision > record.revision) return;
    records = [...records.filter((r) => r.id !== record.id), record];
  }
  $effect(() => {
    const id = task?.id ?? "";
    if (sourceIdentity !== id) {
      sourceIdentity = id;
      intent = "";
      open = null;
      localIds = [];
    }
    if (!isTauri() || !id) return;
    let cancelled = false;
    invoke<OperationRecord[]>("operation_list", { taskId: id })
      .then((items) => {
        if (!cancelled) (items ?? []).forEach(merge);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  });
  onMount(() => {
    let disposed = false,
      off: (() => void) | undefined;
    listen<OperationRecord>("operation-changed", (e) => merge(e.payload)).then(
      (f) => {
        if (disposed) f();
        else off = f;
      },
    );
    return () => {
      disposed = true;
      off?.();
    };
  });
  async function show(kind: "explain" | "reply", event: MouseEvent) {
    opener = event.currentTarget as HTMLButtonElement;
    open = open === kind ? null : kind;
    error = "";
    if (open) {
      await tick();
      panel?.focus({ preventScroll: true });
    }
  }
  function close() {
    open = null;
    opener?.focus({ preventScroll: true });
  }
  async function submit() {
    if (
      !open ||
      submitting ||
      operationBusy ||
      !compatible ||
      !sourceText.trim()
    )
      return;
    if (open === "reply" && !replyLanguage) {
      error = t(
        "请先在原文上方选择语言，再生成回复。",
        "Choose the source language above before generating a reply.",
      );
      return;
    }
    submitting = true;
    error = "";
    const request: OperationRequest = {
      request_id: crypto.randomUUID(),
      previous_operation_id: operation?.id ?? null,
      kind: open,
      source_text: sourceText,
      task_id: task?.id ?? null,
      source_revision: task?.source_revision ?? null,
      profile_id: profileId,
      source_language:
        sourceLanguage || detectedLanguage || replyLanguage || null,
      target_language: referenceLanguage || settings.target_language,
      selection_start: selection?.start,
      selection_end: selection?.end,
      context_scope: scope,
      intent,
      tone: "default",
      reply_language: replyLanguage || null,
      include_back_translation: true,
    };
    const snapshot = signature();
    try {
      const r = await invoke<OperationRecord>("submit_operation", { request });
      snapshots.set(request.request_id, snapshot);
      localIds = [...localIds, r.id];
      merge(r);
    } catch (e) {
      error = errorText(e, locale);
    } finally {
      submitting = false;
    }
  }
  async function resolve(allow: boolean) {
    if (!operation) return;
    try {
      merge(
        await invoke<OperationRecord>("resolve_operation_sensitive", {
          id: operation.id,
          allow,
        }),
      );
    } catch (e) {
      error = errorText(e, locale);
    }
  }
  async function stop() {
    if (!operation) return;
    try {
      await invoke("cancel_operation", { id: operation.id });
    } catch (e) {
      error = errorText(e, locale);
    }
  }
</script>

<div class="assistant-entrypoints">
  <button
    disabled={!selection || !!task?.sensitive}
    aria-expanded={open === "explain"}
    onclick={(e) => show("explain", e)}
    >{t("语境解析", "Explain context")}</button
  ><button
    disabled={!sourceText.trim() || !!task?.sensitive}
    aria-expanded={open === "reply"}
    onclick={(e) => show("reply", e)}>{t("构思回复", "Compose reply")}</button
  >{#if !open && activeRecords.some( (r) => ["queued", "generating"].includes(r.status) )}<small
      role="status"
      >{t("独立任务正在生成", "An assistant task is generating")}</small
    >{/if}
</div>
{#if open}<div
    class="workflow-tray assistant-drawer"
    use:panelEntrance
    bind:this={panel!}
    tabindex="-1"
    role="dialog"
    aria-modal="false"
    aria-label={open === "reply"
      ? t("回复抽屉", "Reply drawer")
      : t("语境解析", "Context explanation")}
    onkeydown={(e) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        close();
      }
    }}
  >
    <div class="workflow-core">
      <header>
        <h2>
          {open === "reply"
            ? t("构思回复", "Compose reply")
            : t("语境解析", "Explain context")}
        </h2>
        <button
          class="icon-button"
          aria-label={t("关闭辅助面板", "Close assistant panel")}
          onclick={close}><Icon name="close" size={15} /></button
        >
      </header>
      {#if open === "explain"}<ServicePicker
          profiles={settings.profiles}
          selected={profileId}
          {locale}
          llmOnly
          onChange={onProfileChange}
        />{/if}
      {#if !compatible}<p class="workflow-note">
          {t(
            "选择支持生成的模型服务后继续。",
            "Choose a model service that supports generation.",
          )}
        </p>{/if}
      {#if open === "explain"}<ContextExplanation
          {locale}
          selection={selectedText}
          bind:scope
          busy={submitting || operationBusy || !compatible}
          onGenerate={submit}
        />{:else}<ReplyDrawer
          {locale}
          bind:intent
          busy={submitting || operationBusy || !compatible}
          hasOutput={!!operation}
          onGenerate={submit}
        />{/if}
      {#if error}<p class="inline-feedback" role="alert">{error}</p>{/if}
      {#if operation}<OperationResult
          {operation}
          {locale}
          {stale}
          onResolve={resolve}
          onStop={stop}
        />{/if}
    </div>
  </div>{/if}
