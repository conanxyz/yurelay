<script lang="ts">
  import { onMount, tick, untrack } from "svelte";
  import {
    invoke,
    isTauri,
    Channel,
    convertFileSrc,
  } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import {
    active,
    defaults,
    type Task,
    type Wire,
    type Bootstrap,
    type Settings,
    type VocabularyCandidate,
    type VocabularyPreview,
    type TranslationHighlight,
  } from "./types";
  import {
    localeFor,
    text,
    statusName,
    errorText,
    applyAppearance,
    languageName,
    languages,
  } from "./locale";
  import ProviderLogo from "./ProviderLogo.svelte";
  import VocabularySidebar from "./VocabularySidebar.svelte";
  import TranslationContent from "./TranslationContent.svelte";
  import { translationPlainText } from "./reading";
  import Icon from "./Icons.svelte";
  import { serviceName } from "./services";
  import { imageEntrance, panelEntrance } from "./motion";
  let settings = $state<Settings>(structuredClone(defaults));
  let permissions = $state({ accessibility: false, screen: false });
  let locale = $derived(localeFor(settings));
  const t = (zh: string, en: string) => text(locale, zh, en);
  $effect(() => applyAppearance(settings));
  let tasks = $state<Task[]>([]),
    selected = $state<string | null>(null),
    notice = $state<string | null>(null),
    error = $state("");
  let pinned = $state(false),
    expanded = $state(false),
    resizing = $state(false),
    submitting = $state(false),
    draft = $state("");
  let sourceLanguage = $state(""),
    targetOverride = $state<string | null>(null),
    detected = $state<string | null>(null),
    disclosed = $state(false);
  let content: HTMLElement;
  let current = $derived(tasks.find((task) => task.id === selected));
  let vocabularyOpen = $state(false),
    sidebarWidth = $state(0);
  let vocabularyIdentity = "",
    vocabularyAutoShown = false;
  let sidebarQueue = Promise.resolve(),
    sidebarRevision = 0;
  $effect(() => {
    const identity = current ? `${current.id}:${current.attempt}` : "";
    const ready =
      current?.status === "completed" && !!current.vocabulary?.length;
    untrack(() => {
      if (identity !== vocabularyIdentity) {
        vocabularyIdentity = identity;
        vocabularyAutoShown = false;
        vocabularyOpen = false;
      }
      if (ready && !vocabularyAutoShown) {
        vocabularyAutoShown = true;
        vocabularyOpen = true;
      }
    });
  });
  let canStart = $derived(
    !!draft.trim() &&
      !current?.sensitive &&
      (!current || !active(current)) &&
      !submitting,
  );
  let waiting = $derived(
    current?.status === "ready" || current?.status === "original_only",
  );
  let targetLanguage = $derived(
    targetOverride ??
      (waiting || !current
        ? settings.target_language
        : current.target_language),
  );
  let service = $derived(
    current && active(current)
      ? current.profile
      : settings.profiles.find((p) => p.id === settings.active_profile),
  );
  let localDictionary = $derived(service?.provider === "system_dictionary");
  let sourceProcessing = $derived(
    !!current &&
      current.source === "screenshot" &&
      !current.sensitive &&
      ["capturing", "recognizing"].includes(current.status),
  );
  let captureFailure = $derived(
    current?.source === "screenshot" &&
      current.status === "failed" &&
      !current.sensitive &&
      !current.original_text.trim() &&
      !draft.trim()
      ? current.error
      : null,
  );
  let changed = $derived(
    !!current &&
      (service?.id !== current.profile.id ||
        service?.revision !== current.profile.revision ||
        draft !== (current.edited_text ?? current.original_text) ||
        sourceLanguage !== (current.source_language ?? "") ||
        targetLanguage !== current.target_language),
  );
  let attempted = $derived(
    !!current &&
      !current.sensitive &&
      !["ready", "original_only", "capturing", "recognizing"].includes(
        current.status,
      ),
  );
  let showResult = $derived(
    disclosed && (submitting || (attempted && !changed)),
  );
  let toggleDisabled = $derived(
    !!current?.sensitive ||
      (!current && !draft.trim()) ||
      (!submitting &&
        (!current || !active(current)) &&
        (waiting || changed) &&
        !canStart),
  );
  const candidateKey = (word: string) =>
    word.trim().replace(/\s+/gu, " ").toLowerCase();
  let manualCandidates = $state<VocabularyCandidate[]>([]);
  let candidateDetails = $state<Record<string, VocabularyCandidate>>({});
  let activeWord = $state("");
  let selectionHighlights = $state<TranslationHighlight[]>([]);
  let highlightTranslation = $state("");
  let previewSequence = 0;
  let previewTimer: ReturnType<typeof setTimeout> | undefined;
  let selectionScope = "";
  let lastSelection = "";
  type SourceSelection = {
    word: string;
    sourceText: string;
    selectionStart: number;
    selectionEnd: number;
    taskId: string | null;
    attempt: number | null;
  };
  let selectedRequest = $state<SourceSelection | null>(null);
  let previewContextIdentity = "";
  const previewContext = $derived(
    JSON.stringify([
      current?.status,
      current?.vocabulary_state,
      current?.vocabulary ?? [],
    ]),
  );
  const previewCache = new Map<string, VocabularyPreview>();
  const visibleCandidates = $derived.by(() => {
    const automatic =
      showResult && current?.status === "completed"
        ? (current.vocabulary ?? [])
        : [];
    const seen = new Set<string>();
    return [...manualCandidates, ...automatic]
      .filter((candidate) => {
        const key = candidateKey(candidate.word);
        if (seen.has(key)) return false;
        seen.add(key);
        return true;
      })
      .map(
        (candidate) =>
          candidateDetails[candidateKey(candidate.word)] ?? candidate,
      );
  });
  const hasVocabulary = $derived(
    !current?.sensitive && visibleCandidates.length > 0,
  );
  const activeHighlights = $derived(
    !changed && current?.translation === highlightTranslation
      ? selectionHighlights
      : [],
  );
  $effect(() => {
    const scope = `${current?.id ?? "draft"}:${current?.attempt ?? 0}:${draft}`;
    untrack(() => {
      if (scope !== selectionScope) {
        selectionScope = scope;
        previewSequence++;
        clearTimeout(previewTimer);
        lastSelection = "";
        previewCache.clear();
        selectedRequest = null;
        manualCandidates = [];
        candidateDetails = {};
        activeWord = "";
        selectionHighlights = [];
        highlightTranslation = "";
      }
    });
  });
  $effect(() => {
    const context = previewContext;
    untrack(() => {
      if (context === previewContextIdentity) return;
      previewContextIdentity = context;
      lastSelection = "";
      previewCache.clear();
      candidateDetails = {};
      selectionHighlights = [];
      if (selectedRequest?.sourceText === draft) {
        const request = {
          ...selectedRequest,
          taskId:
            !changed && current?.status === "completed" ? current.id : null,
          attempt:
            !changed && current?.status === "completed"
              ? current.attempt
              : null,
        };
        selectedRequest = request;
        requestPreview(request, false);
      }
    });
  });
  $effect(() => {
    const width = hasVocabulary ? (vocabularyOpen ? 220 : 40) : 0;
    const revision = ++sidebarRevision;
    sidebarQueue = sidebarQueue.then(async () => {
      if (revision !== sidebarRevision) return;
      try {
        const applied = isTauri()
          ? await invoke<number>("set_vocabulary_sidebar", { width })
          : width;
        if (revision === sidebarRevision)
          sidebarWidth = typeof applied === "number" ? applied : width;
      } catch (e) {
        if (revision === sidebarRevision) error = String(e);
      }
    });
  });
  let triggerLabel = $derived(
    showResult
      ? localDictionary
        ? t("收起释义", "Collapse definition")
        : t("收起译文", "Collapse translation")
      : !current || waiting || changed
        ? localDictionary
          ? t("查询并展开（X）", "Look up and expand (X)")
          : t("翻译并展开（X）", "Translate and expand (X)")
        : localDictionary
          ? t("展开释义", "Expand definition")
          : t("展开译文", "Expand translation"),
  );
  let detectionSequence = 0;
  $effect(() => {
    const value = draft;
    const ticket = ++detectionSequence;
    detected = null;
    if (!value.trim()) return;
    const timer = setTimeout(() => {
      if (isTauri())
        invoke<string | null>("detect_text_language", { text: value })
          .then((v) => {
            if (ticket === detectionSequence) detected = v;
          })
          .catch(() => {});
    }, 300);
    return () => clearTimeout(timer);
  });
  let requiredPermission = $derived(
    notice === "请先在设置中开启 Glossa 的屏幕录制权限。"
      ? "screen"
      : notice === "请先在设置中开启 Glossa 的辅助功能权限。"
        ? "accessibility"
        : null,
  );
  let visibleNotice = $derived(
    notice &&
      (!requiredPermission ||
        !permissions[requiredPermission as "screen" | "accessibility"])
      ? notice
      : null,
  );
  let pendingRevision = 0,
    pendingAttempt = 0;
  let draftId = "";
  let sourceVersion = "";
  let trigger: HTMLButtonElement;
  $effect(() => {
    const version = `${current?.id}:${current?.original_text}:${current?.edited_text}`;
    if (version !== sourceVersion) {
      sourceVersion = version;
      clearSelection();
      draft = current?.edited_text ?? current?.original_text ?? "";
    }
    if (current?.id !== draftId) {
      draftId = current?.id ?? "";
      submitting = false;
      sourceLanguage =
        current?.source_language ?? settings.source_language ?? "";
      targetOverride = null;
      detected = current?.detected_language ?? null;
      disclosed =
        !!current &&
        !current.sensitive &&
        !["ready", "original_only", "capturing", "recognizing"].includes(
          current.status,
        );
    }
  });
  function clearSelection() {
    selectedRequest = null;
    activeWord = "";
    selectionHighlights = [];
    highlightTranslation = "";
    lastSelection = "";
    previewSequence++;
    clearTimeout(previewTimer);
  }
  function captureSelection(event: Event) {
    const el = event.currentTarget as HTMLTextAreaElement;
    const word = el.value.slice(el.selectionStart, el.selectionEnd).trim();
    if (
      !word ||
      word.length > 128 ||
      word.split(/\s+/u).length > 3 ||
      !/\p{L}/u.test(word)
    ) {
      clearSelection();
      return;
    }
    const request = {
      word,
      sourceText: el.value,
      selectionStart: el.selectionStart,
      selectionEnd: el.selectionEnd,
      taskId: !changed && current?.status === "completed" ? current.id : null,
      attempt:
        !changed && current?.status === "completed" ? current.attempt : null,
    };
    selectedRequest = request;
    requestPreview(request, true);
  }
  function requestPreview(request: SourceSelection, reveal: boolean) {
    error = "";
    const translation = current?.translation ?? "";
    const key = JSON.stringify([
      selectionScope,
      request.selectionStart,
      request.selectionEnd,
      translation,
      previewContext,
    ]);
    if (key === lastSelection) return;
    lastSelection = key;
    const sequence = ++previewSequence;
    clearTimeout(previewTimer);
    selectionHighlights = [];
    activeWord = "";
    const scope = selectionScope;
    const cached = previewCache.get(key);
    if (cached) {
      applyPreview(cached, translation, reveal);
      return;
    }
    previewTimer = setTimeout(async () => {
      try {
        const result = await invoke<VocabularyPreview>("vocabulary_preview", {
          sourceText: request.sourceText,
          selectionStart: request.selectionStart,
          selectionEnd: request.selectionEnd,
          taskId: request.taskId,
          attempt: request.attempt,
        });
        if (
          sequence !== previewSequence ||
          scope !== selectionScope ||
          !result?.candidate
        )
          return;
        if (previewCache.size >= 100)
          previewCache.delete(previewCache.keys().next().value!);
        previewCache.set(key, result);
        applyPreview(result, translation, reveal);
      } catch (e) {
        if (sequence === previewSequence && scope === selectionScope)
          error = String(e);
      }
    }, 120);
  }
  function applyPreview(
    result: VocabularyPreview,
    translation: string,
    reveal: boolean,
  ) {
    error = "";
    const candidate = result.candidate;
    const wordKey = candidateKey(candidate.word);
    if (
      !visibleCandidates.some((item) => candidateKey(item.word) === wordKey)
    ) {
      manualCandidates = [candidate, ...manualCandidates];
    }
    candidateDetails = { ...candidateDetails, [wordKey]: candidate };
    activeWord = wordKey;
    selectionHighlights = result.translation_highlights ?? [];
    highlightTranslation = translation;
    if (reveal) {
      vocabularyOpen = true;
      vocabularyAutoShown = true;
    }
  }
  function selectWordAtCaret(event: MouseEvent) {
    if (event.button !== 0) return;
    const el = event.currentTarget as HTMLTextAreaElement;
    if (el.selectionStart === el.selectionEnd) {
      const caret = el.selectionStart;
      for (const match of el.value.matchAll(
        /[\p{L}\p{N}]+(?:['’_-][\p{L}\p{N}]+)*/gu,
      )) {
        const start = match.index!;
        if (start <= caret && caret <= start + match[0].length) {
          el.setSelectionRange(start, start + match[0].length);
          break;
        }
      }
    }
    captureSelection(event);
  }
  let resolvingSensitive = $state(false);
  async function resolveSensitive(allow: boolean) {
    if (!current?.sensitive || resolvingSensitive) return;
    resolvingSensitive = true;
    if (allow) disclosed = true;
    try {
      await action("resolve_sensitive", { id: current.id, allow });
    } finally {
      resolvingSensitive = false;
    }
  }
  async function action(command: string, args?: Record<string, unknown>) {
    error = "";
    try {
      return await invoke(command, args);
    } catch (e) {
      error = String(e);
      return undefined;
    }
  }
  async function refresh() {
    if (!isTauri()) return;
    try {
      const [p, s] = await Promise.all([
        invoke<typeof permissions>("permissions"),
        invoke<Settings>("configuration_settings"),
      ]);
      permissions = p;
      settings = s;
    } catch {
      /* Initial bootstrap reports errors. */
    }
  }
  async function translate() {
    if (!canStart) return;
    if (!current) {
      submitting = true;
      await action("translate_text", {
        text: draft,
        sourceLanguage: sourceLanguage || null,
        targetLanguage,
      });
      submitting = false;
      return;
    }
    pendingRevision = current.revision;
    pendingAttempt =
      current.status === "ready" ? current.attempt : current.attempt + 1;
    submitting = true;
    disclosed = true;
    const id = current.id;
    await action("retry_task", {
      id,
      editedText: draft,
      sourceLanguage: sourceLanguage || null,
      targetLanguage,
    });
    if ((error || !isTauri()) && current?.id === id) submitting = false;
  }
  function toggleTranslation() {
    if (toggleDisabled) return;
    if (!current) {
      translate();
      return;
    }
    if (showResult) {
      disclosed = false;
      return;
    }
    if (submitting || (current && active(current)) || (!waiting && !changed)) {
      disclosed = true;
      return;
    }
    translate();
  }
  function keydown(event: KeyboardEvent) {
    const editing =
      'input,textarea,select,[contenteditable]:not([contenteditable="false"]),[role="textbox"],[role="combobox"]';
    if (
      !!document.querySelector("dialog[open]") ||
      event.defaultPrevented ||
      event.repeat ||
      event.isComposing ||
      event.ctrlKey ||
      event.altKey ||
      event.metaKey ||
      (event.target as HTMLElement)?.closest(editing) ||
      document.activeElement?.closest(editing)
    )
      return;
    if (event.key.toLowerCase() === "x" && !toggleDisabled) {
      event.preventDefault();
      toggleTranslation();
    }
  }
  async function toggleExpanded() {
    if (resizing) return;
    const next = !expanded;
    resizing = true;
    await action("set_window_expanded", { expanded: next });
    if (!error) expanded = next;
    resizing = false;
  }
  function drag(event: PointerEvent) {
    if (event.button !== 0 || (event.target as HTMLElement).closest("button"))
      return;
    if (isTauri()) invoke("drag_window").catch((e) => (error = String(e)));
  }
  function apply(message: Wire) {
    const previous = current?.id;
    const priorStatus = current?.status;
    if (message.type === "snapshot") {
      tasks = message.snapshot.tasks;
      notice = message.notice;
      selected = message.selected;
      if (!tasks.some((task) => task.id === selected)) selected = null;
    } else
      for (const event of message.events) {
        if (event.type === "removed") {
          tasks = tasks.filter((task) => task.id !== event.id);
        } else if (event.type === "changed") {
          const index = tasks.findIndex((task) => task.id === event.task.id);
          if (index < 0) tasks.push(event.task);
          else if (tasks[index].revision <= event.task.revision)
            tasks[index] = event.task;
        } else {
          const task = tasks.find((task) => task.id === event.id);
          if (
            task &&
            task.attempt === event.attempt &&
            task.revision < event.revision
          ) {
            task.translation += event.text;
            task.revision = event.revision;
          }
        }
      }
    const next = tasks.find((task) => task.id === selected);
    if (
      next &&
      next.revision > pendingRevision &&
      next.attempt === pendingAttempt &&
      !["ready", "original_only"].includes(next.status)
    )
      submitting = false;
    if (
      next?.status === "ready" &&
      (next.id !== previous || priorStatus !== "ready")
    )
      tick().then(() => trigger?.focus({ preventScroll: true }));
  }
  onMount(() => {
    let frame = 0,
      lastHeight = 0;
    const observer = new ResizeObserver(() => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        const height = Math.ceil(content.getBoundingClientRect().height + 44);
        if (!expanded && height !== lastHeight && isTauri()) {
          lastHeight = height;
          invoke("fit_result_height", { height }).catch(() => {});
        }
      });
    });
    observer.observe(content);
    return () => {
      observer.disconnect();
      cancelAnimationFrame(frame);
    };
  });
  onMount(() => {
    if (!isTauri()) return;
    let disposed = false;
    let subscription: string | undefined;
    const cleanup: (() => void)[] = [];
    let preferenceEvents = 0;
    const timer = setInterval(() => {
      if (requiredPermission) refresh();
    }, 2000);
    (async () => {
      try {
        const off = await listen<Settings>("configuration-changed", (e) => {
          preferenceEvents++;
          settings = e.payload;
        });
        if (disposed) {
          off();
          return;
        }
        cleanup.push(off);
        const before = preferenceEvents;
        const data = await invoke<Bootstrap>("bootstrap");
        if (disposed) return;
        pinned = data.pinned;
        expanded = data.expanded;
        permissions = data.permissions;
        if (before === preferenceEvents) settings = data.configuration.settings;
        apply({
          type: "snapshot",
          snapshot: data.snapshot,
          selected: data.selected,
          notice: data.notice,
        });
        const channel = new Channel<Wire>();
        channel.onmessage = apply;
        subscription = await invoke("subscribe", { onEvent: channel });
        if (disposed && subscription)
          await invoke("unsubscribe", { subscriptionId: subscription });
        const offMode = await listen<boolean>(
          "window-mode-changed",
          (e) => (expanded = e.payload),
        );
        if (disposed) offMode();
        else cleanup.push(offMode);
      } catch (e) {
        error = String(e);
      }
    })();
    return () => {
      disposed = true;
      previewSequence++;
      clearTimeout(previewTimer);
      clearInterval(timer);
      cleanup.forEach((off) => off());
      if (subscription)
        invoke("unsubscribe", { subscriptionId: subscription }).catch(() => {});
    };
  });
</script>

<svelte:window onfocus={refresh} onkeydown={keydown} />
<div
  class="translation-window"
  class:expanded
  class:with-vocabulary={sidebarWidth > 0}
  style={`--reading-size:${settings.font_size}px;--vocabulary-width:${sidebarWidth}px`}
>
  {#if hasVocabulary && sidebarWidth > 0}{#key `${current?.id ?? "draft"}:${current?.attempt ?? 0}`}<VocabularySidebar
        candidates={visibleCandidates}
        {activeWord}
        {locale}
        open={sidebarWidth > 40}
        onToggle={() => (vocabularyOpen = !vocabularyOpen)}
      />{/key}{/if}
  <div class="translation-main">
    <div class="window-chrome" onpointerdown={drag} role="presentation">
      <span class="window-name">Glossa</span>
      <div class="window-tools">
        <button
          class="icon-button"
          aria-label={expanded
            ? t("恢复小窗口", "Restore compact window")
            : t("展开", "Expand")}
          title={expanded
            ? t("恢复小窗口", "Restore compact window")
            : t("展开", "Expand")}
          aria-pressed={expanded}
          disabled={resizing}
          onclick={toggleExpanded}
          ><Icon name={expanded ? "compact" : "expand"} size={16} /></button
        >
        <button
          class="icon-button"
          aria-label={t("收起", "Hide")}
          title={t("收起", "Hide")}
          onclick={() => action("hide_window")}
          ><Icon name="hide" size={17} /></button
        >
        <button
          class="icon-button"
          class:chosen={pinned}
          aria-label={pinned ? t("取消置顶", "Unpin") : t("置顶", "Pin")}
          title={pinned ? t("取消置顶", "Unpin") : t("置顶", "Pin")}
          aria-pressed={pinned}
          onclick={async () => {
            const next = !pinned;
            await action("pin_window", { pinned: next });
            if (!error) pinned = next;
          }}><Icon name="pin" size={16} /></button
        >
      </div>
    </div>
    <main bind:this={content!} class="translation-content">
      {#if visibleNotice}<div class="inline-feedback" role="alert">
          <span>{errorText(visibleNotice, locale)}</span
          >{#if requiredPermission}<button
              onclick={async () => {
                await action("request_permission", {
                  kind: requiredPermission,
                });
                await refresh();
              }}>{t("打开设置", "Open settings")}</button
            >{/if}
        </div>{/if}
      {#if error}<p class="inline-feedback" role="alert">
          {errorText(error, locale)}
        </p>{/if}
      {#if captureFailure}<p class="inline-feedback" role="alert">
          {errorText(captureFailure, locale)}
        </p>{/if}
      <div class="reading-sheet">
        <section class="source-panel" aria-label={t("原文", "Source")}>
          <div class="reading-caption">
            <h1>{t("原文", "Source")}</h1>
            <select
              class="language-control"
              aria-label={t("原文语言", "Source language")}
              title={t(
                "自动识别或手动指定原文语言",
                "Detect automatically or choose the source language",
              )}
              bind:value={sourceLanguage}
              disabled={!!current && (active(current) || !!current.sensitive)}
            >
              <option value=""
                >{detected
                  ? `${t("自动", "Auto")} · ${languageName(detected, locale)}`
                  : t("自动检测", "Auto-detect")}</option
              >
              {#each languages as language}<option value={language[0]}
                  >{language[locale === "zh" ? 1 : 2]}</option
                >{/each}
            </select>
          </div>
          {#if current?.sensitive}
            <div class="sensitive-mask" role="alert">
              <span class="guard-emblem"><Icon name="shield" size={30} /></span>
              <h2>
                {t(
                  "检测到疑似敏感信息（助记词 / 私钥 / API 密钥）",
                  "Possible sensitive information detected (seed phrase / private key / API key)",
                )}
              </h2>
              <p>
                {current.source === "manual"
                  ? t(
                      "Glossa 已在本地拦截该请求并隐藏您输入的原文，未发送至任何云端服务。",
                      "Glossa blocked this request locally and hid your input. It has not been sent to any cloud service.",
                    )
                  : t(
                      "为保护您的资产与隐私安全，Glossa 已在本地拦截该内容，未将其展示或发送至任何云端服务。",
                      "Glossa blocked this content locally. It has not been displayed or sent to any cloud service.",
                    )}
              </p>
              <div class="guard-actions">
                <button
                  class="primary"
                  disabled={resolvingSensitive}
                  onclick={() => resolveSensitive(false)}
                  >{t("取消并清空", "Cancel and clear")}</button
                ><button
                  disabled={resolvingSensitive}
                  onclick={() => resolveSensitive(true)}
                  >{t(
                    "继续展示原文并翻译",
                    "Reveal source and translate",
                  )}</button
                >
              </div>
            </div>
          {:else}
            {#if current?.has_image}<details class="source-image">
                <summary>{t("查看截图", "View screenshot")}</summary><img
                  use:imageEntrance
                  src={convertFileSrc(current.id, "glossa-image")}
                  alt={t("原始截图选区", "Original screenshot selection")}
                />
              </details>{/if}
            <div class="source-editor-frame">
              <textarea
                class="source-editor"
                aria-label={t("原文内容", "Source text")}
                aria-busy={sourceProcessing}
                aria-describedby={sourceProcessing
                  ? "source-processing-status"
                  : undefined}
                bind:value={draft}
                readonly={!!current && active(current)}
                onselect={captureSelection}
                onmouseup={selectWordAtCaret}
                oninput={clearSelection}
                placeholder={sourceProcessing
                  ? ""
                  : t(
                      "选中文字或截取屏幕区域…",
                      "Select text or capture a screen region…",
                    )}
                spellcheck="false"></textarea>
              {#if sourceProcessing}<div
                  class="source-processing"
                  id="source-processing-status"
                  role="status"
                  aria-live="polite"
                >
                  <svg
                    class="translation-loader"
                    width="20"
                    height="20"
                    viewBox="0 0 24 24"
                    fill="none"
                    aria-hidden="true"
                  >
                    <circle
                      cx="12"
                      cy="12"
                      r="9"
                      stroke="currentColor"
                      stroke-width="1.8"
                      opacity=".2"
                    />
                    <path
                      d="M12 3a9 9 0 0 1 9 9"
                      stroke="currentColor"
                      stroke-width="1.8"
                      stroke-linecap="round"
                    />
                  </svg>
                  <span
                    >{current?.status === "capturing"
                      ? t("正在准备截图…", "Preparing screenshot…")
                      : t(
                          "正在识别截图文字…",
                          "Recognizing screenshot text…",
                        )}</span
                  >
                </div>{/if}
            </div>
          {/if}
          {#if current?.image_error}<p class="inline-feedback">
              {errorText(current.image_error, locale)}
            </p>{/if}
        </section>
        <div class="service-strip">
          <div
            class="service-identity"
            title={`${service?.name ?? ""} · ${service?.model ?? ""}`}
          >
            <span class="provider-mark"
              >{#if service}<ProviderLogo
                  provider={service.provider}
                  size={18}
                />{/if}</span
            ><span class="provider-name" title={service?.name}
              >{service
                ? serviceName(service, locale)
                : t("未配置服务", "No service")}</span
            ><span class="model-name" title={service?.model}
              >{service?.provider === "system_dictionary"
                ? t("本机词典", "Local dictionary")
                : service?.provider === "youdao"
                  ? t("文本翻译", "Text translation")
                  : service?.model || t("选择模型", "Choose a model")}</span
            >
          </div>
          <div class="translation-controls">
            {#if service?.provider !== "system_dictionary"}<label
                class="target-language"
                ><span>{t("译文", "To")}</span><select
                  class="language-control"
                  aria-label={t("译文语言", "Translation language")}
                  value={targetLanguage}
                  onchange={(e) => (targetOverride = e.currentTarget.value)}
                  disabled={!!current &&
                    (active(current) || !!current.sensitive)}
                  >{#each languages as language}<option value={language[0]}
                      >{language[locale === "zh" ? 1 : 2]}</option
                    >{/each}{#if !languages.some((l) => l[0] === targetLanguage)}<option
                      value={targetLanguage}
                      >{languageName(targetLanguage, locale)}</option
                    >{/if}</select
                ></label
              >
            {/if}
            {#if current && ["queued", "translating"].includes(current.status)}<div
                class="translation-progress"
              >
                <button
                  class="icon-button small-control"
                  aria-label={t("停止翻译", "Stop translation")}
                  aria-describedby="translation-stop-tip"
                  onclick={() => action("stop_task", { id: current.id })}
                  ><svg
                    class="translation-loader"
                    width="16"
                    height="16"
                    viewBox="0 0 24 24"
                    fill="none"
                    aria-hidden="true"
                    ><circle
                      cx="12"
                      cy="12"
                      r="9"
                      stroke="currentColor"
                      stroke-width="1.8"
                      opacity=".2"
                    /><path
                      d="M12 3a9 9 0 0 1 9 9"
                      stroke="currentColor"
                      stroke-width="1.8"
                      stroke-linecap="round"
                    /></svg
                  ></button
                ><span
                  class="progress-tooltip"
                  id="translation-stop-tip"
                  role="tooltip">{t("停止翻译", "Stop translation")}</span
                >
              </div>{/if}
            <button
              bind:this={trigger!}
              class="icon-button disclosure-control"
              class:opened={showResult}
              title={triggerLabel}
              aria-label={triggerLabel}
              aria-expanded={showResult}
              aria-controls="translation-body"
              aria-keyshortcuts="X"
              disabled={toggleDisabled}
              onclick={toggleTranslation}
              ><Icon name="chevron" size={15} /></button
            >
          </div>
        </div>
        {#if showResult}<section
            id="translation-body"
            class="translation-body"
            use:panelEntrance
            aria-label={localDictionary
              ? t("释义", "Definition")
              : t("译文", "Translation")}
          >
            {#if current?.error && !captureFailure}<p
                class="inline-feedback"
                role="alert"
              >
                {errorText(current.error, locale)}
              </p>{/if}
            {#if current?.translation && !submitting}<TranslationContent
                text={current.translation}
                {locale}
                identity={`${current.id}:${current.attempt}`}
                streaming={active(current)}
                plain={localDictionary}
                highlights={activeHighlights}
              />
            {:else if submitting || (current && active(current))}<div
                class="translation-skeleton"
                aria-busy="true"
              >
                <i></i><i></i>
                <p>{t("正在翻译…", "Translating…")}</p>
              </div>
            {:else}<p class="result-empty">
                {localDictionary
                  ? t("暂无释义，可重新查询", "No definition yet. Try again.")
                  : t("尚无译文，可重试", "No translation yet. Try again.")}
              </p>{/if}
            {#if service?.provider === "youdao" && draft.trim() && draft.length <= 512}<button
                class="dictionary-link"
                disabled={!draft.trim() || draft.length > 512}
                onclick={() =>
                  action("open_dictionary", {
                    kind: "youdao",
                    text: draft.trim(),
                  })}
                >{t("在有道词典中查看", "View in Youdao dictionary")}</button
              >{/if}
            {#if current}<div class="result-actions">
                <button
                  class="copy-translation"
                  disabled={active(current) || !current.translation}
                  onclick={() =>
                    action("copy_text", {
                      text: localDictionary
                        ? current!.translation
                        : translationPlainText(current!.translation),
                    })}>{t("复制译文", "Copy translation")}</button
                >
                {#if current.vocabulary_state === "extracting"}<button
                    class="quiet-tail-stop"
                    aria-label={t("停止生词提炼", "Stop vocabulary extraction")}
                    title={t("停止生词提炼", "Stop vocabulary extraction")}
                    onclick={() => action("stop_task", { id: current!.id })}
                    ><Icon name="stop" size={12} /></button
                  >{/if}
                <button
                  class="retry-link"
                  disabled={!canStart}
                  onclick={translate}
                  >{localDictionary
                    ? t("重新查询", "Look up again")
                    : t("重新翻译", "Translate again")}</button
                >
              </div>{/if}
          </section>{/if}
        <span class="sr-only" role="status" aria-hidden={sourceProcessing}
          >{current ? statusName(current.status, locale) : ""}</span
        >
      </div>
      {#if waiting && !submitting && !current?.sensitive}<div
          class="capture-hint"
        >
          <span
            >{localDictionary
              ? t("检查原文后开始查询", "Review the word before looking it up")
              : t(
                  "检查原文后开始翻译",
                  "Review the source before translating",
                )}</span
          ><kbd>X</kbd>
        </div>{/if}
    </main>
  </div>
</div>
