<script lang="ts">
  import { onMount, tick } from "svelte";
  import { panelEntrance } from "./motion";
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import {
    defaults,
    type Settings,
    type Bootstrap,
    type Configuration,
    type Profile,
  } from "./types";
  import {
    localeFor,
    text,
    applyAppearance,
    languages,
    primaryCode,
    errorText,
  } from "./locale";
  import GettingStarted from "./GettingStarted.svelte";
  import type { SettingsRoute } from "./types";
  import ServiceVerificationBadge from "./ServiceVerificationBadge.svelte";
  import PromptSettings from "./PromptSettings.svelte";
  import ServicePromptReferences from "./ServicePromptReferences.svelte";
  import ReliabilitySettings from "./ReliabilitySettings.svelte";
  let verificationVersion = $state(0);
  import ProviderLogo from "./ProviderLogo.svelte";
  import VocabularyPanel from "./VocabularyPanel.svelte";
  import promptTemplate from "../crates/core/data/translation-prompt.txt?raw";
  import passageRules from "../crates/core/data/translation-passage.txt?raw";
  import outputFormat from "../crates/core/data/vocabulary-format.txt?raw";
  import translationOnly from "../crates/core/data/translation-only-format.txt?raw";
  let defaultPrompt = $derived.by(() => {
    const extract = settings.auto_vocabulary !== false;
    const rules = passageRules.trim();
    return promptTemplate
      .replace("{{mode_rules}}", extract ? rules : rules.split("\n")[0])
      .replace(
        "{{output_format}}",
        (extract ? outputFormat : translationOnly).trim(),
      );
  });
  import {
    matchesService,
    modelChoices,
    modelPriceLabel,
    type RemoteModel,
  } from "./services";
  let serviceQuery = $state("");
  import HistoryPanel from "./HistoryPanel.svelte";
  import Icon from "./Icons.svelte";
  import logo from "./assets/yurelay-logo.png";
  import {
    servicePresets,
    presetFor,
    serviceName,
    serviceFamily,
    serviceEntries,
    usesModel,
  } from "./services";
  import ShortcutRecorder from "./ShortcutRecorder.svelte";
  import { listen } from "@tauri-apps/api/event";
  type Nav =
    | "translation"
    | "services"
    | "prompts"
    | "vocabulary"
    | "history"
    | "appearance"
    | "help";
  let nav = $state<Nav>("translation");
  let replyPromptTemplate = $state(""),
    replyPromptLoading = $state(false),
    replyPromptError = $state("");
  let replyPromptLoaded = false;
  $effect(() => {
    if (
      !["prompts", "services"].includes(nav) ||
      replyPromptLoaded ||
      !isTauri()
    )
      return;
    replyPromptLoaded = true;
    replyPromptLoading = true;
    invoke<string>("default_reply_prompt")
      .then((value) => {
        replyPromptTemplate = typeof value === "string" ? value : "";
      })
      .catch((e) => {
        replyPromptError = errorText(e, locale);
      })
      .finally(() => (replyPromptLoading = false));
  });
  let settings = $state<Settings>(structuredClone(defaults));
  let saved = $state(JSON.stringify(defaults));
  type CloseRequest = { id: number; intent: "close" | "quit" };
  let closeRequest = $state<CloseRequest | null>(null);
  let closeDialog: HTMLDialogElement;
  let closeSaving = $state(false);
  async function finishClose(request: CloseRequest) {
    await invoke("finish_settings_close", { requestId: request.id });
    closeRequest = null;
    closeDialog?.close();
  }
  async function requestClose(request: CloseRequest) {
    closeRequest = request;
    if (!dirty) return;
    await invoke("show_settings");
    await tick();
    if (!closeDialog.open) closeDialog.showModal();
  }
  async function cancelClose() {
    if (!closeRequest) return;
    await invoke("cancel_settings_close", { requestId: closeRequest.id });
    closeRequest = null;
    closeDialog.close();
  }
  async function saveAndClose() {
    if (!closeRequest || closeSaving) return;
    closeSaving = true;
    const request = closeRequest;
    try {
      if (await save()) await finishClose(request);
    } catch (e) {
      error = errorText(e, locale);
    } finally {
      closeSaving = false;
    }
  }
  async function discardAndClose() {
    if (!closeRequest || busy) return;
    closeSaving = true;
    const request = closeRequest;
    try {
      revert();
      await finishClose(request);
    } catch (e) {
      error = errorText(e, locale);
    } finally {
      closeSaving = false;
    }
  }
  $effect(() => {
    const request = closeRequest;
    if (request && !dirty && (!busy || !ready) && !closeSaving) {
      closeSaving = true;
      finishClose(request)
        .catch((e) => (error = errorText(e, locale)))
        .finally(() => (closeSaving = false));
    }
  });
  let configuration = $state<Configuration>({
    settings: structuredClone(defaults),
    has_keys: [],
    credential_errors: {},
  });
  let permissions = $state({ accessibility: false, screen: false });
  let selected = $state("default");
  let historyVersion = $state(0);
  let profile = $derived(
    settings.profiles.find((p) => p.id === selected) ?? settings.profiles[0],
  );
  let keys = $state<Record<string, string | null>>({});
  let models = $state<RemoteModel[]>([]);
  let onlyFreeModels = $state(true);
  let choices = $derived(modelChoices(profile.provider, models));
  let visibleModels = $derived(
    profile.provider === "openrouter" && onlyFreeModels
      ? choices.filter((m) => m.pricing === "free")
      : choices,
  );
  let currentModelChoice = $derived(
    choices.find((m) => m.id === profile.model.trim()),
  );
  let error = $state(""),
    message = $state(""),
    busy = $state(false),
    ready = $state(false);
  let recordingField = $state<string | null>(null);
  let shortcutError = $state<string | null>(null);
  let locale = $derived(localeFor(settings));
  let dirty = $derived(
    JSON.stringify(settings) !== saved || Object.keys(keys).length > 0,
  );
  const t = (zh: string, en: string) => text(locale, zh, en);
  let pendingRoute: SettingsRoute | null = null;
  let quickZhipu = $state<string | null>(null);
  async function navigateRoute(route: SettingsRoute) {
    if (!ready) {
      pendingRoute = route;
      return;
    }
    // Navigation changes only the active pane; unsaved fields and keys survive.
    nav = route.section;
    quickZhipu = null;
    if (route.zhipu) {
      add("zhipu");
      const chosen = settings.profiles.find((p) => p.id === selected);
      if (
        chosen?.provider === "zhipu" &&
        chosen.model === presets.zhipu.model
      ) {
        quickZhipu = chosen.id;
        settings.active_profile = chosen.id;
      } else {
        message = t(
          "保留已有服务配置；可在此查看或修改。",
          "Existing service configuration was preserved. Review or edit it here.",
        );
      }
    } else if (route.translation) {
      add("custom");
      if (
        settings.profiles.find((p) => p.id === selected)?.provider !== "custom"
      ) {
        error = t(
          "服务数量已达上限，请先移除不再使用的服务。",
          "Service limit reached. Remove an unused service first.",
        );
      }
    } else if (route.dictionary) {
      add("system_dictionary");
      if (
        settings.profiles.find((p) => p.id === selected)?.provider ===
        "system_dictionary"
      )
        settings.active_profile = selected;
      if (
        settings.profiles.find((p) => p.id === selected)?.provider !==
        "system_dictionary"
      ) {
        error = t(
          "服务数量已达上限，请先移除不再使用的服务。",
          "Service limit reached. Remove an unused service first.",
        );
        return;
      }
      message = t(
        "已选择系统词典，请点击“保存设置”应用。",
        "System dictionary selected. Click Save changes to apply.",
      );
    } else if (
      route.profile_id &&
      settings.profiles.some((p) => p.id === route.profile_id)
    ) {
      if (selected !== route.profile_id) models = [];
      selected = route.profile_id;
    }
    await tick();
    document.querySelector<HTMLElement>(".settings-heading-inner h1")?.focus();
  }
  async function consumeRoute() {
    const route = await invoke<SettingsRoute | null>("take_settings_route");
    if (route) await navigateRoute(route);
  }
  onMount(() => {
    if (!isTauri()) return;
    let disposed = false;
    const listener = listen("settings-navigate", () => {
      void consumeRoute().catch((e) => (error = errorText(e, locale)));
    });
    listener.then((off) => {
      if (disposed) off();
    });
    void listener
      .then(() => consumeRoute())
      .catch((e) => (error = errorText(e, locale)));
    return () => {
      disposed = true;
      listener.then((off) => off());
    };
  });
  const sections: {
    id: Nav;
    icon: string;
    zh: string;
    en: string;
    description: [string, string];
  }[] = [
    {
      id: "help",
      icon: "help",
      zh: "使用帮助",
      en: "Getting started",
      description: [
        "权限、服务与基础操作。",
        "Permissions, services and basic operations.",
      ],
    },
    {
      id: "translation",
      icon: "translate",
      zh: "翻译与快捷键",
      en: "Translation",
      description: [
        "确定翻译成什么语言，以及如何随手取词。",
        "Choose your target language and how you capture text.",
      ],
    },
    {
      id: "services",
      icon: "service",
      zh: "服务",
      en: "Services",
      description: [
        "连接你自己的模型服务，密钥保存在系统钥匙串。",
        "Connect your own model services. Keys stay in the system keychain.",
      ],
    },
    {
      id: "prompts",
      icon: "edit",
      zh: "提示词",
      en: "Prompts",
      description: [
        "集中调整翻译和回复的表达方式。",
        "Set shared instructions for translation and reply writing.",
      ],
    },
    {
      id: "vocabulary",
      icon: "book",
      zh: "生词本",
      en: "Vocabulary",
      description: [
        "在划词与长句翻译中沉淀你的专属词汇库，支持一词多境复习。",
        "Build your vocabulary through words and sentences, with every context in one place.",
      ],
    },
    {
      id: "history",
      icon: "history",
      zh: "历史记录",
      en: "History",
      description: [
        "回看每一次划词、截图和翻译。",
        "Revisit your selections, screenshots, and translations.",
      ],
    },
    {
      id: "appearance",
      icon: "display",
      zh: "界面与显示",
      en: "Appearance",
      description: [
        "让语言、字号和窗口适合你的阅读习惯。",
        "Make the language, type, and windows work for you.",
      ],
    },
  ];
  let section = $derived(sections.find((s) => s.id === nav)!);
  const presets = Object.fromEntries(
    servicePresets.map((p) => [p.provider, p]),
  );
  presets.minimax_china = {
    ...presets.minimax,
    endpoint: "https://api.minimax.cn/v1",
  };
  function configured(item: Profile) {
    if (item.provider === "system_dictionary") return true;
    const key =
      keys[item.id] !== null &&
      (!!keys[item.id]?.trim() || configuration.has_keys.includes(item.id));
    return item.provider === "youdao"
      ? key && !!item.app_id?.trim()
      : !!item.model.trim() &&
          (key ||
            (item.provider === "custom" &&
              /^http:\/\/(localhost|127\.0\.0\.1|\[::1\])(:|\/)/.test(
                item.endpoint,
              )));
  }
  $effect(() => {
    applyAppearance(settings);
  });
  async function run(work: () => Promise<void>) {
    busy = true;
    error = "";
    message = "";
    try {
      await work();
      return true;
    } catch (e) {
      error = errorText(e, locale);
      return false;
    } finally {
      busy = false;
    }
  }
  function add(provider: Profile["provider"]) {
    if (settings.profiles.length >= 20) return;
    const family = serviceFamily;
    const existing = settings.profiles.find(
      (p) => family(p.provider) === family(provider),
    );
    if (existing) {
      selected = existing.id;
      models = [];
      return;
    }
    const preset = presets[provider];
    const id = crypto.randomUUID();
    settings.profiles.push({
      ...structuredClone(defaults.profiles[0]),
      id,
      provider,
      name: preset
        ? locale === "zh"
          ? preset.zh
          : preset.name
        : t("自定义服务", "Custom service"),
      app_id: "",
      streaming: !["youdao", "system_dictionary"].includes(provider),
      endpoint: preset?.endpoint ?? "",
      model: preset?.model ?? "",
    });
    const active = settings.profiles.find(
      (p) => p.id === settings.active_profile,
    );
    if (active && !active.endpoint && !active.model)
      settings.active_profile = id;
    selected = id;
    models = [];
  }
  function remove() {
    if (settings.profiles.length <= 1) return;
    settings.profiles = settings.profiles.filter((p) => p.id !== selected);
    delete keys[selected];
    if (settings.active_profile === selected)
      settings.active_profile = settings.profiles[0].id;
    selected = settings.profiles[0].id;
    models = [];
  }
  function keyOverride() {
    return selected in keys ? (keys[selected] ?? "") : null;
  }
  let testEpoch = 0;
  let testStatus = $state<HTMLDivElement>();
  let testFeedback = $state<{
    profileId: string;
    state: "testing" | "success" | "error";
    message: string;
  } | null>(null);
  $effect(() => {
    void JSON.stringify(profile);
    void keys[selected];
    testEpoch++;
    testFeedback = null;
  });
  async function testService(key: string | null) {
    const epoch = ++testEpoch;
    const id = selected;
    const unsaved = dirty;
    testFeedback = {
      profileId: id,
      state: "testing",
      message: t("正在连接，请稍候…", "Connecting, please wait…"),
    };
    await tick();
    testStatus?.scrollIntoView({ block: "nearest" });
    try {
      await invoke("test_connection", {
        profile: $state.snapshot(profile),
        keyOverride: key,
      });
      if (epoch === testEpoch)
        testFeedback = {
          profileId: id,
          state: "success",
          message: unsaved
            ? t(
                "当前表单测试成功。请点击保存，让修改生效。",
                "This form passed the test. Save changes to apply it.",
              )
            : t(
                "已保存的配置测试成功，可以使用。",
                "Your saved configuration passed the test and is ready to use.",
              ),
        };
    } catch (e) {
      if (epoch === testEpoch)
        testFeedback = {
          profileId: id,
          state: "error",
          message: errorText(e, locale),
        };
    } finally {
      verificationVersion++;
      await tick();
      if (epoch === testEpoch) testStatus?.scrollIntoView({ block: "nearest" });
    }
  }
  async function save() {
    return await run(async () => {
      const result = await invoke<{
        configuration: Configuration;
        shortcuts: { error: string | null };
      }>("save_configuration", {
        settings: $state.snapshot(settings),
        apiKeyUpdates: $state.snapshot(keys),
      });
      configuration = result.configuration;
      settings = structuredClone(result.configuration.settings);
      saved = JSON.stringify(settings);
      historyVersion++;
      verificationVersion++;
      keys = {};
      shortcutError = result.shortcuts.error;
      message = "";
    });
  }
  async function authorizeSavedKey() {
    const id = selected;
    await run(async () => {
      await invoke("authorize_saved_credential", { profileId: id });
      const credentialErrors = { ...configuration.credential_errors };
      delete credentialErrors[id];
      configuration = { ...configuration, credential_errors: credentialErrors };
      message = t("已授权读取已保存的密钥", "Saved key access authorized");
    });
  }
  function revert() {
    settings = structuredClone(JSON.parse(saved));
    keys = {};
    selected = settings.active_profile;
    models = [];
    error = "";
    message = t("已还原未保存的修改", "Unsaved changes discarded");
  }
  let permissionSequence = 0;
  let permissionRecoveryOpen = $state(false);
  async function refreshPermissions() {
    if (!isTauri()) return;
    const sequence = ++permissionSequence;
    const next = await invoke<typeof permissions>("permissions");
    if (sequence === permissionSequence) permissions = next;
  }
  async function request(kind: "accessibility" | "screen") {
    if (!permissions[kind]) permissionRecoveryOpen = true;
    await run(async () => {
      await invoke("request_permission", { kind });
      await refreshPermissions();
    });
  }
  onMount(() => {
    if (!isTauri()) return;
    let disposed = false;
    const refresh = () => {
      if (!document.hidden) refreshPermissions().catch(() => {});
    };
    window.addEventListener("focus", refresh);
    document.addEventListener("visibilitychange", refresh);
    const timer = setInterval(() => {
      if (!permissions.accessibility || !permissions.screen) refresh();
    }, 1500);
    const watcher = listen("settings-focused", refresh);
    watcher.then((off) => {
      if (disposed) off();
    });
    return () => {
      disposed = true;
      permissionSequence++;
      clearInterval(timer);
      window.removeEventListener("focus", refresh);
      document.removeEventListener("visibilitychange", refresh);
      watcher.then((off) => off());
    };
  });
  onMount(() => {
    let disposed = false;
    const watcher = isTauri()
      ? listen<{ registered: boolean; error: string | null }>(
          "shortcut-status-changed",
          ({ payload }) => {
            shortcutError = payload.error;
          },
        )
      : null;
    watcher?.then((off) => {
      if (disposed) off();
    });
    return () => {
      disposed = true;
      watcher?.then((off) => off());
    };
  });
  onMount(() => {
    if (!isTauri()) return;
    let disposed = false;
    let off: (() => void) | undefined;
    listen<CloseRequest>("settings-close-requested", (e) => {
      requestClose(e.payload).catch((err) => (error = errorText(err, locale)));
    }).then(async (unlisten) => {
      if (disposed) {
        unlisten();
        return;
      }
      off = unlisten;
      await invoke("settings_guard_ready");
    });
    const beforeUnload = (event: BeforeUnloadEvent) => {
      if (dirty) {
        event.preventDefault();
        event.returnValue = "";
      }
    };
    window.addEventListener("beforeunload", beforeUnload);
    return () => {
      disposed = true;
      off?.();
      window.removeEventListener("beforeunload", beforeUnload);
    };
  });
  onMount(() => {
    if (!isTauri()) {
      ready = true;
      message = t(
        "界面预览，保存与系统功能需在桌面应用中使用。",
        "UI preview. Saving and system features require the desktop app.",
      );
      return;
    }
    run(async () => {
      const permissionVersion = permissionSequence;
      const data = await invoke<Bootstrap>("bootstrap");
      configuration = data.configuration;
      settings = structuredClone(data.configuration.settings);
      settings.target_language = primaryCode(settings.target_language);
      settings.source_language ??= "";
      saved = JSON.stringify(settings);
      selected = settings.active_profile;
      if (permissionVersion === permissionSequence)
        permissions = data.permissions;
      shortcutError = data.shortcuts.error;
      ready = true;
      if (pendingRoute) {
        const route = pendingRoute;
        pendingRoute = null;
        await navigateRoute(route);
      }
    });
  });
</script>

{#snippet serviceTestFeedback()}
  {#if testFeedback?.profileId === selected}<div
      bind:this={testStatus}
      class="service-test-feedback"
      data-state={testFeedback.state}
      role="status"
      aria-live="polite"
    >
      <strong
        >{testFeedback.state === "testing"
          ? t("正在测试连接…", "Testing connection…")
          : testFeedback.state === "success"
            ? t("✓ 连接成功", "✓ Connection successful")
            : t("连接失败", "Connection failed")}</strong
      >
      <p>{testFeedback.message}</p>
    </div>{/if}
{/snippet}

<div
  class="preferences-shell grid min-h-[100dvh] grid-cols-1 grid-rows-[auto_minmax(0,1fr)] md:grid-cols-[192px_minmax(0,1fr)] md:grid-rows-1"
>
  <aside
    class="border-b border-[var(--line)] bg-[var(--pref-sidebar)] p-4 md:sticky md:top-0 md:flex md:h-[100dvh] md:flex-col md:border-r md:border-b-0 md:p-5"
  >
    <div class="mb-5 flex items-center gap-3 px-2 pt-1 md:mb-10">
      <img
        class="settings-brand-logo"
        src={logo}
        width="36"
        height="36"
        alt=""
      />
      <div>
        <strong class="block text-base tracking-tight">Yurelay</strong><span
          class="subtle">{t("设置", "Settings")}</span
        >
      </div>
    </div>
    <nav
      aria-label={t("设置导航", "Settings navigation")}
      class="grid grid-cols-2 gap-1 md:grid-cols-1"
    >
      {#each sections as item}<button
          class="nav-item flex items-center gap-3 rounded-lg px-3 py-3"
          aria-current={nav === item.id ? "page" : undefined}
          onclick={() => {
            nav = item.id;
            error = "";
            message = "";
          }}
          ><Icon name={item.icon} size={19} /><span
            >{locale === "zh" ? item.zh : item.en}</span
          ></button
        >{/each}
    </nav>
    <div class="settings-sidebar-footer">
      <span class="settings-tagline"
        >{t("你的语言，你的节奏。", "Your words. Your pace.")}</span
      ><span class="settings-version">Yurelay 0.1</span>
    </div>
  </aside>
  <div class="flex min-h-0 min-w-0 flex-col overflow-hidden">
    <header class="settings-header">
      <div class="settings-heading-inner">
        <h1 tabindex="-1">{locale === "zh" ? section.zh : section.en}</h1>
        {#if nav === "history" || nav === "appearance"}<p
            class="subtle mt-3 max-w-[65ch]"
          >
            {section.description[locale === "zh" ? 0 : 1]}
          </p>{/if}
      </div>
    </header>
    <main
      class="settings-content m-0 min-h-0 w-full flex-1 overflow-y-auto px-6 md:px-7"
      class:workspace-content={nav === "services" || nav === "vocabulary"}
    >
      {#if error}<div class="mb-5">
          <p class="error" role="alert">{error}</p>
          {#if !ready}<button class="mt-3" onclick={() => location.reload()}
              >{t("重试加载", "Retry loading")}</button
            >{/if}
        </div>{/if}
      {#if message}<p class="subtle mb-5" role="status">{message}</p>{/if}
      {#if !ready}<div
          class="space-y-6"
          aria-busy="true"
          aria-label={t("正在加载设置", "Loading settings")}
        >
          <div class="pref-skeleton w-2/3"></div>
          <div class="pref-skeleton"></div>
          <div class="pref-skeleton"></div>
        </div>
      {:else}
        {#key nav}<div
            class="pref-pane"
            class:translation-pref-pane={nav === "translation"}
            class:workspace-pref-pane={nav === "services" ||
              nav === "vocabulary"}
            use:panelEntrance
          >
            {#if nav === "help"}<GettingStarted
                settings={configuration.settings}
                {locale}
                navigate={navigateRoute}
                onclose={() => {
                  void invoke("show_translation_window").catch(
                    (e) => (error = errorText(e, locale)),
                  );
                }}
              />
            {:else if nav === "translation"}
              <fieldset class="form-section guard-setting" disabled={busy}>
                <ReliabilitySettings bind:settings {locale} />
                <label class="extraction-setting"
                  ><input
                    type="checkbox"
                    bind:checked={settings.auto_vocabulary}
                  /><span
                    >{t(
                      "自动提炼生词",
                      "Automatically extract vocabulary",
                    )}<small class="subtle"
                      >{t(
                        "英语原文的生词候选会留在侧栏，主动收录后才进入生词本。",
                        "English vocabulary candidates stay in the sidebar and enter your vocabulary only when you save them.",
                      )}</small
                    ></span
                  ></label
                >
                <label class="extraction-setting"
                  ><input
                    type="checkbox"
                    checked={settings.auto_clean !== false}
                    onchange={(e) =>
                      (settings.auto_clean = e.currentTarget.checked)}
                  /><span
                    >{t(
                      "自动整理 PDF 断行",
                      "Automatically tidy PDF line breaks",
                    )}<small class="subtle"
                      >{t(
                        "捕获后合并明显的句中换行，保留段落、列表和代码；本机处理，可撤销。",
                        "After capture, join obvious breaks inside sentences while keeping paragraphs, lists and code. Local and reversible.",
                      )}</small
                    ></span
                  ></label
                >
                <label class="toggle"
                  ><input
                    type="checkbox"
                    checked={settings.encounter_highlight !== false}
                    onchange={(e) =>
                      (settings.encounter_highlight = e.currentTarget.checked)}
                  /><span
                    >{t(
                      "标记学习中的表达",
                      "Highlight learning vocabulary",
                    )}</span
                  ></label
                >
                <label class="toggle"
                  ><input
                    type="checkbox"
                    checked={settings.preserve_structure === true}
                    onchange={(e) =>
                      (settings.preserve_structure = e.currentTarget.checked)}
                  /><span
                    >{t(
                      "保留表格与公式结构",
                      "Preserve table and formula structure",
                    )}</span
                  ></label
                >
                <label class="toggle"
                  ><input
                    type="checkbox"
                    bind:checked={settings.sensitive_guard}
                  /><span
                    >{t(
                      "敏感信息本地拦截保护",
                      "Local sensitive-data protection",
                    )}</span
                  ></label
                >
              </fieldset>
              <div class="translation-settings">
                <div class="preference-tray">
                  <fieldset class="preference-core" disabled={busy}>
                    <legend
                      ><span class="preference-symbol"
                        ><Icon name="translate" size={18} /></span
                      >{t("默认语言", "Default languages")}</legend
                    >
                    <div class="default-languages">
                      <label
                        >{t("原文语言", "Source language")}<span
                          class="language-field"
                          ><select
                            aria-label={t(
                              "默认原文语言",
                              "Default source language",
                            )}
                            bind:value={settings.source_language}
                          >
                            <option value=""
                              >{t("自动识别", "Auto-detect")}</option
                            >
                            {#each languages as language}<option
                                value={language[0]}
                                >{language[locale === "zh" ? 1 : 2]}</option
                              >{/each}
                          </select></span
                        ></label
                      >
                      <span class="language-direction" aria-hidden="true"
                        ><Icon name="arrow" size={18} /></span
                      >
                      <label
                        >{t("翻译语言", "Translation language")}<span
                          class="language-field"
                          ><select
                            aria-label={t(
                              "默认翻译语言",
                              "Default translation language",
                            )}
                            bind:value={settings.target_language}
                          >
                            {#each languages as language}<option
                                value={language[0]}
                                >{language[locale === "zh" ? 1 : 2]}</option
                              >{/each}
                            {#if !languages.some((l) => l[0] === settings.target_language)}<option
                                value={settings.target_language}
                                >{settings.target_language}</option
                              >{/if}
                          </select></span
                        ></label
                      >
                    </div>
                  </fieldset>
                </div>
                <div class="preference-tray">
                  <fieldset class="preference-core" disabled={busy}>
                    <legend
                      ><span class="preference-symbol"
                        ><Icon name="keyboard" size={18} /></span
                      >{t("全局快捷键", "Global shortcuts")}</legend
                    >
                    <div class="shortcut-list">
                      <ShortcutRecorder
                        name="selection"
                        bind:value={settings.selection_shortcut}
                        {locale}
                        onrecordingchange={(active) => {
                          if (active) recordingField = "selection";
                          else if (recordingField === "selection")
                            recordingField = null;
                        }}
                      />
                      <ShortcutRecorder
                        name="screenshot"
                        bind:value={settings.screenshot_shortcut}
                        {locale}
                        onrecordingchange={(active) => {
                          if (active) recordingField = "screenshot";
                          else if (recordingField === "screenshot")
                            recordingField = null;
                        }}
                      />
                    </div>
                    <details class="optional-shortcuts">
                      <summary
                        ><span
                          >{t(
                            "可选追加快捷键",
                            "Optional append shortcuts",
                          )}</span
                        ><Icon name="chevron" size={15} /></summary
                      >
                      <div class="append-shortcut-list">
                        {#each ["append_selection_shortcut", "append_screenshot_shortcut"] as field}
                          {@const key = field as
                            | "append_selection_shortcut"
                            | "append_screenshot_shortcut"}
                          <ShortcutRecorder
                            name={key === "append_selection_shortcut"
                              ? "append_selection"
                              : "append_screenshot"}
                            bind:value={
                              () => settings[key] ?? "",
                              (value) => (settings[key] = value || null)
                            }
                            {locale}
                            onrecordingchange={(active) => {
                              if (active) recordingField = key;
                              else if (recordingField === key)
                                recordingField = null;
                            }}
                          />
                        {/each}
                      </div>
                    </details>
                    {#if recordingField}<span
                        class="recording-status"
                        role="status"
                        >{t("正在录入快捷键…", "Recording shortcut…")}</span
                      >{/if}
                    {#if shortcutError}<p class="error mt-4" role="alert">
                        {errorText(shortcutError, locale)}
                      </p>
                      <button
                        class="mt-3"
                        onclick={() =>
                          run(async () => {
                            shortcutError = (
                              await invoke<{ error: string | null }>(
                                "retry_shortcuts",
                              )
                            ).error;
                          })}
                        >{t(
                          "重新注册快捷键",
                          "Register shortcuts again",
                        )}</button
                      >{/if}
                  </fieldset>
                </div>
                <div class="preference-tray">
                  <fieldset
                    class="preference-core permission-core"
                    disabled={busy}
                  >
                    <legend
                      ><span class="preference-symbol"
                        ><Icon name="shield" size={18} /></span
                      >{t("系统权限", "System permissions")}</legend
                    >
                    <div class="permission-list">
                      <div class="permission-row">
                        <div class="permission-identity">
                          <strong>{t("辅助功能", "Accessibility")}</strong><span
                            class="permission-state"
                            class:allowed={permissions.accessibility}
                            >{permissions.accessibility
                              ? t("已授权", "Allowed")
                              : t("未授权", "Not allowed")}</span
                          >
                        </div>
                        <button
                          class="permission-action"
                          onclick={() => request("accessibility")}
                          >{t("打开系统设置", "Open system settings")}<span
                            class="button-island"
                            ><Icon name="arrow" size={14} /></span
                          ></button
                        >
                      </div>
                      <div class="permission-row">
                        <div class="permission-identity">
                          <strong>{t("屏幕录制", "Screen recording")}</strong
                          ><span
                            class="permission-state"
                            class:allowed={permissions.screen}
                            >{permissions.screen
                              ? t("已授权", "Allowed")
                              : t("未授权", "Not allowed")}</span
                          >
                        </div>
                        <button
                          class="permission-action"
                          onclick={() => request("screen")}
                          >{t("打开系统设置", "Open system settings")}<span
                            class="button-island"
                            ><Icon name="arrow" size={14} /></span
                          ></button
                        >
                      </div>
                    </div>
                    {#if !permissions.accessibility || !permissions.screen}
                      <details
                        class="permission-recovery"
                        bind:open={permissionRecoveryOpen}
                      >
                        <summary
                          >{t(
                            "更新后仍未授权？",
                            "Updated Yurelay but still not allowed?",
                          )}</summary
                        >
                        <p>
                          {t(
                            "如果系统列表里的 Yurelay 已开启，但这里仍显示未授权，仅点“＋”可能仍保留旧记录。请在对应权限列表中重新添加：",
                            "If Yurelay is switched on in System Settings but still shows Not allowed here, Add (+) may leave the old entry unchanged. Re-add it in the affected permission list:",
                          )}
                        </p>
                        <ol>
                          <li>
                            {t(
                              "选中旧的 Yurelay 条目，点击“−”移除。",
                              "Select the old Yurelay entry and click Remove (−).",
                            )}
                          </li>
                          <li>
                            {t(
                              "点击“＋”，重新添加当前正在使用的 Yurelay.app（通常在“应用程序”中）。",
                              "Click Add (＋) and select the current Yurelay.app, usually in Applications.",
                            )}
                          </li>
                          <li>
                            {t(
                              "开启授权；若系统提示，选择“退出并重新打开”。",
                              "Enable access and follow Quit & Reopen if macOS prompts you.",
                            )}
                          </li>
                          <li>
                            {t(
                              "回到 Yurelay，点击“刷新状态”确认结果。",
                              "Return to Yurelay and click Refresh status to confirm.",
                            )}
                          </li>
                        </ol>
                      </details>
                    {/if}
                    <button
                      class="permission-refresh"
                      onclick={() =>
                        run(async () => {
                          await refreshPermissions();
                          message = t(
                            "权限状态已刷新",
                            "Permissions refreshed",
                          );
                        })}>{t("刷新状态", "Refresh status")}</button
                    >
                  </fieldset>
                </div>
              </div>
            {:else if nav === "services"}
              {#if quickZhipu === selected}
                <section
                  class="getting-started"
                  aria-label={t("智谱快速配置", "Zhipu quick setup")}
                >
                  <h2>
                    {t("只需填入 API Key", "Only your API key is needed")}
                  </h2>
                  <p>
                    {t(
                      "已选择 GLM-4.7-Flash，智谱官方当前提供免费调用。使用额度与限流以平台为准。",
                      "GLM-4.7-Flash is selected and currently offered free by Zhipu. Usage limits follow the provider's policy.",
                    )}
                  </p>
                  <div class="guide-actions">
                    <button
                      disabled={busy}
                      onclick={() =>
                        run(async () => {
                          await invoke("open_translation_link", {
                            url: "https://open.bigmodel.cn/",
                          });
                        })}
                      >{t(
                        "1. 注册 / 登录智谱",
                        "1. Register / sign in",
                      )}</button
                    >
                    <button
                      disabled={busy}
                      onclick={() =>
                        run(async () => {
                          await invoke("open_translation_link", {
                            url: "https://bigmodel.cn/usercenter/proj-mgmt/apikeys",
                          });
                        })}>{t("2. 获取 API Key", "2. Get an API key")}</button
                    >
                  </div>
                  <p class="guide-detail">
                    {t(
                      "在智谱创建“模型标准 API”类型的 Key；Coding Plan 的 Key 不适用于这个入口。",
                      "Create a standard model API key. Coding Plan keys do not work with this endpoint.",
                    )}
                  </p>
                  <label
                    >API Key<input
                      type="password"
                      autocomplete="new-password"
                      spellcheck="false"
                      value={keys[selected] ?? ""}
                      placeholder={configuration.has_keys.includes(selected)
                        ? t("已保存；留空保留", "Saved; leave empty to keep")
                        : t("粘贴 API Key", "Paste API key")}
                      oninput={(e) => {
                        if (e.currentTarget.value)
                          keys[selected] = e.currentTarget.value;
                        else delete keys[selected];
                      }}
                    /></label
                  >
                  <p class="guide-detail">
                    {t(
                      "密钥仅在点击保存后写入系统钥匙串。若 macOS 要求授权，请在系统弹窗内输入密码。",
                      "The key is saved to Keychain only when you save. If macOS requests authorization, enter your password in its system dialog.",
                    )}
                  </p>
                  {@render serviceTestFeedback()}
                  <div class="guide-actions">
                    <button
                      disabled={busy || !configured(profile)}
                      onclick={() =>
                        run(async () => {
                          await testService(keyOverride());
                        })}
                      >{testFeedback?.state === "testing" &&
                      testFeedback.profileId === selected
                        ? t("测试中…", "Testing…")
                        : t("测试连接", "Test connection")}</button
                    >
                    <button
                      class="primary"
                      disabled={busy || !configured(profile)}
                      onclick={async () => {
                        if (await save())
                          message = t(
                            "智谱已保存并设为默认服务。可返回使用帮助继续。",
                            "Zhipu was saved as your default service. Return to Getting started to continue.",
                          );
                      }}>{t("保存并使用", "Save and use")}</button
                    >
                    <button disabled={busy} onclick={() => (quickZhipu = null)}
                      >{t(
                        "其他模型 / 高级设置",
                        "Other models / advanced settings",
                      )}</button
                    >
                    <button disabled={busy} onclick={() => (nav = "help")}
                      >{t("返回使用引导", "Back to getting started")}</button
                    >
                  </div>
                </section>
              {:else}
                <div class="service-workspace">
                  <aside class="service-list">
                    <div class="service-list-heading">
                      <h2>{t("我的服务", "My services")}</h2>
                    </div>
                    <label class="service-search"
                      ><Icon name="search" size={16} /><input
                        aria-label={t("搜索服务商", "Search providers")}
                        placeholder={t(
                          "搜索服务商 · 中文 / English / 拼音",
                          "Search providers · name or pinyin",
                        )}
                        bind:value={serviceQuery}
                      /></label
                    >
                    <div class="service-scroll">
                      {#each serviceEntries(settings.profiles, settings.active_profile, serviceQuery) as entry}
                        {#if entry.profile}
                          {@const item = entry.profile}
                          <button
                            class="service-item"
                            disabled={busy || closeSaving}
                            aria-pressed={selected === item.id}
                            onclick={() => {
                              selected = item.id;
                              if (
                                !item.model.trim() &&
                                presetFor(item.provider).model
                              )
                                item.model = presetFor(item.provider).model;
                              models = [];
                              error = "";
                              message = "";
                            }}
                          >
                            <span class="service-symbol"
                              ><ProviderLogo provider={item.provider} /></span
                            >
                            <span
                              ><strong>{serviceName(item, locale)}</strong
                              ><small
                                >{item.provider === "system_dictionary"
                                  ? t("本机可用", "On this Mac")
                                  : configured(item)
                                    ? t("已配置", "Configured")
                                    : t("待配置", "Not configured")}</small
                              ></span
                            >
                            {#if item.id === settings.active_profile}<span
                                class="service-default-tag"
                                >{t("默认", "Default")}</span
                              >{/if}
                          </button>
                        {:else}
                          {@const preset = entry.preset}
                          <button
                            class="service-item available-service"
                            disabled={busy ||
                              closeSaving ||
                              settings.profiles.length >= 20}
                            onclick={() => add(preset.provider)}
                          >
                            <span class="service-symbol"
                              ><ProviderLogo provider={preset.provider} /></span
                            ><span
                              ><strong
                                >{locale === "zh"
                                  ? preset.zh
                                  : preset.name}</strong
                              ><small
                                >{preset.provider === "system_dictionary"
                                  ? t("本机可用", "On this Mac")
                                  : t("待配置", "Not configured")}</small
                              ></span
                            >
                          </button>
                        {/if}
                      {/each}
                      {#if !servicePresets.some( (p) => matchesService(p.provider, p.name, serviceQuery) ) && !settings.profiles.some( (p) => matchesService(p.provider, p.name, serviceQuery) )}<p
                          class="subtle"
                        >
                          {t("没有匹配的服务商", "No matching providers")}
                        </p>{/if}
                    </div>
                  </aside>
                  <div class="service-editor-tray">
                    <fieldset
                      class="form-section service-editor"
                      disabled={busy || closeSaving}
                    >
                      <legend
                        ><ProviderLogo
                          provider={profile.provider}
                          size={28}
                        />{serviceName(profile, locale)}</legend
                      >
                      <div class="service-editor-state">
                        <span
                          class="service-state"
                          class:configured={configured(profile)}
                          >{profile.provider === "system_dictionary"
                            ? t("本机可用", "On this Mac")
                            : configured(profile)
                              ? t("已配置", "Configured")
                              : t("待配置", "Not configured")}</span
                        ><button
                          disabled={profile.id === settings.active_profile ||
                            !configured(profile)}
                          onclick={() => (settings.active_profile = profile.id)}
                          >{t("设为默认", "Use by default")}</button
                        >
                      </div>
                      <ServiceVerificationBadge
                        {profile}
                        savedProfile={configuration.settings.profiles.find(
                          (p) => p.id === profile.id,
                        )}
                        keyChanged={profile.id in keys}
                        revision={verificationVersion}
                        {locale}
                      />
                      {#if profile.provider === "system_dictionary"}
                        <div class="dictionary-note">
                          <span class="preference-symbol"
                            ><Icon name="translate" size={22} /></span
                          >
                          <h2>
                            {t(
                              "查询本机已启用的词典",
                              "Look up words in your Mac dictionaries",
                            )}
                          </h2>
                          <p>
                            {t(
                              "直接展示单词和短语的释义，无需 API Key。",
                              "Definitions for words and phrases. No API key required.",
                            )}
                          </p>
                        </div>
                        {@render serviceTestFeedback()}
                        <div class="service-actions">
                          <button
                            onclick={() =>
                              run(async () => {
                                await testService(null);
                              })}>{t("测试词典", "Test dictionary")}</button
                          ><button
                            onclick={() =>
                              run(async () => {
                                await invoke("open_dictionary", {
                                  kind: "system",
                                  text: "",
                                });
                              })}
                            >{t(
                              "打开系统词典",
                              "Open system dictionary",
                            )}</button
                          >
                        </div>
                      {:else}
                        <div class="service-fields">
                          {#if profile.provider === "custom"}
                            <label
                              >{t("服务名称", "Service name")}<input
                                value={serviceName(profile, locale)}
                                oninput={(event) =>
                                  (profile.name = event.currentTarget.value)}
                                maxlength="80"
                              /></label
                            >
                            <label
                              >{t("服务地址", "Endpoint")}<input
                                bind:value={profile.endpoint}
                                placeholder="https://api.example.com/v1"
                                spellcheck="false"
                                autocomplete="off"
                              /></label
                            >
                            <label
                              >{t("地址类型", "Endpoint type")}<select
                                bind:value={profile.endpoint_mode}
                                ><option value="base"
                                  >{t(
                                    "基础地址（自动补全请求路径）",
                                    "Base URL (append request path)",
                                  )}</option
                                ><option value="exact"
                                  >{t(
                                    "完整请求地址",
                                    "Full request URL",
                                  )}</option
                                ></select
                              ></label
                            >
                          {:else if profile.provider.startsWith("minimax")}
                            <label
                              >{t("账户区域", "Account region")}<select
                                value={profile.provider}
                                onchange={(e) => {
                                  profile.provider = e.currentTarget
                                    .value as Profile["provider"];
                                  profile.endpoint =
                                    presets[profile.provider].endpoint;
                                }}
                                ><option value="minimax_china"
                                  >{t("中国区", "China")}</option
                                ><option value="minimax"
                                  >{t("国际区", "International")}</option
                                ></select
                              ></label
                            >
                          {/if}
                          {#if profile.provider === "youdao"}<label
                              >{t("应用 ID", "Application ID")}<input
                                bind:value={profile.app_id}
                                placeholder={t(
                                  "有道智云应用 ID",
                                  "Youdao application ID",
                                )}
                                maxlength="256"
                                autocomplete="off"
                              /></label
                            >{/if}
                          <label
                            >{profile.provider === "youdao"
                              ? t("应用密钥", "Application secret")
                              : "API Key"}<input
                              type="password"
                              value={keys[selected] ?? ""}
                              oninput={(e) => {
                                if (e.currentTarget.value)
                                  keys[selected] = e.currentTarget.value;
                                else delete keys[selected];
                              }}
                              autocomplete="new-password"
                              spellcheck="false"
                              placeholder={configuration.has_keys.includes(
                                selected,
                              )
                                ? t(
                                    "已保存；留空保留原密钥",
                                    "Saved · leave empty to keep",
                                  )
                                : profile.provider === "youdao"
                                  ? t(
                                      "粘贴应用密钥",
                                      "Paste the application secret",
                                    )
                                  : t("粘贴你的 API Key", "Paste your API key")}
                            /></label
                          >
                          {#if keys[selected] === null}<p class="subtle">
                              {t(
                                "保存后将删除已存密钥。",
                                "The saved key will be removed when you save.",
                              )}
                            </p>{/if}
                          {#if configuration.credential_errors[selected]}<p
                              class="error"
                            >
                              {errorText(
                                configuration.credential_errors[selected],
                                locale,
                              )}
                            </p>{/if}
                          <p class="subtle">
                            {t(
                              "API Key 已加密保存在 macOS 系统钥匙串。保存或授权读取已有密钥时，系统可能询问钥匙串密码，通常是 Mac 登录密码。",
                              "API keys are encrypted in macOS Keychain. Saving or authorizing an existing key may require your keychain password, usually your Mac login password.",
                            )}
                          </p>
                          {#if configuration.has_keys.includes(selected)}
                            <button
                              class="subtle-button"
                              disabled={busy}
                              onclick={authorizeSavedKey}
                            >
                              {t("授权已保存密钥", "Authorize saved key")}
                            </button>
                          {/if}
                          {#if usesModel(profile.provider)}<label
                              >{t("模型", "Model")}<input
                                bind:value={profile.model}
                                list="provider-models"
                                placeholder={t(
                                  "获取模型列表或手动填写模型名称",
                                  "Fetch models or enter a model name",
                                )}
                                maxlength="256"
                              /></label
                            >{/if}
                        </div>
                        {@render serviceTestFeedback()}
                        <div class="service-actions">
                          <button
                            onclick={() =>
                              run(async () => {
                                await testService(keyOverride());
                              })}
                            >{testFeedback?.state === "testing" &&
                            testFeedback.profileId === selected
                              ? t("测试中…", "Testing…")
                              : t("测试连接", "Test connection")}</button
                          >
                          {#if usesModel(profile.provider) && !profile.provider.startsWith("minimax")}<button
                              onclick={() =>
                                run(async () => {
                                  models =
                                    (await invoke<RemoteModel[]>(
                                      "fetch_models",
                                      {
                                        profile: $state.snapshot(profile),
                                        keyOverride: keyOverride(),
                                      },
                                    )) ?? [];
                                  message = t(
                                    `已获取 ${models.length} 个模型，可在模型字段选择。`,
                                    `${models.length} models loaded. Choose one in the model field.`,
                                  );
                                })}>{t("获取模型列表", "Fetch models")}</button
                            >{/if}
                          {#if profile.provider === "youdao"}<button
                              onclick={() =>
                                run(async () => {
                                  await invoke("open_dictionary", {
                                    kind: "youdao",
                                    text: "",
                                  });
                                })}
                              >{t(
                                "打开有道词典",
                                "Open Youdao dictionary",
                              )}</button
                            >{/if}
                        </div>
                        <p class="subtle mt-3">
                          {profile.provider === "youdao"
                            ? t(
                                "翻译使用有道智云官方 API；网页词典可单独打开。",
                                "Translation uses the official Youdao API. The web dictionary opens separately.",
                              )
                            : t(
                                "测试仅发送一段固定示例文字，可能产生少量 API 费用。",
                                "The test sends a fixed sample text and may incur a small API charge.",
                              )}
                        </p>
                        <datalist id="provider-models"
                          >{#each visibleModels as choice}<option
                              value={choice.id}
                              label={modelPriceLabel(choice, locale) ||
                                choice.id}
                            ></option>{/each}</datalist
                        >
                        {#if usesModel(profile.provider)}<section
                            class="model-catalog"
                            aria-label={t("模型候选", "Model candidates")}
                          >
                            <div class="model-catalog-heading">
                              {#if modelPriceLabel(currentModelChoice, locale)}<span
                                  class="model-price-tag"
                                  >{modelPriceLabel(
                                    currentModelChoice,
                                    locale,
                                  )}</span
                                >{/if}
                              {#if profile.provider === "openrouter"}<label
                                  class="model-free-filter"
                                  ><input
                                    type="checkbox"
                                    bind:checked={onlyFreeModels}
                                  />{t(
                                    "仅显示免费模型",
                                    "Show free models only",
                                  )}</label
                                >{/if}
                            </div>
                            {#if visibleModels.length}<div
                                class="model-suggestions"
                              >
                                {#each visibleModels as choice}<button
                                    type="button"
                                    class:selected-model={profile.model ===
                                      choice.id}
                                    onclick={() => (profile.model = choice.id)}
                                    ><span>{choice.id}</span
                                    >{#if modelPriceLabel(choice, locale)}<span
                                        class="model-price-tag"
                                        >{modelPriceLabel(choice, locale)}</span
                                      >{/if}</button
                                  >{/each}
                              </div>{/if}
                            {#if profile.provider === "gemini"}<p
                                class="subtle"
                              >
                                {t(
                                  "免费层受账户、地区及配额限制；付费项目按官方价格计费。",
                                  "Free-tier eligibility and quotas depend on the account and region. Paid projects are billed at published rates.",
                                )}
                              </p>
                            {:else if profile.provider === "openrouter"}<p
                                class="subtle"
                              >
                                {models.length && !visibleModels.length
                                  ? t(
                                      "本次列表中没有已确认的免费模型，可关闭筛选查看全部。",
                                      "No confirmed free models in this list. Turn off the filter to view all models.",
                                    )
                                  : t(
                                      "获取实时模型列表后筛选免费项；免费型号仍有调用配额。",
                                      "Fetch the current catalog to filter free models; usage limits still apply.",
                                    )}
                              </p>{/if}
                          </section>{/if}
                        {#if usesModel(profile.provider)}<details class="mt-6">
                            <summary
                              >{t("高级选项", "Advanced options")}</summary
                            >
                            <div class="service-fields mt-4">
                              <label class="flex-row items-center gap-3"
                                ><input
                                  type="checkbox"
                                  bind:checked={profile.streaming}
                                />{t("流式输出", "Streaming response")}</label
                              >{#if !["openai", "anthropic"].includes(profile.provider)}<label
                                  >{t("温度", "Temperature")}<input
                                    type="number"
                                    min="0"
                                    max={profile.provider === "zhipu" ? 1 : 2}
                                    step={profile.provider === "zhipu"
                                      ? 0.01
                                      : 0.1}
                                    bind:value={profile.temperature}
                                  /></label
                                >{/if}
                            </div>
                          </details>{/if}
                      {/if}
                      {#if usesModel(profile.provider)}<ServicePromptReferences
                          {profile}
                          {settings}
                          {locale}
                          defaultTranslation={defaultPrompt}
                          defaultReply={replyPromptTemplate}
                          onOpenPrompts={() => (nav = "prompts")}
                          onChange={(field, value) => (profile[field] = value)}
                        />{/if}
                      <div class="service-secondary-actions">
                        {#if profile.provider !== "system_dictionary"}<button
                            onclick={() => (keys[selected] = null)}
                            >{t("删除已保存密钥", "Remove saved key")}</button
                          >{/if}<button
                          disabled={settings.profiles.length <= 1}
                          onclick={remove}
                          >{t("移除服务", "Remove service")}</button
                        >
                      </div>
                    </fieldset>
                  </div>
                </div>
              {/if}
            {:else if nav === "prompts"}
              <PromptSettings
                bind:settings
                {locale}
                defaultTranslation={defaultPrompt}
                defaultReply={replyPromptTemplate}
                replyLoading={replyPromptLoading}
                replyError={replyPromptError}
              />
            {:else if nav === "vocabulary"}
              <VocabularyPanel {locale} />
            {:else if nav === "history"}
              {#key historyVersion}<HistoryPanel {locale} />{/key}
              <fieldset class="form-section retention-section" disabled={busy}>
                <legend>{t("保存与清理", "Storage and retention")}</legend>
                <div class="grid grid-cols-1 gap-5 lg:grid-cols-2">
                  <label
                    >{t("保存期限", "Keep records")}<select
                      value={settings.retention.mode}
                      onchange={(e) => {
                        const mode = e.currentTarget.value;
                        settings.retention =
                          mode === "days"
                            ? { mode: "days", days: 7 }
                            : { mode: mode as "disabled" | "forever" };
                      }}
                      ><option value="days"
                        >{t("指定天数", "For a number of days")}</option
                      ><option value="forever"
                        >{t("永久保存", "Forever")}</option
                      ><option value="disabled"
                        >{t("不保存到磁盘", "Do not save to disk")}</option
                      ></select
                    ></label
                  >{#if settings.retention.mode === "days"}<label
                      >{t("天数", "Days")}<input
                        type="number"
                        min="1"
                        max="36500"
                        bind:value={settings.retention.days}
                      /></label
                    >{/if}
                </div>
                <p class="subtle mt-4">
                  {t(
                    "按创建时间清理。缩短期限或关闭保存会删除已有记录。",
                    "Shortening retention or turning saving off removes older records.",
                  )}
                </p>
              </fieldset>
            {:else if nav === "appearance"}
              <fieldset class="form-section" disabled={busy}>
                <legend>{t("界面语言", "Interface language")}</legend><label
                  >{t("显示语言", "Display language")}<select
                    bind:value={settings.ui_language}
                    ><option value="follow_primary"
                      >{t("跟随主语言", "Follow primary language")}</option
                    ><option value="zh">中文</option><option value="en"
                      >English</option
                    ></select
                  >
                </label>
              </fieldset>
              <fieldset class="form-section" disabled={busy}>
                <legend>{t("外观与文字", "Appearance and type")}</legend>
                <div class="grid grid-cols-1 gap-5 lg:grid-cols-2">
                  <label
                    >{t("外观", "Theme")}<select bind:value={settings.theme}
                      ><option value="system">{t("跟随系统", "System")}</option
                      ><option value="light">{t("浅色", "Light")}</option
                      ><option value="dark">{t("深色", "Dark")}</option></select
                    ></label
                  ><label
                    >{t("界面字号", "Interface font size")}<select
                      bind:value={settings.ui_font_size}
                      >{#each [12, 13, 14, 15, 16, 17, 18] as size}<option
                          value={size}>{size} px</option
                        >{/each}</select
                    ></label
                  >
                </div>
                <label class="mt-5"
                  >{t("原文与译文字号", "Reading font size")}
                  <div class="flex items-center gap-4">
                    <input
                      type="range"
                      min="12"
                      max="28"
                      step="1"
                      bind:value={settings.font_size}
                    /><output class="shrink-0 text-sm tabular-nums"
                      >{settings.font_size} px</output
                    >
                  </div></label
                >
                <div class="mt-5 rounded-lg bg-[var(--hover)] p-5">
                  <p class="mb-2 text-xs text-[var(--muted)]">
                    {t("阅读预览", "READING PREVIEW")}
                  </p>
                  <p
                    style={`font-size:${settings.font_size}px`}
                    class="leading-relaxed"
                  >
                    {t(
                      "在文字之间，自由切换语言。",
                      "Move freely between words and languages.",
                    )}
                  </p>
                </div>
              </fieldset>
              <fieldset class="form-section" disabled={busy}>
                <legend>{t("窗口与任务", "Windows and tasks")}</legend><label
                  class="flex-row items-start gap-3"
                  ><input
                    type="checkbox"
                    bind:checked={settings.auto_hide}
                  /><span
                    >{t(
                      "切换到其他应用时收起翻译窗口",
                      "Hide the translation window when another app gains focus",
                    )}
                  </span></label
                ><label class="mt-5"
                  >{t("同时翻译数量", "Concurrent translations")}<select
                    bind:value={settings.max_concurrency}
                    ><option value={1}>1</option><option value={2}>2</option
                    ></select
                  ></label
                >
              </fieldset>
            {/if}
          </div>{/key}
      {/if}
    </main>
    <footer class="settings-actions">
      <span class="subtle" role="status"
        >{busy
          ? t("正在处理…", "Working…")
          : dirty
            ? t("有未保存的修改", "Unsaved changes")
            : t("所有修改已保存", "All changes saved")}</span
      >
      <div class="settings-action-buttons">
        <button disabled={busy || !dirty || !ready} onclick={revert}
          >{t("还原", "Revert")}</button
        ><button
          class="primary"
          disabled={busy || !ready || !dirty || recordingField !== null}
          onclick={save}>{t("保存设置", "Save changes")}</button
        >
      </div>
    </footer>
  </div>
</div>

<dialog
  bind:this={closeDialog!}
  class="settings-close-dialog"
  aria-labelledby="close-settings-title"
  oncancel={(e) => {
    e.preventDefault();
    if (!closeSaving) cancelClose();
  }}
>
  <div class="close-dialog-core">
    <span class="close-dialog-symbol"><Icon name="display" size={24} /></span>
    <h2 id="close-settings-title">
      {t("保存设置修改？", "Save your changes?")}
    </h2>
    <p>
      {t(
        "有未保存的修改，请保存后再关闭。",
        "You have unsaved changes. Save them before closing.",
      )}
    </p>
    {#if error}<p class="dialog-error" role="alert">{error}</p>{/if}
    <div class="close-dialog-actions">
      <button disabled={closeSaving || busy} onclick={cancelClose}
        >{t("继续编辑", "Keep editing")}</button
      >
      <button disabled={closeSaving || busy} onclick={discardAndClose}
        >{t("不保存", "Don’t save")}</button
      >
      <button
        class="primary"
        disabled={closeSaving || busy || recordingField !== null}
        onclick={saveAndClose}
        >{closeRequest?.intent === "quit"
          ? t("保存并退出", "Save and quit")
          : t("保存并关闭", "Save and close")}</button
      >
    </div>
  </div>
</dialog>
