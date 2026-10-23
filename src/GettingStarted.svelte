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
  import ServiceVerificationBadge from "./ServiceVerificationBadge.svelte";
  let {
    settings,
    locale,
    initialStep,
    onclose,
    navigate,
  }: {
    settings: Settings;
    locale: Locale;
    initialStep?: number;
    onclose?: () => void;
    navigate: (route: SettingsRoute) => Promise<void>;
  } = $props();
  const t = (zh: string, en: string) => (locale === "zh" ? zh : en);
  let step = $state(untrack(() => initialStep ?? 0)),
    lesson = $state(0),
    busy = $state(false),
    error = $state("");
  let permissions = $state({ accessibility: false, screen: false });
  let status = $state<ExperienceStatus | null>(null);
  let heading = $state<HTMLHeadingElement>();
  let demo = $state(""),
    demoResult = $state(false),
    copied = $state(false),
    demoHidden = $state(false),
    demoPinned = $state(false);
  let advanced = $state(false);
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
  async function nextLesson() {
    await run(async () => {
      const next = Math.min(3, lesson + 1);
      await progress({ type: "lesson", lesson: next });
      lesson = next;
      await tick();
      heading?.focus();
    });
  }
  onMount(() => {
    let loaded = false;
    const update = () =>
      refresh()
        .then(() => {
          if (!loaded && status) {
            lesson =
              status.progress.tutorial === "completed"
                ? 0
                : status.progress.lesson;
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

<section
  class="getting-started"
  aria-label={t("使用引导", "Getting started")}
  aria-busy={busy}
>
  <div class="guide-heading">
    <span class="guide-eyebrow"
      >YURELAY · {t("开始使用", "GETTING STARTED")}</span
    >
    {#if onclose}<button
        class="guide-skip"
        disabled={busy}
        onclick={() => close(false)}>{t("稍后设置", "Set up later")}</button
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
      {t("练习一次，掌握基础操作", "Practice the basic workflow")}
    </h2>
    <p class="guide-detail">
      {t(
        "以下是独立演示，不请求网络、不修改你的原文，也不写入历史记录。",
        "This is a separate demo: no network requests, no changes to your source and no history entries.",
      )}
    </p>
    <div class="guide-demo">
      <strong
        >{lesson + 1} / 4 · {[
          t("取得原文", "Get text"),
          t("核对文字和语言", "Review text and languages"),
          t("主动开始翻译", "Start translation"),
          t("复制与窗口操作", "Copy and window controls"),
        ][lesson]}</strong
      >
      {#if lesson === 0}<p>
          {t(
            "在其他应用选中文字后按",
            "Select text in another app, then press ",
          )} <kbd>{shortcut(settings.selection_shortcut)}</kbd>；{t(
            "截图按",
            "capture a region with ",
          )} <kbd>{shortcut(settings.screenshot_shortcut)}</kbd>。
        </p>
        <button
          disabled={busy}
          onclick={() => {
            demo = "Hello, how are you?";
            nextLesson();
          }}>{t("使用示例原文", "Use sample text")}</button
        >
      {:else if lesson === 1}<label
          >{t("演示原文", "Demo source")}<textarea
            bind:value={demo}
            placeholder="Hello, how are you?"></textarea></label
        >
        <p>
          {t(
            "核对截图识别的文字，并确认原文语言和目标语言。示例：英语 → 简体中文。",
            "Check captured text and confirm the source and target languages. Example: English → Simplified Chinese.",
          )}
        </p>
        <button disabled={busy} onclick={nextLesson}
          >{t("确认，继续", "Confirm and continue")}</button
        >
      {:else if lesson === 2}<p>{demo || "Hello, how are you?"}</p>
        <p>
          {t(
            "取词后不会立即发送。检查原文，再点击“翻译”；窗口内也可按 D / S，编辑文字时不触发。",
            "Capturing text does not send it immediately. Review it, then click Translate. D / S also works in the result window when you are not editing text.",
          )}
        </p>
        <button
          disabled={busy}
          onclick={() => {
            demoResult = true;
          }}>{t("显示演示译文", "Show demo translation")}</button
        >{#if demoResult}<p class="demo-result">
            {t(
              "演示结果：你好，你好吗？（固定示例）",
              "Demo result: 你好，你好吗？ (fixed example)",
            )}
          </p>
          <button disabled={busy} onclick={nextLesson}
            >{t("继续", "Continue")}</button
          >{/if}
      {:else}<p>
          {t(
            "复制译文后粘贴到需要的位置。Esc 收起窗口；置顶可让窗口保持在其他应用前方。",
            "Copy the translation and paste it where needed. Esc hides the window; Pin keeps it above other apps.",
          )}
        </p>
        <div class="guide-actions">
          <button
            onclick={() => {
              copied = true;
            }}>{t("演示复制", "Practice copying")}</button
          ><button
            aria-pressed={demoPinned}
            onclick={() => (demoPinned = !demoPinned)}
            >{demoPinned
              ? t("取消演示置顶", "Unpin demo")
              : t("演示置顶", "Pin demo")}</button
          ><button onclick={() => (demoHidden = !demoHidden)}
            >{demoHidden
              ? t("重新显示演示", "Show demo again")
              : t("收起演示", "Hide demo")}</button
          >
        </div>
        {#if !demoHidden}<p class="demo-result">
            你好，你好吗？
          </p>{/if}{#if copied}<p role="status">
            {t(
              "已完成复制练习；演示没有改动系统剪贴板。",
              "Copy practice complete. The demo did not change your clipboard.",
            )}
          </p>{/if}
      {/if}
      <div class="guide-actions">
        {#if lesson > 0}<button
            disabled={busy}
            onclick={() =>
              run(async () => {
                await progress({ type: "lesson", lesson: lesson - 1 });
                lesson--;
              })}>{t("上一步练习", "Previous exercise")}</button
          >{/if}<button
          disabled={busy}
          onclick={() =>
            run(async () => {
              await progress({ type: "lesson", lesson: 0 });
              lesson = 0;
              demo = "";
              demoResult = false;
              copied = false;
            })}>{t("重新练习", "Restart practice")}</button
        >
      </div>
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
    <details bind:open={advanced}>
      <summary
        >{t(
          "进阶：生词、追加内容与回复",
          "More: vocabulary, adding context and replies",
        )}</summary
      >
      <p>
        {t(
          "生词候选需要你点击收录，不会自动展开侧栏。追加内容可把多段文字放到同一任务。语境解析帮助理解表达；构思回复可将你的中文意图写成对方语言的回答。生成后核对内容，再复制发送。",
          "Save vocabulary candidates explicitly; their arrival never opens the sidebar. Add text to combine context in one task. Explain context to understand an expression, or compose a reply from your intent in the other person's language. Review the result before copying and sending it.",
        )}
      </p>
    </details>
    {#if status?.progress.tutorial === "completed"}<p role="status">
        {t(
          "基础教程已完成，可随时重播。",
          "Practice completed. You can replay it any time.",
        )}
      </p>{/if}
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
              type: lesson === 3 ? "finish_tutorial" : "skip_tutorial",
            });
            await progress({ type: "finish" });
            onclose?.();
          })}
        >{lesson === 3
          ? t("完成教程", "Finish practice")
          : t("跳过练习，开始使用", "Skip practice and start")}</button
      >{/if}
  </footer>
</section>
