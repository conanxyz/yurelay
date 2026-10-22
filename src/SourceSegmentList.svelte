<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onDestroy } from "svelte";
  import type { SourceAsset, Task } from "./types";
  import type { Locale } from "./locale";
  let {
    task,
    locale,
    editable = false,
    onUpdate,
  }: {
    task: Task;
    locale: Locale;
    editable?: boolean;
    onUpdate?: (task: Task) => void;
  } = $props();
  let changing = $state(false);
  async function removeLast() {
    if (!editable || changing || !onUpdate) return;
    changing = true;
    error = "";
    try {
      onUpdate(
        await invoke<Task>("source_remove_last", {
          request: {
            task_id: task.id,
            expected_revision: task.source_revision ?? 0,
          },
        }),
      );
    } catch {
      error = t(
        "无法移除这段原文，请先完成当前编辑。",
        "This segment could not be removed. Finish the current edit first.",
      );
    } finally {
      changing = false;
    }
  }
  const t = (zh: string, en: string) => (locale === "zh" ? zh : en);
  let assets = $state<SourceAsset[]>([]),
    images = $state<Record<string, string>>({}),
    loading = $state<string[]>([]),
    error = $state("");
  const segmentCount = $derived(Math.max(1, task.source_segments?.length ?? 1));
  const singleImage = $derived(segmentCount === 1 && assets.length === 1);
  let version = 0;
  const urls = new Set<string>();
  $effect(() => {
    const id = task.id,
      revision = task.source_revision;
    void revision;
    const ticket = ++version;
    urls.forEach((url) => URL.revokeObjectURL(url));
    urls.clear();
    assets = [];
    images = {};
    loading = [];
    error = "";
    invoke<SourceAsset[]>("source_assets", { taskId: id })
      .then((r) => {
        if (ticket === version) assets = r ?? [];
      })
      .catch(() => {});
  });
  async function show(asset: SourceAsset) {
    if (images[asset.id] || loading.includes(asset.id)) return;
    const ticket = version;
    loading = [...loading, asset.id];
    error = "";
    try {
      const bytes = await invoke<number[]>("source_asset", {
        taskId: task.id,
        assetId: asset.id,
      });
      if (ticket !== version) return;
      const url = URL.createObjectURL(
        new Blob([new Uint8Array(bytes)], { type: "image/png" }),
      );
      urls.add(url);
      images = { ...images, [asset.id]: url };
    } catch {
      error = t(
        "截图读取失败，可以重试",
        "Screenshot could not be loaded. Try again.",
      );
    } finally {
      if (ticket === version) loading = loading.filter((id) => id !== asset.id);
    }
  }
  onDestroy(() => {
    version++;
    urls.forEach((url) => URL.revokeObjectURL(url));
  });
</script>

{#if segmentCount > 1 || assets.length > 1 || assets.some((asset) => !asset.legacy)}<details
    class="source-segments"
    ontoggle={(event) => {
      if (event.currentTarget.open && singleImage) void show(assets[0]);
    }}
  >
    <summary
      >{#if singleImage}{t("查看截图", "View screenshot")}{:else}{segmentCount}
        {t("段原文", "source segments")} · {assets.length}
        {t("张截图", "screenshots")}{/if}</summary
    >
    {#if segmentCount > 1}
      <ol>
        {#each task.source_segments ?? [] as segment}<li>
            <span
              >{segment.kind === "screenshot"
                ? t("截图", "Screenshot")
                : segment.kind === "manual"
                  ? t("输入", "Typed")
                  : t("划词", "Selection")}</span
            ><small>{segment.original_text.slice(0, 100)}</small>
          </li>{/each}
      </ol>{/if}
    {#if editable && onUpdate && (task.source_segments?.length ?? 0) > 1}<button
        disabled={changing}
        onclick={removeLast}>{t("移除最后一段", "Remove last segment")}</button
      >{/if}
    {#each assets as asset}{#if !singleImage}<button
          disabled={loading.includes(asset.id)}
          onclick={() => show(asset)}
          >{t("查看截图", "View screenshot")}
          {assets.indexOf(asset) + 1}</button
        >{/if}{#if images[asset.id]}<img
          src={images[asset.id]}
          alt={t("原始截图", "Original screenshot")}
        />{/if}{/each}{#if error}<p role="alert">{error}</p>{/if}
  </details>{/if}
