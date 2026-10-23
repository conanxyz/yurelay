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
  import ResizableReading from "./ResizableReading.svelte";
  import { fitReadingWorkspace } from "./readingWorkspace";
  let viewportWidth = $state(0);
  let pairedHeight = $state<number | null>(null);
  let pairedViewportHeight = $state<number | null>(null);
  let pairedAvailable = $state(400);
  const paired = $derived(viewportWidth >= 1100);
  const pairedRowHeight = $derived(
    pairedHeight === null
      ? pairedAvailable
      : Math.max(
          240,
          Math.min(
            pairedAvailable,
            Math.round(
              (pairedHeight * pairedAvailable) /
                (pairedViewportHeight ?? pairedAvailable),
            ),
          ),
        ),
  );
  function resizePair(height: number | null, commit: boolean) {
    pairedHeight =
      height === null
        ? null
        : Math.max(240, Math.min(4000, pairedAvailable, Math.round(height)));
    pairedViewportHeight = height === null ? null : pairedAvailable;
    if (commit) saveReadingHeight("paired", pairedHeight, pairedViewportHeight);
  }
  let sourceHeight = $state<number | null>(null);
  let translationHeight = $state<number | null>(null);
  let sourceViewportHeight = $state<number | null>(null);
  let translationViewportHeight = $state<number | null>(null);
  let readingLayoutLoading = $state(true);
  let heightSaveQueue = Promise.resolve();
  function saveReadingHeight(
    panel: "source" | "translation" | "paired",
    height: number | null,
    viewportHeight: number | null,
  ) {
    if (!isTauri()) return;
    heightSaveQueue = heightSaveQueue
      .then(() =>
        invoke<void>("save_reading_height", { panel, height, viewportHeight }),
      )
      .catch((e) => {
        error = errorText(e, locale);
      });
  }
  onMount(() => {
    if (!isTauri()) {
      readingLayoutLoading = false;
      return;
    }
    let disposed = false;
    invoke<{
      source_height: number | null;
      translation_height: number | null;
      source_viewport_height?: number | null;
      translation_viewport_height?: number | null;
      paired_height?: number | null;
      paired_viewport_height?: number | null;
    }>("reading_layout")
      .then((layout) => {
        if (!disposed && layout) {
          pairedHeight = layout.paired_height ?? null;
          pairedViewportHeight = layout.paired_viewport_height ?? null;
          sourceHeight = layout.source_height;
          translationHeight = layout.translation_height;
          sourceViewportHeight = layout.source_viewport_height ?? null;
          translationViewportHeight =
            layout.translation_viewport_height ?? null;
        }
      })
      .catch((e) => {
        if (!disposed) error = errorText(e, locale);
      })
      .finally(() => {
        if (!disposed) readingLayoutLoading = false;
      });
    return () => {
      disposed = true;
    };
  });
  import GettingStarted from "./GettingStarted.svelte";
  import type {
    Experience,
    ExperienceStatus,
    SettingsRoute,
    WindowSizing,
  } from "./types";
  let experience = $state<ExperienceStatus | null>(null);
  let guideOpen = $state(false);
  let guideBusy = $state(false);
  let guideView = $state<GettingStarted>();
  let guideStep = $state(0);
  let experienceSequence = 0;
  let guideOpener: HTMLElement | null = null;
  async function refreshExperience() {
    if (!isTauri()) return;
    const sequence = ++experienceSequence;
    const result = await invoke<ExperienceStatus>("experience_status");
    if (sequence === experienceSequence) experience = result;
  }
  const serviceStatus = $derived(
    experience?.services.find((s) => s.profile_id === service?.id),
  );
  const setupBlocked = $derived(
    !!serviceStatus &&
      ["needs_configuration", "needs_key"].includes(serviceStatus.state),
  );
  async function openSettings(route: SettingsRoute) {
    await action("open_settings_at", { route });
  }
  async function toggleGuide() {
    if (guideOpen) await guideView?.skip();
    else openGuide();
  }
  function openGuide() {
    if (guideOpen) return;
    guideOpener = document.activeElement as HTMLElement;
    guideStep = experience?.progress.step ?? 0;
    guideOpen = true;
    tick().then(() =>
      content?.querySelector<HTMLButtonElement>(".guide-steps button")?.focus(),
    );
  }
  function closeGuide() {
    if (!guideOpen) return;
    guideOpen = false;
    guideBusy = false;
    void refreshExperience().catch((e) => (error = errorText(e, locale)));
    tick().then(() =>
      guideOpener?.isConnected ? guideOpener.focus() : sourceEditor?.focus(),
    );
  }
  function windowMode(value: boolean | WindowSizing) {
    expanded = typeof value === "boolean" ? value : value.expanded;
  }

  import PendingCaptures from "./PendingCaptures.svelte";
  import SourceToolbar from "./SourceToolbar.svelte";
  import SourceReadingView from "./SourceReadingView.svelte";
  import SourceSegmentList from "./SourceSegmentList.svelte";
  import AssistantOperations from "./AssistantOperations.svelte";
  let sourceEditing = $state(false);
  let sourceEditBusy = $state(false);
  let sourceImageOpen = $state(false);
  let sourceEditor = $state<HTMLTextAreaElement>();
  async function toggleSourceEdit() {
    if (!current || active(current) || current.sensitive || sourceEditBusy)
      return;
    if (!sourceEditing) {
      sourceEditing = true;
      await tick();
      sourceEditor?.focus({ preventScroll: true });
      return;
    }
    sourceEditBusy = true;
    const id = current.id;
    try {
      if (
        draft !== (current.edited_text ?? current.original_text) ||
        sourceLanguage !== (current.source_language ?? "")
      ) {
        const updated = await invoke<Task>("source_update", {
          request: {
            task_id: id,
            expected_revision: current.source_revision ?? 0,
            text: draft,
            source_language: sourceLanguage,
            target_language: targetLanguage,
          },
        });
        updateSource(updated);
      }
      if (current?.id === id) sourceEditing = false;
    } catch (e) {
      error = String(e);
    } finally {
      sourceEditBusy = false;
    }
  }
  let explanationSelection = $state<{ start: number; end: number } | null>(
    null,
  );
  let manuallyCollapsed = false;
  function updateSource(task: Task, choose = false) {
    const known = tasks.find((item) => item.id === task.id);
    if (!known || known.revision <= task.revision)
      tasks = [...tasks.filter((item) => item.id !== task.id), task];
    if (choose) selected = task.id;
  }
  function chooseSourceRange(start: number, end: number) {
    explanationSelection = { start, end };
    const word = draft.slice(start, end).trim();
    if (
      vocabularyEligible &&
      word &&
      word.length <= 128 &&
      word.split(/\s+/u).length <= 3
    ) {
      const request = {
        word,
        sourceText: draft,
        selectionStart: start,
        selectionEnd: end,
        taskId: !changed && current?.status === "completed" ? current.id : null,
        attempt:
          !changed && current?.status === "completed" ? current.attempt : null,
      };
      selectedRequest = request;
      requestPreview(request, true);
    }
  }
  import ProviderLogo from "./ProviderLogo.svelte";
  import VocabularySidebar from "./VocabularySidebar.svelte";
  import TranslationContent from "./TranslationContent.svelte";
  import { translationPlainText } from "./reading";
  import Icon from "./Icons.svelte";
  import LoadingSpinner from "./LoadingSpinner.svelte";
  import { serviceName } from "./services";
  import { imageEntrance } from "./motion";
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
  let credentialChecking = $state(false);
  let credentialAuthorizing = $state(false);
  let credentialDialogError = $state("");
  let credentialDialog: HTMLDialogElement;
  let pendingCredential = $state<{ profileId: string; context: string } | null>(
    null,
  );
  let sourceLanguage = $state(""),
    targetOverride = $state<string | null>(null),
    detected = $state<string | null>(null),
    disclosed = $state(false);
  let content: HTMLElement;
  let current = $derived(tasks.find((task) => task.id === selected));
  const usesSourceAssets = $derived(
    !!current &&
      ((current.source_segments?.length ?? 0) > 1 ||
        current.source_segments?.some(
          (segment) => !!segment.asset_id && segment.asset_id !== current.id,
        )),
  );
  type DictionaryGuidance = {
    issue: "sentence" | "too_long" | null;
    profile_ids: string[];
  };
  let dictionaryGuidance = $state<DictionaryGuidance | null>(null);
  let dictionaryPending = $state(false);
  let dictionarySequence = 0;
  let profileOverride = $state<string | null>(null);
  let defaultProfileId = defaults.active_profile;
  let pendingManualProfile: string | null = null;
  let candidateProfileId = $state("");
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
        manuallyCollapsed = false;
        vocabularyOpen = false;
      }
      // Candidate arrival never expands the reading sidebar.
      void ready;
    });
  });
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
    current && (active(current) || current.sensitive)
      ? current.profile
      : settings.profiles.find(
          (p) => p.id === (profileOverride ?? settings.active_profile),
        ),
  );
  let localDictionary = $derived(service?.provider === "system_dictionary");
  let canStart = $derived(
    !!draft.trim() &&
      !setupBlocked &&
      !current?.sensitive &&
      (!current || !active(current)) &&
      !submitting &&
      !credentialChecking &&
      !credentialAuthorizing &&
      !(localDictionary && (dictionaryPending || dictionaryGuidance?.issue)),
  );
  // UI affordance only; preview and collection are validated again in core.
  let vocabularyEligible = $derived(
    (
      sourceLanguage ||
      detected ||
      (draft === (current?.edited_text ?? current?.original_text)
        ? current?.detected_language
        : "") ||
      ""
    )
      .toLowerCase()
      .split(/[-_]/)[0] === "en",
  );
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
  function attemptStartProfile(task: Task) {
    let profile = task.profile;
    // Consecutive fallback entries belong to this request, not a user changing
    // the selected service. Earlier manual attempts start a different request.
    for (let i = task.past_attempts.length - 1; i >= 0; i--) {
      const previous = task.past_attempts[i];
      if (previous.reason !== "failover" || !previous.profile) break;
      profile = previous.profile;
    }
    return profile;
  }
  const requestStartProfile = $derived(
    current ? attemptStartProfile(current) : null,
  );
  let changed = $derived(
    !!current &&
      ((!active(current) &&
        !current.sensitive &&
        (service?.id !== requestStartProfile?.id ||
          service?.revision !== requestStartProfile?.revision)) ||
        (!!current.translation &&
          (current.translation_source_revision ?? 0) !==
            (current.source_revision ?? 0)) ||
        draft !== (current.edited_text ?? current.original_text) ||
        sourceLanguage !== (current.source_language ?? "") ||
        targetLanguage !== current.target_language),
  );
  let dictionaryNotFound = $derived(
    localDictionary &&
      !changed &&
      !current?.sensitive &&
      [
        "系统词典未找到匹配词条。",
        "本机词典未找到释义，请在 macOS 词典中启用所需词典。",
      ].includes(current?.error ?? ""),
  );
  let dictionaryHint = $derived(
    localDictionary && !current?.sensitive && !sourceProcessing && draft.trim()
      ? dictionaryGuidance?.issue === "too_long"
        ? t(
            "查询内容过长，请缩短为单词或短语，或使用翻译服务。",
            "The query is too long. Shorten it to a word or phrase, or use a translation service.",
          )
        : dictionaryGuidance?.issue
          ? t(
              "系统词典用于查询单词或短语；整句或段落请使用翻译服务。",
              "The system dictionary looks up words and phrases. Use a translation service for sentences or paragraphs.",
            )
          : dictionaryNotFound
            ? t(
                "系统词典未找到匹配词条。",
                "No matching dictionary entry was found.",
              )
            : ""
      : "",
  );
  let translationChoices = $derived(
    settings.profiles.filter((profile) =>
      dictionaryGuidance?.profile_ids.includes(profile.id),
    ),
  );
  $effect(() => {
    const source = draft;
    const configuration = settings;
    const blocked = !!current?.sensitive || sourceProcessing;
    const enabled = localDictionary;
    const ticket = ++dictionarySequence;
    dictionaryGuidance = null;
    dictionaryPending = false;
    if (!enabled || blocked || !source.trim() || !isTauri()) return;
    dictionaryPending = true;
    invoke<DictionaryGuidance>("dictionary_guidance", { text: source })
      .then((guide) => {
        if (ticket !== dictionarySequence) return;
        dictionaryGuidance = guide;
        const ids = guide?.profile_ids ?? [];
        if (!ids.includes(candidateProfileId))
          candidateProfileId = ids[0] ?? "";
      })
      .catch(() => {
        // The authoritative query guard still applies if this optional preview fails.
      })
      .finally(() => {
        if (ticket === dictionarySequence) dictionaryPending = false;
      });
    void configuration;
  });
  $effect(() => {
    if (settings.active_profile !== defaultProfileId) {
      defaultProfileId = settings.active_profile;
      profileOverride = current?.sensitive ? current.profile.id : null;
    }
  });
  $effect(() => {
    if (
      profileOverride &&
      !settings.profiles.some((profile) => profile.id === profileOverride)
    )
      profileOverride = null;
  });
  function useTranslationService() {
    if (
      !translationChoices.some((profile) => profile.id === candidateProfileId)
    )
      return;
    profileOverride = candidateProfileId;
    disclosed = false;
    error = "";
    tick().then(() => trigger?.focus());
  }
  let attempted = $derived(
    !!current &&
      !current.sensitive &&
      !["ready", "original_only", "capturing", "recognizing"].includes(
        current.status,
      ),
  );
  const shownService = $derived(
    current && attempted && !changed ? current.profile : service,
  );
  let showResult = $derived(
    disclosed && !dictionaryHint && (submitting || (attempted && !changed)),
  );
  let toggleDisabled = $derived(
    (setupBlocked && (!current || waiting || changed)) ||
      !!current?.sensitive ||
      credentialChecking ||
      credentialAuthorizing ||
      (!!current && ["capturing", "recognizing"].includes(current.status)) ||
      (localDictionary && (dictionaryPending || !!dictionaryGuidance?.issue)) ||
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
    if (!vocabularyEligible) return [];
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
    const scope = `${current?.id ?? "draft"}:${current?.attempt ?? 0}:${draft}:${sourceLanguage}:${vocabularyEligible}`;
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
    dictionaryNotFound
      ? t("重新查询（D / S）", "Look up again (D / S)")
      : showResult
        ? localDictionary
          ? t("收起释义", "Collapse definition")
          : t("收起译文", "Collapse translation")
        : !current || waiting || changed
          ? localDictionary
            ? t("查询并展开（D / S）", "Look up and expand (D / S)")
            : t("翻译并展开（D / S）", "Translate and expand (D / S)")
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
    notice === "请先在设置中开启 Yurelay 的屏幕录制权限。"
      ? "screen"
      : notice === "请先在设置中开启 Yurelay 的辅助功能权限。"
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
  let draftId: string | null = null;
  let sourceVersion = "";
  let lastSourceText = "";
  let trigger = $state<HTMLButtonElement>();
  $effect(() => {
    const version = `${current?.id}:${current?.original_text}:${current?.edited_text}`;
    if (version !== sourceVersion) {
      sourceVersion = version;
      clearSelection();
      const nextText = current?.edited_text ?? current?.original_text ?? "";
      // A committed snapshot may arrive after a newer local edit. Keep that edit.
      if (
        (current?.id ?? "") !== draftId ||
        draft === lastSourceText ||
        draft === nextText
      )
        draft = nextText;
      lastSourceText = nextText;
    }
    const nextDraftId = current?.id ?? "";
    if (nextDraftId !== draftId) {
      draftId = nextDraftId;
      sourceEditing = current?.source === "manual";
      sourceImageOpen = false;
      if (
        !pendingManualProfile ||
        current?.profile.id !== pendingManualProfile
      ) {
        // New captures use the configured default. Existing results retain
        // their requested primary service, never an automatic fallback choice.
        profileOverride = current?.sensitive
          ? current.profile.id
          : current &&
              !["ready", "original_only", "capturing", "recognizing"].includes(
                current.status,
              )
            ? attemptStartProfile(current).id
            : null;
      }
      pendingManualProfile = null;
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
    explanationSelection = null;
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
    if (document.activeElement !== el) return;
    const word = el.value.slice(el.selectionStart, el.selectionEnd).trim();
    const explicitRange = word
      ? { start: el.selectionStart, end: el.selectionEnd }
      : null;
    explanationSelection = explicitRange;
    if (
      !vocabularyEligible ||
      !word ||
      word.length > 128 ||
      word.split(/\s+/u).length > 3 ||
      !/\p{Script=Latin}/u.test(word) ||
      !/^[\p{Script=Latin}\x21-\x7e\s’‘–—]+$/u.test(word)
    ) {
      clearSelection();
      explanationSelection = explicitRange;
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
          sourceLanguage: sourceLanguage || null,
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
    if (reveal && !manuallyCollapsed) {
      vocabularyOpen = true;
      vocabularyAutoShown = true;
    }
  }
  function selectWordAtCaret(event: MouseEvent) {
    if (event.button !== 0 || !vocabularyEligible) return;
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
      await refreshExperience();
    } catch {
      /* Initial bootstrap reports errors. */
    }
  }
  async function translate() {
    if (!canStart) return;
    const context = translationContext();
    const profileId = service?.id ?? profileOverride;
    credentialChecking = true;
    try {
      const required = await invoke<boolean>(
        "credential_authorization_required",
        { profileId },
      );
      if (context !== translationContext()) return;
      if (required && service) {
        pendingCredential = { profileId: service.id, context };
        credentialDialogError = "";
        credentialDialog.showModal();
        return;
      }
    } catch (e) {
      error = String(e);
      return;
    } finally {
      credentialChecking = false;
    }
    if (!current) {
      submitting = true;
      pendingManualProfile = profileId;
      await action("translate_text", {
        text: draft,
        sourceLanguage: sourceLanguage || null,
        targetLanguage,
        profileId,
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
      profileId,
    });
    if ((error || !isTauri()) && current?.id === id) submitting = false;
  }
  function translationContext() {
    return JSON.stringify([
      current?.id,
      current?.source_revision,
      draft,
      service?.id,
      service?.revision,
      sourceLanguage,
      targetLanguage,
    ]);
  }
  function cancelCredentialAuthorization() {
    if (credentialAuthorizing) return;
    pendingCredential = null;
    credentialDialog.close();
  }
  $effect(() => {
    const context = translationContext();
    if (pendingCredential && pendingCredential.context !== context) {
      pendingCredential = null;
      credentialDialog?.close();
    }
  });
  async function authorizeAndContinue() {
    const request = pendingCredential;
    if (
      !request ||
      credentialAuthorizing ||
      request.context !== translationContext()
    )
      return;
    credentialAuthorizing = true;
    credentialDialogError = "";
    let resume = false;
    try {
      await invoke("authorize_saved_credential", {
        profileId: request.profileId,
      });
      if (
        pendingCredential === request &&
        request.context === translationContext()
      ) {
        pendingCredential = null;
        credentialDialog.close();
        resume = true;
      }
    } catch (e) {
      credentialDialogError = errorText(String(e), locale);
    } finally {
      credentialAuthorizing = false;
    }
    if (resume) await translate();
  }
  function toggleTranslation() {
    if (toggleDisabled) return;
    if (dictionaryNotFound) {
      translate();
      return;
    }
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
      'input,textarea,select,[contenteditable]:not([contenteditable="false"]),[role="textbox"]:not([aria-readonly="true"]),[role="combobox"]';
    if (
      guideOpen ||
      !!document.querySelector("dialog[open]") ||
      event.defaultPrevented ||
      event.repeat ||
      event.isComposing ||
      event.ctrlKey ||
      event.altKey ||
      event.metaKey
    )
      return;
    if (event.key === "Escape") {
      // Give native menus and the vocabulary sidebar their own dismissal first.
      if (
        event.shiftKey ||
        (event.target as HTMLElement)?.closest('select,[role="combobox"]') ||
        document.activeElement?.closest('select,[role="combobox"]') ||
        (hasVocabulary && vocabularyOpen)
      )
        return;
      event.preventDefault();
      void action("hide_window");
      return;
    }
    if (
      (event.target as HTMLElement)?.closest(editing) ||
      document.activeElement?.closest(editing)
    )
      return;
    if (
      ["d", "s"].includes(event.key.toLowerCase()) &&
      draft.trim() &&
      !toggleDisabled
    ) {
      event.preventDefault();
      toggleTranslation();
    }
  }
  async function toggleExpanded() {
    if (resizing) return;
    const next = !expanded;
    resizing = true;
    await action("set_window_expanded", { expanded: next });
    // The native resize event is authoritative; it may arrive after this command.
    if (!error && !isTauri()) expanded = next;
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
          void refreshExperience().catch(() => {});
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
        if (data.window_sizing) windowMode(data.window_sizing);
        permissions = data.permissions;
        if (before === preferenceEvents) settings = data.configuration.settings;
        if (!data.selected) sourceLanguage = settings.source_language ?? "";
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
        const offMode = await listen<boolean | WindowSizing>(
          "window-mode-changed",
          (e) => windowMode(e.payload),
        );
        if (disposed) offMode();
        else cleanup.push(offMode);
        const offExperience = await listen<{
          progress: Experience;
          close_guide: boolean;
        }>("experience-changed", (e) => {
          if (experience) experience.progress = e.payload.progress;
          if (guideOpen && e.payload.close_guide) closeGuide();
        });
        if (disposed) offExperience();
        else cleanup.push(offExperience);
        await refreshExperience();
        if (
          !disposed &&
          experience &&
          ["new", "started"].includes(experience.progress.setup)
        )
          openGuide();
      } catch (e) {
        error = String(e);
      }
    })();
    return () => {
      disposed = true;
      experienceSequence++;
      previewSequence++;
      clearTimeout(previewTimer);
      clearInterval(timer);
      cleanup.forEach((off) => off());
      if (subscription)
        invoke("unsubscribe", { subscriptionId: subscription }).catch(() => {});
    };
  });
</script>

<svelte:window
  bind:innerWidth={viewportWidth}
  onfocus={refresh}
  onkeydown={keydown}
/>
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
        sourceText={draft}
        {sourceLanguage}
        open={sidebarWidth > 40}
        onToggle={() => {
          vocabularyOpen = !vocabularyOpen;
          if (!vocabularyOpen) manuallyCollapsed = true;
        }}
      />{/key}{/if}
  <div class="translation-main">
    <div class="window-chrome" onpointerdown={drag} role="presentation">
      <span class="window-name">Yurelay</span>
      <div class="window-tools">
        <button
          class="icon-button"
          aria-label={t("设置", "Settings")}
          title={t("设置（⌘,）", "Settings (⌘,)")}
          onclick={() => action("show_settings")}
          ><Icon name="settings" size={16} /></button
        >
        <button
          class="icon-button"
          aria-label={t("使用帮助", "Usage guide")}
          title={guideOpen
            ? t("关闭引导，稍后设置", "Close guide and set up later")
            : t("使用帮助", "Usage guide")}
          class:chosen={guideOpen}
          aria-expanded={guideOpen}
          aria-controls="reading-usage-guide"
          disabled={guideBusy}
          onclick={toggleGuide}><Icon name="help" size={16} /></button
        >
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
          title={t("收起（Esc）", "Hide (Esc)")}
          aria-keyshortcuts="Escape"
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
      <PendingCaptures
        task={current}
        {draft}
        {locale}
        onUpdate={(task, choose) => updateSource(task, choose)}
      />
      {#if guideOpen}<div id="reading-usage-guide">
          <GettingStarted
            bind:this={guideView}
            bind:busy={guideBusy}
            {settings}
            {locale}
            initialStep={guideStep}
            onclose={closeGuide}
            navigate={openSettings}
          />
        </div>{/if}
      <div
        class="reading-sheet"
        hidden={guideOpen}
        inert={guideOpen}
        use:fitReadingWorkspace={(height) => (pairedAvailable = height)}
        style:--paired-row-height={`${pairedRowHeight}px`}
      >
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
            {#if current && !current.sensitive && !sourceProcessing && draft.trim()}<button
                class="icon-button source-edit-control"
                aria-label={sourceEditing
                  ? t("完成编辑", "Finish editing")
                  : t("编辑原文", "Edit source")}
                title={sourceEditing
                  ? t("完成编辑", "Finish editing")
                  : t("编辑原文", "Edit source")}
                disabled={active(current) || sourceEditBusy}
                onclick={toggleSourceEdit}
                ><Icon
                  name={sourceEditing ? "check" : "edit"}
                  size={14}
                /></button
              >{/if}
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
                      "Yurelay 已在本地拦截该请求并隐藏您输入的原文，未发送至任何云端服务。",
                      "Yurelay blocked this request locally and hid your input. It has not been sent to any cloud service.",
                    )
                  : t(
                      "为保护您的资产与隐私安全，Yurelay 已在本地拦截该内容，未将其展示或发送至任何云端服务。",
                      "Yurelay blocked this content locally. It has not been displayed or sent to any cloud service.",
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
            {#if current}<SourceToolbar
                task={current}
                {draft}
                {sourceLanguage}
                {targetLanguage}
                {locale}
                onUpdate={updateSource}
              />{#if usesSourceAssets}<SourceSegmentList
                  task={current}
                  {locale}
                  editable={!active(current)}
                  onUpdate={updateSource}
                />{/if}{/if}
            {#if current?.has_image && !usesSourceAssets}<details
                class="source-image"
                ontoggle={(event) =>
                  (sourceImageOpen = event.currentTarget.open)}
              >
                <summary>{t("查看截图", "View screenshot")}</summary
                >{#if sourceImageOpen}<img
                    use:imageEntrance
                    src={convertFileSrc(current.id, "yurelay-image")}
                    alt={t("原始截图选区", "Original screenshot selection")}
                  />{/if}
              </details>{/if}
            <ResizableReading
              id="source-reading-viewport"
              linkedHeight={paired ? pairedRowHeight : null}
              linkedMaximum={pairedAvailable}
              onLinkedResize={resizePair}
              disabled={readingLayoutLoading}
              onCommit={(height, viewport) =>
                saveReadingHeight("source", height, viewport)}
              bind:viewportHeight={sourceViewportHeight}
              bind:height={sourceHeight}
              label={t(
                paired
                  ? "调整原文高度（宽屏左右联动；拖动或上下键；双击铺满）"
                  : "调整原文高度（拖动或上下键；双击恢复）",
                paired
                  ? "Resize source height (both columns; drag or arrow keys; double-click to fill)"
                  : "Resize source height (drag or arrow keys; double-click to reset)",
              )}
            >
              {#if !sourceEditing && current && !sourceProcessing && !!draft.trim()}
                <SourceReadingView
                  task={current}
                  text={draft}
                  {locale}
                  enabled={settings.encounter_highlight !== false &&
                    vocabularyEligible}
                  onSelect={chooseSourceRange}
                />
              {:else}<div class="source-editor-frame">
                  <textarea
                    class="source-editor"
                    bind:this={sourceEditor!}
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
                          "输入或粘贴文字，也可划词或截图…",
                          "Type or paste text, or capture a selection…",
                        )}
                    spellcheck="false"></textarea>
                  {#if sourceProcessing}<div
                      class="source-processing"
                      id="source-processing-status"
                      role="status"
                      aria-live="polite"
                    >
                      <LoadingSpinner size={20} />
                      <span
                        >{current?.status === "capturing"
                          ? t("正在准备截图…", "Preparing screenshot…")
                          : t(
                              "正在识别截图文字…",
                              "Recognizing screenshot text…",
                            )}</span
                      >
                    </div>{/if}
                </div>{/if}
            </ResizableReading>
          {/if}
          {#if current?.image_error}<p class="inline-feedback">
              {errorText(current.image_error, locale)}
            </p>{/if}
        </section>
        <div class="service-strip">
          <button
            onclick={() =>
              openSettings({
                section: "services",
                profile_id: shownService?.id,
              })}
            aria-label={setupBlocked
              ? t("配置翻译服务", "Configure translation service")
              : t("更改服务或模型", "Change service or model")}
            class="service-identity"
            title={`${shownService?.name ?? ""} · ${shownService?.model ?? ""}`}
          >
            <span class="provider-mark"
              >{#if shownService}<ProviderLogo
                  provider={shownService.provider}
                  size={18}
                />{/if}</span
            ><span class="provider-name" title={shownService?.name}
              >{shownService
                ? serviceName(shownService, locale)
                : t("未配置服务", "No service")}</span
            ><span class="model-name" title={shownService?.model}
              >{shownService?.provider === "system_dictionary"
                ? t("本机词典", "Local dictionary")
                : shownService?.provider === "youdao"
                  ? t("文本翻译", "Text translation")
                  : shownService?.model ||
                    t("选择模型", "Choose a model")}</span
            >
          </button>
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
                  ><LoadingSpinner /></button
                ><span
                  class="progress-tooltip"
                  id="translation-stop-tip"
                  role="tooltip">{t("停止翻译", "Stop translation")}</span
                >
              </div>{/if}
            {#if !current || waiting || changed || dictionaryNotFound}
              <button
                bind:this={trigger!}
                class="icon-button disclosure-control labeled"
                class:opened={showResult}
                title={triggerLabel}
                aria-label={triggerLabel}
                aria-expanded={showResult}
                aria-controls="translation-body"
                aria-keyshortcuts="D S"
                disabled={toggleDisabled}
                onclick={translate}
                >{localDictionary
                  ? t("查询", "Look up")
                  : t("翻译", "Translate")}<Icon
                  name="chevron"
                  size={15}
                /></button
              >
            {:else}
              <button
                bind:this={trigger!}
                class="icon-button disclosure-control labeled"
                class:opened={showResult}
                title={triggerLabel}
                aria-label={triggerLabel}
                aria-expanded={showResult}
                aria-controls="translation-body"
                aria-keyshortcuts="D S"
                disabled={toggleDisabled}
                onclick={() => (disclosed = !disclosed)}
                >{showResult ? t("收起", "Collapse") : t("展开", "Expand")}<Icon
                  name="chevron"
                  size={15}
                /></button
              >
            {/if}
          </div>
        </div>
        {#if setupBlocked}<div class="service-setup-hint" role="status">
            <p>
              {serviceStatus?.state === "needs_key"
                ? t(
                    "此服务还需要 API 凭据。保存后即可开始翻译。",
                    "This service needs API credentials. Save them before translating.",
                  )
                : t(
                    "尚未配置可用服务。可以选择系统词典查词，或配置翻译 API。",
                    "No service is configured. Choose the system dictionary for words, or configure a translation API.",
                  )}
            </p>
            <button
              onclick={() =>
                openSettings({ section: "services", profile_id: service?.id })}
              >{t("配置服务", "Set up a service")}</button
            ><button onclick={openGuide}>{t("查看引导", "View guide")}</button>
          </div>{/if}
        {#if dictionaryHint}<section
            class="dictionary-guidance"
            aria-label={t("词典查询提示", "Dictionary lookup guidance")}
          >
            <p role="status">{dictionaryHint}</p>
            {#if dictionaryNotFound}<p class="dictionary-guidance-detail">
                {t(
                  "可调整词形后重新查询，或选择翻译服务。",
                  "Try another word form, or choose a translation service.",
                )}
              </p>{/if}
            {#if translationChoices.length}
              <div class="dictionary-service-choice">
                <label
                  >{t("翻译服务", "Translation service")}
                  <select bind:value={candidateProfileId}>
                    {#each translationChoices as profile}<option
                        value={profile.id}
                        >{serviceName(profile, locale)}{profile.model
                          ? ` · ${profile.model}`
                          : ""}</option
                      >{/each}
                  </select>
                </label>
                <button
                  class="subtle-button"
                  onclick={useTranslationService}
                  disabled={!translationChoices.some(
                    (p) => p.id === candidateProfileId,
                  )}>{t("使用此服务", "Use this service")}</button
                >
              </div>
              <p class="dictionary-guidance-detail">
                {t(
                  "切换仅用于当前查询；确认后按 D / S 或点击翻译按钮开始。",
                  "This changes only the current query. Then press D / S or click Translate to start.",
                )}
              </p>
            {:else if !dictionaryPending}
              <p class="dictionary-guidance-detail">
                {t(
                  "暂无可选的翻译服务，请在设置的“服务”中检查配置。",
                  "No translation service can be selected. Check its configuration under Services in Settings.",
                )}
              </p>
              <button
                class="subtle-button"
                onclick={() => action("show_settings")}
                >{t("打开设置", "Open settings")}</button
              >
            {/if}
          </section>{/if}
        <div class="translation-output" class:has-result={showResult}>
          <div class="reading-caption output-caption">
            <h2>
              {localDictionary
                ? t("释义", "Definition")
                : t("译文", "Translation")}
            </h2>
            <span
              >{localDictionary
                ? t("本机查询", "Local lookup")
                : languageName(targetLanguage, locale)}</span
            >
          </div>
          {#if showResult}<section
              id="translation-body"
              class="translation-body"
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
              {#if current?.error?.includes("密钥需要授权") || current?.error?.includes("API Key 需要系统钥匙串授权")}
                <button
                  class="subtle-button"
                  disabled={!canStart}
                  onclick={translate}
                >
                  {t("授权已保存密钥", "Authorize saved key")}
                </button>
              {/if}
              <ResizableReading
                id="translation-reading-viewport"
                linkedHeight={paired ? pairedRowHeight : null}
                linkedMaximum={pairedAvailable}
                onLinkedResize={resizePair}
                disabled={readingLayoutLoading}
                onCommit={(height, viewport) =>
                  saveReadingHeight("translation", height, viewport)}
                bind:viewportHeight={translationViewportHeight}
                bind:height={translationHeight}
                label={t(
                  paired
                    ? "调整译文高度（宽屏左右联动；拖动或上下键；双击铺满）"
                    : "调整译文高度（拖动或上下键；双击恢复）",
                  paired
                    ? "Resize translation height (both columns; drag or arrow keys; double-click to fill)"
                    : "Resize translation height (drag or arrow keys; double-click to reset)",
                )}
              >
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
                      ? t(
                          "暂无释义，可重新查询",
                          "No definition yet. Try again.",
                        )
                      : t("尚无译文，可重试", "No translation yet. Try again.")}
                  </p>{/if}
              </ResizableReading>
            </section>{/if}
          {#if !showResult}<div class="reading-empty">
              <span class="empty-reading-mark" aria-hidden="true"
                ><Icon name="translate" size={24} /></span
              >
              <p>
                {current?.translation
                  ? t("译文已收起", "Translation collapsed")
                  : localDictionary
                    ? t("释义将在这里呈现", "Your definition appears here")
                    : t("译文将在这里呈现", "Your translation appears here")}
              </p>
              <span
                >{current?.translation
                  ? t(
                      "点击展开，继续对照阅读。",
                      "Expand to continue reading side by side.",
                    )
                  : t(
                      "准备好原文，再开始。",
                      "Start when your source is ready.",
                    )}</span
              >
            </div>{/if}
        </div>
        {#if showResult}<div class="reading-output-actions">
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
          </div>{/if}
        {#if changed && current?.translation && !active(current)}<details
            class="previous-translation"
            open
          >
            <summary
              >{t("上一版结果", "Previous result")} · {current.profile.name} · v{current.translation_source_revision ??
                current.source_revision ??
                0}</summary
            >
            <p class="workflow-note">
              {t(
                "原文、语言或服务已变更；此结果不会自动更新。",
                "The source, language or service changed. This result is retained until you explicitly translate again.",
              )}
            </p>
            <TranslationContent
              text={current.translation}
              {locale}
              identity={`${current.id}:${current.attempt}:previous`}
            />
          </details>{/if}
        <AssistantOperations
          {locale}
          task={current}
          sourceText={draft}
          {sourceLanguage}
          {targetLanguage}
          detectedLanguage={detected}
          {settings}
          profileId={profileOverride ?? settings.active_profile}
          selection={explanationSelection}
          onProfileChange={(id) => (profileOverride = id)}
        />
        <span class="sr-only" role="status" aria-hidden={sourceProcessing}
          >{current ? statusName(current.status, locale) : ""}</span
        >
      </div>
      {#if !guideOpen && waiting && !submitting && !current?.sensitive && !dictionaryHint}<div
          class="capture-hint"
        >
          <span
            >{localDictionary
              ? t("检查原文后开始查询", "Review the word before looking it up")
              : t(
                  "检查原文后开始翻译",
                  "Review the source before translating",
                )}</span
          ><kbd>D / S</kbd>
        </div>{/if}
    </main>
  </div>
</div>

<dialog
  bind:this={credentialDialog!}
  class="keychain-dialog"
  aria-labelledby="keychain-title"
  aria-describedby="keychain-description"
  oncancel={(event) => {
    event.preventDefault();
    cancelCredentialAuthorization();
  }}
>
  <div class="close-dialog-core">
    <h2 id="keychain-title">
      {t("授权读取已保存的 API Key", "Authorize saved API key")}
    </h2>
    <p id="keychain-description">
      {t(
        "你的 API Key 已加密保存在 macOS 系统钥匙串中，无需重新填写。继续后将由 macOS 请求访问授权；若提示输入密码，请输入钥匙串密码，通常是 Mac 登录密码。",
        "Your API key is encrypted in macOS Keychain. You do not need to enter it again. macOS will request access next; if asked for a password, use your keychain password, usually your Mac login password.",
      )}
    </p>
    {#if credentialDialogError}<p class="dialog-error" role="alert">
        {credentialDialogError}
      </p>{/if}
    <div class="close-dialog-actions">
      <button
        disabled={credentialAuthorizing}
        onclick={cancelCredentialAuthorization}>{t("取消", "Cancel")}</button
      >
      <button
        class="primary"
        disabled={credentialAuthorizing}
        onclick={authorizeAndContinue}
        >{credentialAuthorizing
          ? t("等待系统授权…", "Waiting for authorization…")
          : t("授权并继续", "Authorize and continue")}</button
      >
    </div>
  </div>
</dialog>
