<script lang="ts">
  import { onMount } from "svelte";
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { text, errorText, type Locale } from "./locale";
  import Icon from "./Icons.svelte";
  let {
    value = $bindable(),
    name,
    locale,
    onrecordingchange,
  }: {
    value: string;
    name: "selection" | "screenshot" | "append_selection" | "append_screenshot";
    locale: Locale;
    onrecordingchange: (active: boolean) => void;
  } = $props();
  let phase = $state<"idle" | "starting" | "recording" | "captured" | "ending">(
    "idle",
  );
  let draft = $state(""),
    error = $state("");
  let button: HTMLButtonElement;
  let epoch: number | undefined;
  let disposed = false;
  let cancelled = false;
  let capturedCode = "";
  const t = (zh: string, en: string) => text(locale, zh, en);
  const optional = $derived(name.startsWith("append_"));
  const selection = $derived(name.endsWith("selection"));
  const recordLabel = $derived(
    optional
      ? selection
        ? t("录入追加划词快捷键", "Record append selection shortcut")
        : t("录入追加截图快捷键", "Record append screenshot shortcut")
      : selection
        ? t("录入划词快捷键", "Record selection shortcut")
        : t("录入截图快捷键", "Record screenshot shortcut"),
  );
  const operationLabel = $derived(
    optional
      ? selection
        ? t("追加划词", "Append selection")
        : t("追加截图", "Append screenshot")
      : selection
        ? t("划词翻译", "Translate selection")
        : t("截屏翻译", "Translate screenshot"),
  );
  function keys(shortcut: string) {
    const labels: Record<string, string> = {
      Control: "⌃",
      Alt: "⌥",
      Shift: "⇧",
      Super: "⌘",
      Space: t("空格", "Space"),
      ArrowUp: "↑",
      ArrowDown: "↓",
      ArrowLeft: "←",
      ArrowRight: "→",
    };
    return shortcut
      .split("+")
      .filter(Boolean)
      .map((key) => labels[key] ?? key);
  }
  async function begin() {
    if (phase !== "idle") return;
    button.focus({ preventScroll: true });
    cancelled = false;
    phase = "starting";
    error = "";
    draft = "";
    onrecordingchange(true);
    try {
      epoch = isTauri() ? await invoke<number>("begin_shortcut_recording") : 0;
      if (disposed || cancelled || document.activeElement !== button) {
        phase = "recording";
        await finish(false);
        return;
      }
      phase = "recording";
    } catch (e) {
      error = errorText(e, locale);
      phase = "idle";
      onrecordingchange(false);
    }
  }
  async function finish(commit = false) {
    if (phase === "idle" || phase === "ending") return;
    const next = draft;
    if (phase === "starting") cancelled = true;
    phase = "ending";
    if (epoch === undefined) return; // begin() will release its lease when it resolves.
    const lease = epoch;
    epoch = undefined;
    try {
      if (isTauri()) {
        const report = await invoke<{
          registered: boolean;
          error: string | null;
        }>("end_shortcut_recording", { epoch: lease });
        if (report.error) error = errorText(report.error, locale);
      }
      if (commit && next) value = next;
    } catch (e) {
      error = errorText(e, locale);
    } finally {
      phase = "idle";
      draft = "";
      onrecordingchange(false);
    }
  }
  function clear() {
    if (!optional || phase !== "recording") return;
    value = "";
    void finish(false);
  }
  function keydown(e: KeyboardEvent) {
    if (phase === "idle") return;
    if (e.key === "Tab" && !e.altKey && !e.ctrlKey && !e.metaKey) {
      finish(false);
      return;
    }
    e.preventDefault();
    e.stopPropagation();
    if (
      optional &&
      phase === "recording" &&
      !e.altKey &&
      !e.ctrlKey &&
      !e.metaKey &&
      !e.shiftKey &&
      ["Backspace", "Delete"].includes(e.key)
    ) {
      void clear();
      return;
    }
    if (e.key === "Escape") {
      finish(false);
      return;
    }
    if (
      phase !== "recording" ||
      e.repeat ||
      ["Alt", "Shift", "Control", "Meta"].includes(e.key)
    )
      return;
    if (!e.altKey && !e.ctrlKey && !e.metaKey) {
      error = t(
        "请同时按下 Option、Control 或 Command。",
        "Hold Option, Control, or Command with a key.",
      );
      return;
    }
    const key = e.code.replace(/^Key/, "").replace(/^Digit/, "");
    if (
      !/^[A-Z0-9]$|^F([1-9]|1\d|2[0-4])$|^(Space|Enter|Backspace|Delete|Home|End|PageUp|PageDown|ArrowUp|ArrowDown|ArrowLeft|ArrowRight|Comma|Period|Slash|Backslash|Semicolon|Quote|BracketLeft|BracketRight|Minus|Equal|Backquote|Tab)$/.test(
        key,
      )
    ) {
      error = t(
        "此按键暂不支持，请换一个组合键。",
        "This key is not supported. Try another combination.",
      );
      return;
    }
    draft = [
      e.ctrlKey ? "Control" : "",
      e.altKey ? "Alt" : "",
      e.shiftKey ? "Shift" : "",
      e.metaKey ? "Super" : "",
      key,
    ]
      .filter(Boolean)
      .join("+");
    capturedCode = e.code;
    phase = "captured";
    error = "";
  }
  function keyup(e: KeyboardEvent) {
    if (phase === "captured" && e.code === capturedCode) finish(true);
  }
  onMount(() => {
    let off: (() => void) | undefined;
    if (isTauri())
      listen<number>("shortcut-recording-ended", (event) => {
        if (event.payload === epoch) {
          epoch = undefined;
          phase = "idle";
          draft = "";
          onrecordingchange(false);
        }
      }).then((fn) => {
        if (disposed) fn();
        else off = fn;
      });
    return () => {
      disposed = true;
      off?.();
      finish(false);
    };
  });
</script>

<svelte:window onkeyup={keyup} onblur={() => finish(false)} />
<div class="shortcut-row" class:optional>
  <button
    bind:this={button!}
    class="shortcut-recorder"
    class:recording={phase !== "idle"}
    aria-label={recordLabel}
    title={optional
      ? t(
          "点击录入；Delete 清空，Esc 取消；保存后生效",
          "Click to record; Delete clears, Esc cancels; save to apply",
        )
      : recordLabel}
    onclick={() => (phase === "captured" ? finish(true) : begin())}
    onkeydown={keydown}
    onblur={() => finish(false)}
  >
    <span class="shortcut-operation"
      ><span class="shortcut-symbol"
        ><Icon name={selection ? "translate" : "image"} size={18} /></span
      ><span class="shortcut-caption"
        ><span class="shortcut-label">{operationLabel}</span
        >{#if !optional}<span class="recorder-action"
            >{phase === "idle"
              ? t("点击录入快捷键", "click to record key")
              : phase === "captured"
                ? t("松开主键或点击完成", "Release key or click to finish")
                : t("Esc 取消", "Esc to cancel")}</span
          >{/if}</span
      ></span
    >
    <span class="shortcut-keycaps">
      {#if phase === "recording"}<span class="shortcut-recording-text"
          >{t("请按下组合键…", "Press keys…")}</span
        >
      {:else if phase === "starting"}<span class="shortcut-recording-text"
          >{t("准备录入…", "Preparing…")}</span
        >
      {:else if !(draft || value)}<span class="shortcut-recording-text"
          >{t("点击录入", "Click to record")}</span
        >
      {:else}{#each keys(draft || value) as key}<kbd>{key}</kbd>{/each}{/if}
    </span>
  </button>
  {#if optional && phase === "recording"}<div class="shortcut-edit-actions">
      <button
        type="button"
        class="shortcut-clear"
        aria-label={selection
          ? t("停用追加划词快捷键", "Disable append selection shortcut")
          : t("停用追加截图快捷键", "Disable append screenshot shortcut")}
        title={t("Delete 清空；保存后生效", "Delete to clear; save to apply")}
        onpointerdown={(event) => event.preventDefault()}
        onclick={clear}>{t("清空并停用", "Clear and disable")}</button
      >
    </div>{/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
</div>
