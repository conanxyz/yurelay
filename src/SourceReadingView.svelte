<script lang="ts">
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import type { SourceEncounter, Task } from "./types";
  import type { Locale } from "./locale";
  let {
    task,
    text,
    locale,
    enabled = true,
    onSelect,
  }: {
    task: Task;
    text: string;
    locale: Locale;
    enabled?: boolean;
    onSelect: (start: number, end: number) => void;
  } = $props();
  const t = (zh: string, en: string) => (locale === "zh" ? zh : en);
  let matches = $state<SourceEncounter[]>([]),
    busy = $state(false),
    error = $state(""),
    refresh = $state(0);
  let host: HTMLDivElement;
  let generation = 0;
  const parts = $derived.by(() => {
    let end = 0;
    const result: { text: string; match?: SourceEncounter }[] = [];
    for (const m of matches) {
      if (m.start_utf16 < end || m.end_utf16 > text.length) continue;
      if (m.start_utf16 > end)
        result.push({ text: text.slice(end, m.start_utf16) });
      result.push({ text: text.slice(m.start_utf16, m.end_utf16), match: m });
      end = m.end_utf16;
    }
    if (end < text.length) result.push({ text: text.slice(end) });
    return result;
  });
  $effect(() => {
    const id = task.id,
      revision = task.source_revision ?? 0,
      value = text,
      highlight = enabled;
    void refresh;
    const ticket = ++generation;
    matches = [];
    error = "";
    busy = false;
    if (
      !highlight ||
      !isTauri() ||
      value !== (task.edited_text ?? task.original_text)
    )
      return;
    busy = true;
    invoke<{ matches: SourceEncounter[] }>("source_encounters", {
      request: { task_id: id, expected_revision: revision },
    })
      .then((r) => {
        if (ticket === generation)
          matches = (r?.matches ?? []).sort(
            (a, b) => a.start_utf16 - b.start_utf16,
          );
      })
      .catch(() => {
        if (ticket === generation)
          error = t("本地匹配暂不可用", "Local matching is unavailable");
      })
      .finally(() => {
        if (ticket === generation) busy = false;
      });
    return () => {
      generation++;
    };
  });
  function selection(event: Event) {
    const s = window.getSelection();
    if (!s || !host) return;
    if (s.isCollapsed) {
      if (
        !(event instanceof MouseEvent) ||
        event.button !== 0 ||
        (event.target as Element)?.closest("button")
      )
        return;
      const position = document as Document & {
        caretRangeFromPoint?: (x: number, y: number) => Range | null;
      };
      const caret = position.caretRangeFromPoint?.(
        event.clientX,
        event.clientY,
      );
      if (!caret || !host.contains(caret.startContainer)) return;
      const before = document.createRange();
      before.selectNodeContents(host);
      before.setEnd(caret.startContainer, caret.startOffset);
      const offset = before.toString().length;
      for (const match of text.matchAll(
        /[\p{L}\p{N}]+(?:['’_-][\p{L}\p{N}]+)*/gu,
      )) {
        const start = match.index!;
        if (start <= offset && offset <= start + match[0].length) {
          onSelect(start, start + match[0].length);
          break;
        }
      }
      return;
    }
    if (!host.contains(s.anchorNode) || !host.contains(s.focusNode)) return;
    const r = s.getRangeAt(0);
    const prefix = document.createRange();
    prefix.selectNodeContents(host);
    prefix.setEnd(r.startContainer, r.startOffset);
    const start = prefix.toString().length;
    onSelect(start, start + r.toString().length);
  }
  onMount(() => {
    let disposed = false,
      off: (() => void) | undefined;
    listen("vocabulary-changed", () => refresh++).then((f) => {
      if (disposed) f();
      else off = f;
    });
    return () => {
      disposed = true;
      off?.();
    };
  });
</script>

<div
  class="source-reading-view"
  bind:this={host!}
  onmouseup={selection}
  onkeyup={selection}
  role="textbox"
  aria-readonly="true"
  aria-busy={busy}
  aria-multiline="true"
  aria-label={t("原文阅读", "Source reading")}
  tabindex="0"
>
  {#each parts as part}{#if part.match}<button
        class="encounter-word"
        title={t("正在学习，查看本地释义", "Learning · view local definition")}
        onclick={() => onSelect(part.match!.start_utf16, part.match!.end_utf16)}
        >{part.text}</button
      >{:else}{part.text}{/if}{/each}
</div>
{#if busy}<span class="sr-only" role="status"
    >{t(
      "正在本地匹配学习中的表达…",
      "Matching learning vocabulary locally…",
    )}</span
  >{/if}
{#if error}<small class="workflow-note" role="status">{error}</small>{/if}
