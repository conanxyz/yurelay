<script lang="ts">
  import { onMount, tick, untrack } from "svelte";
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { errorText, type Locale } from "./locale";
  import type {
    Settings,
    ExperienceStatus,
    Experience,
    SettingsRoute,
  } from "./types";
  import Icon from "./Icons.svelte";
  import ServiceVerificationBadge from "./ServiceVerificationBadge.svelte";
  let {
    settings,
    locale,
    initialStep,
    busy = $bindable(false),
    onclose,
    navigate,
  }: {
    settings: Settings;
    locale: Locale;
    initialStep?: number;
    busy?: boolean;
    onclose?: () => void;
    navigate: (route: SettingsRoute) => Promise<void>;
  } = $props();
  const t = (zh: string, en: string) => (locale === "zh" ? zh : en);
  let step = $state(untrack(() => initialStep ?? 0)),
    error = $state("");
  let permissions = $state({ accessibility: false, screen: false });
  let status = $state<ExperienceStatus | null>(null);
  let heading = $state<HTMLHeadingElement>();
  let demoResult = $state(false);
  function demoKeydown(event: KeyboardEvent) {
    if (
      step !== 2 ||
      busy ||
      event.defaultPrevented ||
      event.repeat ||
      event.isComposing ||
      event.ctrlKey ||
      event.altKey ||
      event.metaKey ||
      event.shiftKey ||
      document.querySelector("dialog[open]")
    )
      return;
    const editing =
      'input,textarea,select,[contenteditable]:not([contenteditable="false"]),[role="textbox"],[role="combobox"]';
    if (
      (event.target as HTMLElement)?.closest(editing) ||
      document.activeElement?.closest(editing)
    )
      return;
    if (["d", "s"].includes(event.key.toLowerCase())) {
      event.preventDefault();
      demoResult = true;
    }
  }
  let disposed = false,
    sequence = 0;
  const active = $derived(
    status?.services.find((s) => s.profile_id === settings.active_profile),
  );
  const profile = $derived(
    settings.profiles.find((p) => p.id === settings.active_profile),
  );
  const shortcut = (value: string) =>
    (value || "")
      .replace(/Alt/g, "⌥")
      .replace(/CommandOrControl|Command|Meta/g, "⌘")
      .replace(/Control/g, "⌃")
      .replace(/Shift/g, "⇧")
      .replace(/\+/g, "");
  async function run(work: () => Promise<void>) {
    if (busy) return;
    busy = true;
    error = "";
    try {
      await work();
    } catch (e) {
      error = errorText(e, locale);
    } finally {
      busy = false;
    }
  }
  async function refresh() {
    if (!isTauri()) return;
    const version = ++sequence;
    const [p, s] = await Promise.all([
      invoke<typeof permissions>("permissions"),
      invoke<ExperienceStatus>("experience_status"),
    ]);
    if (!disposed && version === sequence) {
      permissions = p;
      status = s;
    }
  }
  async function progress(action: object) {
    if (isTauri()) {
      const result = await invoke<Experience>("update_experience", { action });
      if (status && result) status.progress = result;
    }
  }
  async function go(next: number) {
    await run(async () => {
      await progress({ type: "step", step: next });
      step = next;
      await tick();
      heading?.focus();
    });
  }
  async function close(finish: boolean) {
    await run(async () => {
      await progress({ type: finish ? "finish" : "skip" });
      onclose?.();
    });
  }
  export async function skip() {
    await close(false);
  }
  onMount(() => {
    let loaded = false;
    const update = () =>
      refresh()
        .then(() => {
          if (!loaded && status) {
            if (initialStep === undefined) step = status.progress.step;
            loaded = true;
          }
        })
        .catch((e) => {
          if (!disposed) error = errorText(e, locale);
        });
    void update();
    window.addEventListener("focus", update);
    const timer = setInterval(() => {
      if (
        !document.hidden &&
        step === 0 &&
        (!permissions.accessibility || !permissions.screen)
      )
        void update();
    }, 2000);
    const off = isTauri()
      ? listen("configuration-changed", update)
      : Promise.resolve(() => {});
    return () => {
      disposed = true;
      sequence++;
      clearInterval(timer);
      window.removeEventListener("focus", update);
      off.then((f) => f());
    };
  });
</script>

<svelte:window onkeydown={demoKeydown} />

<section
  class="getting-started"
  aria-label={t("使用引导", "Getting started")}
  aria-busy={busy}
>
  <div class="guide-heading">
    <span class="guide-eyebrow"
      >YURELAY · {t("开始使用", "GETTING STARTED")}</span
    >
    {#if onclose}<button class="guide-skip" disabled={busy} onclick={skip}
        ><span>{t("稍后设置", "Set up later")}</span><Icon
          name="close"
          size={14}
        /></button
      >{/if}
  </div>
  <nav class="guide-steps" aria-label={t("引导步骤", "Setup steps")}>
    {#each [t("权限准备", "Permissions"), t("选择服务", "Service"), t("基础操作", "Practice")] as label, i}<button
        disabled={busy}
        aria-current={step === i ? "step" : undefined}
        onclick={() => go(i)}><span>{i + 1}</span>{label}</button
      >{/each}
  </nav>
  {#if error}<p class="inline-feedback" role="alert">{error}</p>{/if}
  {#if step === 0}
    <h2 bind:this={heading!} tabindex="-1">
      {t("选择你需要的取词方式", "Choose how to capture text")}
    </h2>
    <p>
      {t(
        "权限只用于对应功能。暂不授权，也可以手动输入或粘贴文字。",
        "Each permission enables one capture method. You can still type or paste without granting either permission.",
      )}
    </p>
    {#each [{ kind: "accessibility" as const, title: t("辅助功能", "Accessibility"), purpose: t("读取其他应用中选中的文字。", "Read selected text from other apps.") }, { kind: "screen" as const, title: t("屏幕录制", "Screen recording"), purpose: t("截取你选择的区域，在本机识别文字。", "Capture your chosen region and recognize its text on this Mac.") }] as item}
      <div class="guide-card">
        <div class="guide-card-heading">
          <h3>{item.title}</h3>
          <span role="status" class:allowed={permissions[item.kind]}
            >{permissions[item.kind]
              ? t("已授权", "Allowed")
              : t("未授权", "Not allowed")}</span
          >
        </div>
        <p>{item.purpose}</p>
        <button
          disabled={busy}
          onclick={() =>
            run(async () => {
              await invoke("request_permission", { kind: item.kind });
              await refresh();
            })}
          >{t("打开系统设置", "Open system settings")} · {item.title}</button
        >
      </div>
    {/each}
    <p class="guide-detail">
      {t(
        "在“隐私与安全性”对应列表中开启 Yurelay，再返回这里。若系统提示“退出并重新打开”，按提示操作，引导进度会保留。",
        "Enable Yurelay in the matching Privacy & Security list, then return here. Follow Quit & Reopen if macOS requests it; your progress is kept.",
      )}
    </p>
    <button disabled={busy} onclick={() => run(refresh)}
      >{t("刷新权限状态", "Refresh permissions")}</button
    >
    <details>
      <summary
        >{t(
          "已开启但仍显示未授权？",
          "Enabled but still not allowed?",
        )}</summary
      >
      <p>
        {t(
          "请确认授权的是当前安装的 Yurelay.app。更新后旧条目可能失效：在对应列表移除旧条目，重新添加当前应用并开启授权，再按系统提示重新打开。",
          "Check that access belongs to the installed Yurelay.app. After an update, remove a stale entry, add the current app, enable it and reopen if macOS requests it.",
        )}
      </p>
    </details>
  {:else if step === 1}
    <h2 bind:this={heading!} tabindex="-1">
      {t(
        "先选一个服务，再开始查询",
        "Choose a service before your first query",
      )}
    </h2>
    <div class="guide-card">
      <h3>{t("系统词典 · 无需 API", "System dictionary · No API key")}</h3>
      <p>
        {t(
          "查询单词和短语；不提供整句翻译、语境解析或构思回复。",
          "Definitions for words and phrases. It does not translate passages, explain context or compose replies.",
        )}
      </p>
      <button
        disabled={busy}
        onclick={() =>
          run(async () => {
            await progress({ type: "step", step: 1 });
            await navigate({
              section: "services",
              profile_id: null,
              dictionary: true,
            });
          })}>{t("选择系统词典", "Choose system dictionary")}</button
      >
    </div>
    <div class="guide-card">
      <h3>{t("智谱 · 免费模型入门", "Zhipu · Start with a free model")}</h3>
      <p>
        {t(
          "注册智谱开放平台账号，创建模型标准 API 的 Key。已预设 GLM-4.7-Flash，只需填 Key 即可配置翻译、语境解析与回复。",
          "Register with BigModel and create a standard model API key. GLM-4.7-Flash is preset; only your key is needed for translation, explanations and replies.",
        )}
      </p>
      <button
        disabled={busy}
        onclick={() =>
          run(async () => {
            await progress({ type: "step", step: 1 });
            await navigate({ section: "services", zhipu: true });
          })}>{t("使用智谱免费模型", "Set up free Zhipu model")}</button
      >
    </div>
    <div class="guide-card">
      <h3>
        {t(
          "翻译服务 · 句子和段落",
          "Translation service · Sentences and passages",
        )}
      </h3>
      <p>
        {t(
          "选择服务商，填写 API 凭据和模型，测试后保存。支持生成的模型服务还可解析语境、构思回复；普通翻译 API 按实际能力使用。",
          "Choose a provider, enter credentials and a model, then test and save. Generative model services also support context explanations and replies; other translation APIs have fewer capabilities.",
        )}
      </p>
      <button
        disabled={busy}
        onclick={() =>
          run(async () => {
            await progress({ type: "step", step: 1 });
            await navigate({
              section: "services",
              profile_id: null,
              translation: true,
            });
          })}>{t("配置翻译服务", "Configure translation service")}</button
      >
    </div>
    <p class="guide-detail">
      {t(
        "在服务页选择“设为默认”，再点击“保存设置”。密钥只存入系统钥匙串。连接测试仅在你点击时发送固定示例，不发送当前原文。",
        "Choose Use by default, then Save changes on the service page. Keys are stored only in Keychain. A connection test sends a fixed sample only when you click it, never your current source.",
      )}
    </p>
    {#if active}<p role="status" class="guide-readiness">
        {t("已保存的默认服务：", "Saved default service: ")}{profile?.name} · {active.state ===
        "configured"
          ? t("已配置", "Configured")
          : active.state === "needs_key"
            ? t("需要 API 凭据", "API credentials needed")
            : active.state === "needs_authorization"
              ? t("需要钥匙串授权", "Keychain authorization needed")
              : t("配置未完成", "Setup incomplete")}
      </p>
      {#if active.state === "configured" && profile}<ServiceVerificationBadge
          {profile}
          savedProfile={profile}
          {locale}
        />{/if}
    {/if}
    <p>
      {t(
        "暂时没有 API？可以先用系统词典，或继续练习基础操作。",
        "No API yet? Start with the system dictionary, or continue to the practice below.",
      )}
    </p>
  {:else}
    <h2 bind:this={heading!} tabindex="-1">
      {t("三步就能开始", "Three steps to get started")}
    </h2>
    <ol class="guide-basics">
      <li>
        <strong>{t("取词", "Get text")}</strong>
        <p>
          {t("选中文字后按", "Select text and press ")}
          <kbd>{shortcut(settings.selection_shortcut)}</kbd>；{t(
            "截图按",
            "capture with ",
          )} <kbd>{shortcut(settings.screenshot_shortcut)}</kbd>。
        </p>
      </li>
      <li>
        <strong>{t("翻译", "Translate")}</strong>
        <p>
          {t(
            "核对原文，点击“翻译”或按 D / S。编辑文字时不触发。",
            "Review the text, then click Translate or press D / S. Shortcuts are inactive while editing.",
          )}
        </p>
      </li>
      <li>
        <strong>{t("复制", "Copy")}</strong>
        <p>
          {t(
            "点击“复制译文”，粘贴到需要的位置。",
            "Click Copy translation, then paste it where needed.",
          )}
        </p>
      </li>
    </ol>
    <div class="guide-demo">
      <strong>{t("试按 D 或 S", "Try pressing D or S")}</strong>
      <p>Hello, how are you?</p>
      <button
        disabled={busy}
        aria-keyshortcuts="D S"
        onclick={() => (demoResult = true)}
        >{t("翻译示例（D / S）", "Translate sample (D / S)")}</button
      >
      {#if demoResult}<p class="demo-result" role="status">
          {t("示例译文：你好，你好吗？", "Sample translation: 你好，你好吗？")}
        </p>{/if}
      <p class="guide-detail">
        {t(
          "这里只练习操作，不调用翻译服务。",
          "This practice does not call a translation service.",
        )}
      </p>
    </div>
    {#if status?.shortcuts && !status.shortcuts.registered}<p role="alert">
        {t(
          "全局快捷键未能注册，请在快捷键设置中检查冲突。手动输入仍可使用。",
          "Global shortcuts could not be registered. Check for conflicts in shortcut settings; manual input still works.",
        )}
      </p>{/if}
    <button
      disabled={busy}
      onclick={() =>
        run(() => navigate({ section: "translation", profile_id: null }))}
      >{t("修改快捷键", "Change shortcuts")}</button
    >
  {/if}
  <footer class="guide-actions">
    {#if step > 0}<button disabled={busy} onclick={() => go(step - 1)}
        >{t("上一步", "Back")}</button
      >{/if}
    {#if step < 2}<button
        class="primary"
        disabled={busy}
        onclick={() => go(step + 1)}
        >{step === 0
          ? t("继续选择服务", "Continue to services")
          : t("练习基础操作", "Practice the basics")}</button
      >
    {:else}<button
        class="primary"
        disabled={busy}
        onclick={() =>
          run(async () => {
            await progress({
              type: demoResult ? "finish_tutorial" : "skip_tutorial",
            });
            await progress({ type: "finish" });
            onclose?.();
          })}>{t("开始使用", "Start using Yurelay")}</button
      >{/if}
  </footer>
</section>
