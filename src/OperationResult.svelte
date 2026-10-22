<script lang="ts">
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
  let error = $state("");
  async function copy() {
    try {
      await invoke("copy_text", {
        text: translationPlainText(operation.primary_text),
      });
    } catch (e) {
      error = String(e);
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
  {#if operation.primary_text}<TranslationContent
      text={operation.primary_text}
      {locale}
      controls={operation.kind !== "reply"}
      identity={`${operation.id}:${operation.attempt}`}
      streaming={active && operation.back_translation_state !== "pending"}
    />{/if}
  {#if operation.kind === "reply" && operation.back_translation_state !== "none"}<details
      class="back-translation"
    >
      <summary>{t("回译参考", "Back-translation reference")}</summary>
      <p class="workflow-note">
        {t(
          "辅助复述，不代表独立核验。",
          "An auxiliary paraphrase, not an independent verification.",
        )}
      </p>
      {#if operation.back_translation}<TranslationContent
          text={operation.back_translation}
          {locale}
          controls={false}
          identity={`${operation.id}:${operation.attempt}:back`}
          streaming={operation.back_translation_state === "pending"}
        />{:else}<p role="status">
          {operation.back_translation_state === "pending"
            ? t("正在生成回译…", "Generating back-translation…")
            : t(
                "回译未完成，外语回复仍可使用。",
                "Back-translation is incomplete. The reply remains available.",
              )}
        </p>{/if}
    </details>{/if}
  {#if operation.primary_text && ["failed", "stopped"].includes(operation.status)}<p
      class="workflow-note"
    >
      {t("这是已保留的部分结果。", "This is the retained partial result.")}
    </p>{/if}
  {#if operation.error}<p class="inline-feedback" role="alert">
      {errorText(operation.error, locale)}
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
    {#if operation.primary_text}<button
        disabled={active && operation.back_translation_state !== "pending"}
        onclick={copy}
        >{operation.kind === "reply"
          ? t("复制回复", "Copy reply")
          : t("复制解析", "Copy explanation")}</button
      >{/if}
  </div>
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
              {errorText(attempt.error, locale)}
            </p>{/if}{#if attempt.partial_text}<pre>{attempt.partial_text}</pre>{/if}{#if attempt.back_translation}<details
            >
              <summary>{t("回译参考", "Back-translation reference")}</summary>
              <pre>{attempt.back_translation}</pre>
            </details>{/if}
        </article>{/each}
    </details>{/if}
  {#if error}<p role="alert">{error}</p>{/if}
</section>
