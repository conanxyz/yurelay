<script lang="ts">
  import { onDestroy } from "svelte";
  import Icon from "./Icons.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import type { OperationRecord } from "./types";
  import { errorText, statusName, type Locale } from "./locale";
  import TranslationContent from "./TranslationContent.svelte";
  import { translationPlainText } from "./reading";
  let {
    operation,
    locale,
    stale = false,
    onResolve,
    onStop,
  }: {
    operation: OperationRecord;
    locale: Locale;
    stale?: boolean;
    onResolve: (allow: boolean) => void;
    onStop: () => void;
  } = $props();
  const t = (zh: string, en: string) => (locale === "zh" ? zh : en);
  const active = $derived(["queued", "generating"].includes(operation.status));
  const canCopy = $derived(
    !!operation.primary_text &&
      (!active || operation.back_translation_state === "pending"),
  );
  let error = $state("");
  let copied = $state(false),
    copying = $state(false);
  let feedbackTimer: ReturnType<typeof setTimeout> | undefined;
  let displayedCopySource = "",
    displayedCopyAttempt = "",
    disposed = false;
  $effect(() => {
    const attempt = `${operation.id}:${operation.attempt}`;
    const source = operation.primary_text;
    if (attempt === displayedCopyAttempt && source === displayedCopySource)
      return;
    displayedCopyAttempt = attempt;
    displayedCopySource = source;
    copied = false;
    error = "";
    clearTimeout(feedbackTimer);
  });
  onDestroy(() => {
    disposed = true;
    clearTimeout(feedbackTimer);
  });
  function operationError(value: string) {
    if (operation.kind === "reply") {
      if (value === "回复已生成，译文补全未成功。")
        return t(
          "回复原文已保留，译文补全未成功。可重新生成回复。",
          "The original reply is retained, but translation recovery failed. Regenerate the reply to try again.",
        );
      if (value === "回复已生成，未获得完整回译参考。")
        return t(
          "回复原文已生成，译文未完成。",
          "The reply is available, but its translation is incomplete.",
        );
      if (value === "回复已生成，回译参考因并发限制未继续。")
        return t(
          "回复原文已生成，译文因并发限制未继续。",
          "The reply is available. Its translation stopped at the concurrency limit.",
        );
      if (value === "回复已生成，回译参考未完成。")
        return t(
          "回复原文已生成，译文未完成。",
          "The reply is available. Its translation did not finish.",
        );
    }
    return errorText(value, locale);
  }
  async function copy() {
    if (!canCopy || copying) return;
    const id = operation.id,
      attempt = operation.attempt,
      source = operation.primary_text;
    copying = true;
    error = "";
    try {
      await invoke("copy_text", { text: translationPlainText(source) });
      if (
        !disposed &&
        operation.id === id &&
        operation.attempt === attempt &&
        operation.primary_text === source
      ) {
        copied = true;
        clearTimeout(feedbackTimer);
        feedbackTimer = setTimeout(() => (copied = false), 1800);
      }
    } catch (e) {
      if (!disposed && operation.id === id && operation.attempt === attempt)
        error = errorText(e, locale);
    } finally {
      copying = false;
    }
  }
</script>

<section
  class="operation-output"
  aria-label={operation.kind === "reply"
    ? t("生成的回复", "Generated reply")
    : t("语境解析结果", "Context explanation result")}
>
  {#if operation.memory_only}<p class="workflow-note">
      {t("仅保留在当前会话", "Kept only for this session")}
    </p>{/if}
  {#if stale}<p class="workflow-note stale-result">
      {t(
        "基于上一版原文或设置；重新生成前请核对。",
        "Based on a previous source or settings version. Review before regenerating.",
      )}
    </p>{/if}
  {#if operation.status === "blocked"}<div role="alert">
      <p>
        {t(
          "此请求包含疑似敏感信息，尚未发送。允许仅适用于本次操作。",
          "Possible sensitive content was detected. Nothing has been sent. Permission applies only to this operation.",
        )}
      </p>
      <div class="workflow-actions">
        <button onclick={() => onResolve(false)}
          >{t("取消操作", "Cancel operation")}</button
        ><button onclick={() => onResolve(true)}
          >{t("允许本次发送", "Allow this request")}</button
        >
      </div>
    </div>{/if}
  {#if operation.kind === "reply" && operation.status !== "blocked" && (operation.primary_text || active || operation.back_translation_state !== "none")}
    <div class="reply-result-card">
      <div class="reply-result-heading">
        <h3>{t("原文", "Original")}</h3>
        <button
          class="icon-button reply-copy"
          aria-label={t("复制回复", "Copy reply")}
          title={copied
            ? t("已复制原文", "Original copied")
            : t("复制回复原文", "Copy original reply")}
          disabled={!canCopy || copying}
          onclick={copy}
          ><Icon name={copied ? "check" : "copy"} size={16} /></button
        >
      </div>
      <div class="reply-original">
        {#if operation.primary_text}<TranslationContent
            text={operation.primary_text}
            {locale}
            controls={false}
            identity={`${operation.id}:${operation.attempt}`}
            streaming={active && operation.back_translation_state !== "pending"}
          />
        {:else}<p class="reply-pending" role="status">
            {operation.status === "queued"
              ? t("等待生成…", "Queued…")
              : t("正在生成原文…", "Generating original…")}
          </p>{/if}
      </div>
      <div class="reply-translation">
        <h3>{t("译文", "Translation")}</h3>
        {#if operation.back_translation}<TranslationContent
            text={operation.back_translation}
            {locale}
            controls={false}
            identity={`${operation.id}:${operation.attempt}:back`}
            streaming={operation.back_translation_state === "pending"}
          />
        {:else}<p class="reply-pending" role="status">
            {operation.back_translation_state === "pending"
              ? t("正在生成译文…", "Generating translation…")
              : active
                ? t("等待原文完成…", "Waiting for the original…")
                : t("暂无译文。", "No translation available.")}
          </p>{/if}
      </div>
      {#if copied}<span class="sr-only" role="status"
          >{t("已复制原文", "Original copied")}</span
        >{/if}
    </div>
  {:else if operation.kind !== "reply" && operation.primary_text}<TranslationContent
      text={operation.primary_text}
      {locale}
      identity={`${operation.id}:${operation.attempt}`}
      streaming={active}
    />{/if}
  {#if operation.primary_text && ["failed", "stopped"].includes(operation.status)}<p
      class="workflow-note"
    >
      {t("这是已保留的部分结果。", "This is the retained partial result.")}
    </p>{/if}
  {#if operation.error}<p class="inline-feedback" role="alert">
      {operationError(operation.error)}
    </p>{/if}
  {#if operation.save_error}<p class="inline-feedback" role="alert">
      {t(
        "结果保留在当前会话，但未能保存：",
        "The result remains in this session but could not be saved: ",
      )}{errorText(operation.save_error, locale)}
    </p>{/if}
  {#if operation.status === "stopped"}<p class="workflow-note">
      {t(
        "已停止；已生成的内容保留。",
        "Stopped. Generated content is retained.",
      )}
    </p>{/if}
  {#if active || (operation.kind !== "reply" && operation.primary_text)}
    <div class="workflow-actions">
      {#if active}<span role="status"
          >{operation.status === "queued"
            ? t("等待生成…", "Queued…")
            : t("正在生成…", "Generating…")}</span
        ><button onclick={onStop}
          >{operation.kind === "reply"
            ? t("停止回复", "Stop reply")
            : t("停止解析", "Stop explanation")}</button
        >{/if}
      {#if operation.kind !== "reply" && operation.primary_text}<button
          disabled={!canCopy || copying}
          onclick={copy}>{t("复制解析", "Copy explanation")}</button
        >{/if}
    </div>
  {/if}
  {#if operation.attempts?.length > 0}<details class="operation-attempts">
      <summary>{t("历史尝试", "Previous attempts")}</summary>
      <p class="workflow-note">
        {t(
          "本地最多保留 40 个操作，合计 32 MiB；每个操作保留最近 5 次历史尝试。",
          "Local history keeps up to 40 operations within 32 MiB, with five previous attempts per operation.",
        )}
      </p>
      {#each operation.attempts as attempt}<article>
          <small
            >{attempt.number} · {attempt.profile.name} · {statusName(
              attempt.status,
              locale,
            )}{attempt.source_revision !== undefined
              ? ` · v${attempt.source_revision}`
              : ""}</small
          >{#if attempt.error}<p>
              {operationError(attempt.error)}
            </p>{/if}{#if attempt.partial_text}{#if operation.kind === "reply"}<h4
              >
                {t("原文", "Original")}
              </h4>{/if}
            <pre>{attempt.partial_text}</pre>{/if}{#if attempt.back_translation}<h4
            >
              {t("译文", "Translation")}
            </h4>
            <pre>{attempt.back_translation}</pre>{/if}
        </article>{/each}
    </details>{/if}
  {#if error}<p role="alert">{error}</p>{/if}
</section>
