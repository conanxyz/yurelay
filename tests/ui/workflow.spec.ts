import { test, expect, type Page } from "@playwright/test";
async function editableSource(page: Page) {
  const edit = page.getByRole("button", { name: "Edit source", exact: true });
  if ((await edit.count()) && (await edit.isEnabled())) await edit.click();
  return page.getByRole("textbox", { name: "Source text", exact: true });
}

async function setup(page: Page) {
  await page.addInitScript(() => {
    const w = window as any;
    w.isTauri = true;
    w.calls = [];
    w.operations = [];
    w.pending = [];
    w.undo = [];
    w.matches = [
      {
        start_utf16: 0,
        end_utf16: 7,
        item_id: "learning-run",
        word: "run",
        surface: "Running",
      },
    ];
    w.settings = {
      profiles: [
        {
          id: "first",
          provider: "custom",
          name: "First model",
          model: "model-a",
          endpoint: "http://localhost:8899/v1",
          endpoint_mode: "base",
          streaming: true,
          temperature: 0.2,
          revision: 1,
        },
        {
          id: "second",
          provider: "custom",
          name: "Second model",
          model: "model-b",
          endpoint: "http://localhost:8899/v1",
          endpoint_mode: "base",
          streaming: true,
          temperature: 0.2,
          revision: 1,
        },
      ],
      active_profile: "first",
      source_language: "en",
      target_language: "zh-Hans",
      ui_language: "en",
      theme: "light",
      ui_font_size: 14,
      font_size: 16,
      auto_vocabulary: true,
      encounter_highlight: true,
      failover: { enabled: false, profile_ids: [] },
      retention: { mode: "days", days: 7 },
    };
    w.task = {
      id: "reading-task",
      source_revision: 1,
      translation_source_revision: null,
      revision: 1,
      attempt: 1,
      source: "selection",
      status: "ready",
      original_text: "Running through the examples\nhelps the team learn.",
      edited_text: null,
      translation: "",
      profile: w.settings.profiles[0],
      source_language: "en",
      detected_language: "en",
      target_language: "zh-Hans",
      error: null,
      sensitive: null,
      save_state: "saved",
      save_error: null,
      has_image: false,
      image_pending: false,
      image_error: null,
      vocabulary: [],
      timings: { first_content_ms: null, total_ms: null },
      past_attempts: [],
      source_segments: [
        {
          id: "segment-a",
          kind: "selection",
          original_text: "Running through the examples\nhelps the team learn.",
          captured_at: 1,
          asset_id: null,
        },
      ],
    };
    w.words = [
      {
        id: "learning-run",
        word: "run",
        surface: "Running",
        phonetic: "",
        pos: "v.",
        dict_meaning: "Move quickly",
        context_meaning: "",
        context_sentence: "Running through the examples helps the team learn.",
        sentence_trans: "",
        difficulty: "",
        status: "learning",
        review_count: 2,
        contexts: [
          {
            id: "context-a",
            created_at: 1700000000000,
            context_sentence: "We run together.",
            context_meaning: "跑步",
            sentence_trans: "",
            surface: "run",
          },
        ],
      },
    ];
    const callbacks: Record<number, Function> = {},
      events: Record<string, number[]> = {};
    let next = 0;
    w.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
    w.emit = (event: string, payload: unknown) =>
      (events[event] ?? []).forEach((id) =>
        callbacks[id]?.({ event, payload }),
      );
    const snapshot = () => ({
      type: "snapshot",
      snapshot: { revision: w.task.revision, tasks: [structuredClone(w.task)] },
      selected: w.task.id,
      notice: null,
    });
    w.update = () => w.wire(snapshot());
    w.progress = (fields: any) => {
      const op = w.operations.at(-1);
      Object.assign(op, fields, {
        revision: op.revision + 1,
        updated_at: op.updated_at + 1,
      });
      w.emit("operation-changed", structuredClone(op));
    };
    const changed = (text: string) => {
      w.undo.push(structuredClone(w.task));
      w.task.edited_text = text;
      w.task.source_revision++;
      w.task.revision++;
      w.update();
      return structuredClone(w.task);
    };
    w.__TAURI_INTERNALS__ = {
      transformCallback: (fn: Function) => {
        callbacks[++next] = fn;
        return next;
      },
      unregisterCallback: (id: number) => delete callbacks[id],
      invoke: async (cmd: string, args: any) => {
        w.calls.push({ cmd, args });
        if (cmd === "plugin:event|listen") {
          (events[args.event] ??= []).push(args.handler);
          return next;
        }
        if (cmd === "bootstrap")
          return {
            ...snapshot(),
            configuration: {
              settings: w.settings,
              has_keys: [],
              credential_errors: {},
            },
            permissions: { accessibility: true, screen: true },
            pinned: false,
            expanded: false,
          };
        if (cmd === "configuration_settings")
          return structuredClone(w.settings);
        if (cmd === "permissions") return { accessibility: true, screen: true };
        if (cmd === "subscribe") {
          w.wire = (message: any) => args.onEvent.onmessage(message);
          return "reading-test";
        }
        if (cmd === "detect_text_language") return "en";
        if (cmd === "source_pending") return structuredClone(w.pending);
        if (cmd === "source_assets") return w.assets ?? [];
        if (cmd === "source_asset")
          return Array.from(
            Uint8Array.from(
              atob(
                "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aDPsAAAAASUVORK5CYII=",
              ),
              (c) => c.charCodeAt(0),
            ),
          );
        if (cmd === "source_update") {
          w.task.source_language =
            args.request.source_language ?? w.task.source_language;
          w.task.target_language =
            args.request.target_language ?? w.task.target_language;
          return changed(args.request.text);
        }
        if (cmd === "source_undo_cleanup") {
          w.task.can_undo_cleanup = false;
          return changed(w.task.original_text);
        }
        if (cmd === "source_clean_preview")
          return {
            text: (w.task.edited_text ?? w.task.original_text).replace(
              "\n",
              " ",
            ),
            changes: 1,
            uncertain: 0,
            rules: ["soft_line_break"],
          };
        if (cmd === "source_clean_apply")
          return changed(
            (w.task.edited_text ?? w.task.original_text).replace("\n", " "),
          );
        if (cmd === "source_undo") {
          const previous = w.undo.pop();
          if (previous) {
            previous.source_revision = w.task.source_revision + 1;
            previous.revision = w.task.revision + 1;
            w.task = previous;
            w.update();
          }
          return structuredClone(w.task);
        }
        if (cmd === "source_encounters") {
          if (w.deferMatches)
            return new Promise((resolve) => (w.resolveMatches = resolve));
          return {
            task_id: w.task.id,
            source_revision: w.task.source_revision,
            matches: w.matches,
          };
        }
        if (cmd === "service_verifications")
          return [
            {
              profile_id: "first",
              state: "verified",
              tested_at: Date.now(),
              expires_at: Date.now() + 86400000,
              tested_capability: "translation_transport",
            },
          ];
        if (cmd === "vocabulary_lookup") return structuredClone(w.words);
        if (cmd === "vocabulary_preview")
          return { candidate: { ...w.words[0] }, translation_highlights: [] };
        if (cmd === "vocabulary_update") {
          w.words[0].status = args.status;
          w.matches = [];
          w.emit("vocabulary-changed", null);
          return;
        }
        if (cmd === "speech_voices") return [];
        if (cmd === "operation_list")
          return structuredClone(
            w.operations.filter((o: any) => o.task_id === args.taskId),
          );
        if (cmd === "submit_operation") {
          const r = args.request;
          const previous = w.operations.find(
            (o: any) => o.id === r.previous_operation_id,
          );
          const op = {
            id: previous?.id ?? `op-${w.operations.length}`,
            request_id: r.request_id,
            task_id: r.task_id,
            source_revision: r.source_revision ?? 0,
            kind: r.kind,
            status: "generating",
            primary_text: "",
            back_translation: "",
            back_translation_state: "none",
            profile: w.settings.profiles.find(
              (p: any) => p.id === r.profile_id,
            ),
            attempt: (previous?.attempt ?? 0) + 1,
            attempts: [],
            error: null,
            sensitive: null,
            output_language: r.reply_language || r.target_language,
            reference_language: r.target_language,
            created_at: Date.now(),
            updated_at: Date.now(),
            revision: (previous?.revision ?? 0) + 1,
          };
          w.operations = w.operations.filter((o: any) => o.id !== op.id);
          w.operations.push(op);
          return structuredClone(op);
        }
        if (cmd === "cancel_operation") {
          w.progress({
            status: "stopped",
            back_translation_state: "cancelled",
          });
          return;
        }
        if (cmd === "resolve_operation_sensitive") {
          w.progress({
            status: args.allow ? "generating" : "stopped",
            sensitive: null,
          });
          return structuredClone(w.operations.at(-1));
        }
        if (cmd === "retry_task") {
          w.task.status = "queued";
          w.task.revision++;
          w.update();
          return;
        }
        return null;
      },
    };
  });
  await page.goto("/");
  await expect(
    page.getByRole("textbox", { name: "Source reading", exact: true }),
  ).toHaveText("Running through the examples\nhelps the team learn.");
}
const remote = (page: Page) =>
  page.evaluate(() =>
    (window as any).calls.filter((c: any) =>
      ["submit_operation", "retry_task", "translate_text"].includes(c.cmd),
    ),
  );
async function select(page: Page, start: number, end: number) {
  await (
    await editableSource(page)
  ).evaluate(
    (el: HTMLTextAreaElement, r) => {
      el.focus();
      el.setSelectionRange(r.start, r.end);
      el.dispatchEvent(new Event("select", { bubbles: true }));
    },
    { start, end },
  );
}

test("reply remains local until Generate; streaming copy excludes back-translation and closing preserves the operation", async ({
  page,
}) => {
  await setup(page);
  await page
    .getByRole("button", { name: "Compose reply", exact: true })
    .click();
  await page
    .getByRole("textbox", { name: "Reply intent" })
    .fill("Thank them for the examples.");
  await expect(page.getByRole("dialog").getByRole("combobox")).toHaveCount(0);
  await page.evaluate(() => {
    const w = window as any;
    w.settings.active_profile = "second";
    w.emit("configuration-changed", structuredClone(w.settings));
  });
  await expect(page.locator(".service-identity")).toContainText("Second model");
  await page
    .getByLabel("Translation language", { exact: true })
    .selectOption("fr");
  expect(await remote(page)).toEqual([]);
  await page
    .getByRole("button", { name: "Generate reply", exact: true })
    .click();
  await expect.poll(() => remote(page).then((r) => r.length)).toBe(1);
  expect((await remote(page))[0].args.request).toMatchObject({
    kind: "reply",
    profile_id: "second",
    tone: "default",
    reply_language: "en",
    target_language: "fr",
    source_revision: 1,
  });
  await page.evaluate(() =>
    (window as any).progress({
      primary_text: "Thank you",
      back_translation_state: "none",
    }),
  );
  await expect(
    page.getByRole("button", { name: "Copy reply", exact: true }),
  ).toBeDisabled();
  await page.evaluate(() =>
    (window as any).progress({
      primary_text: "Thank you for the examples.",
      back_translation: "Merci pour les exemples.",
      back_translation_state: "pending",
    }),
  );
  await expect(
    page.getByRole("button", { name: "Copy reply", exact: true }),
  ).toBeEnabled();
  await page.getByRole("button", { name: "Copy reply", exact: true }).click();
  expect(
    await page.evaluate(
      () =>
        (window as any).calls.filter((c: any) => c.cmd === "copy_text").at(-1)
          .args.text,
    ),
  ).toBe("Thank you for the examples.");
  await page.getByRole("button", { name: "Close assistant panel" }).click();
  expect(
    await page.evaluate(() =>
      (window as any).calls.filter((c: any) => c.cmd === "cancel_operation"),
    ),
  ).toEqual([]);
  await page
    .getByRole("button", { name: "Compose reply", exact: true })
    .click();
  await expect(
    page.getByRole("region", { name: "Generated reply" }),
  ).toContainText("Thank you for the examples.");
  await page.getByRole("button", { name: "Stop reply", exact: true }).click();
  expect(
    await page.evaluate(() =>
      (window as any).calls.filter((c: any) => c.cmd === "stop_task"),
    ),
  ).toEqual([]);
  await expect(
    page.getByRole("button", { name: "Regenerate reply" }),
  ).toBeEnabled();
  await page.getByRole("button", { name: "Regenerate reply" }).click();
  expect((await remote(page)).at(-1).args.request.previous_operation_id).toBe(
    "op-0",
  );
  await page.evaluate(() =>
    (window as any).progress({
      status: "completed",
      primary_text:
        "Thank you for sharing the examples. They make the workflow much clearer.",
      back_translation:
        "Merci pour ces exemples. Ils rendent le fonctionnement beaucoup plus clair.",
      back_translation_state: "complete",
    }),
  );
  await page.setViewportSize({ width: 680, height: 950 });
  await page.locator(".back-translation summary").click();
  await page.screenshot({
    path: "artifacts/previews/reading-workflow-reply-light.png",
    fullPage: true,
  });
});

test("long source selections offer explicit context explanation without automatic preview or remote dispatch", async ({
  page,
}) => {
  await setup(page);
  await select(page, 0, 27);
  await expect(
    page.getByRole("button", { name: "Explain context", exact: true }),
  ).toBeEnabled();
  expect(await remote(page)).toEqual([]);
  expect(
    await page.evaluate(() =>
      (window as any).calls.filter((c: any) => c.cmd === "vocabulary_preview"),
    ),
  ).toEqual([]);
  await page
    .getByRole("button", { name: "Explain context", exact: true })
    .click();
  await page.getByLabel("Explanation context scope").selectOption("full");
  expect(await remote(page)).toEqual([]);
  await page.getByRole("button", { name: "Generate explanation" }).click();
  expect((await remote(page))[0].args.request).toMatchObject({
    kind: "explain",
    selection_start: 0,
    selection_end: 27,
    context_scope: "full",
  });
  await page.evaluate(() =>
    (window as any).progress({
      status: "completed",
      primary_text: "Here, running through means reviewing.",
    }),
  );
  await (await editableSource(page)).fill("A new source draft.");
  await expect(page.locator(".stale-result")).toContainText("previous");
  expect(await remote(page)).toHaveLength(1);
});

test("source controls stay minimal and only offer cleanup undo after an actual automatic change", async ({
  page,
}) => {
  await setup(page);
  await expect(
    page.getByRole("button", { name: "Read source", exact: true }),
  ).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "Preview cleanup", exact: true }),
  ).toHaveCount(0);
  await expect(page.getByLabel("Cleanup mode")).toHaveCount(0);
  await expect(page.getByLabel("More source actions")).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "Undo cleanup", exact: true }),
  ).toHaveCount(0);
  await page.evaluate(() => {
    const w = window as any;
    w.task.edited_text = "Running through the examples helps the team learn.";
    w.task.can_undo_cleanup = true;
    w.task.source_revision++;
    w.task.revision++;
    w.update();
  });
  await expect(
    page.getByRole("textbox", { name: "Source reading" }),
  ).toHaveText("Running through the examples helps the team learn.");
  await page.getByRole("button", { name: "Undo cleanup", exact: true }).click();
  await expect(
    page.getByRole("textbox", { name: "Source reading" }),
  ).toHaveText("Running through the examples\nhelps the team learn.");
  await expect(
    page.getByRole("button", { name: "Undo cleanup", exact: true }),
  ).toHaveCount(0);
  expect(await remote(page)).toEqual([]);
  expect(
    await page.evaluate(() =>
      (window as any).calls.filter((c: any) =>
        ["source_clean_apply", "source_clean_preview"].includes(c.cmd),
      ),
    ),
  ).toEqual([]);
});

test("learning encounters reuse sidebar contexts without collection and disappear on mastery", async ({
  page,
}) => {
  await setup(page);
  await page.setViewportSize({ width: 680, height: 800 });
  await expect(page.locator(".encounter-word")).toHaveText("Running");
  const before = await page.locator(".source-reading-view").boundingBox();
  await page.locator(".encounter-word").click();
  await expect(page.locator(".vocabulary-row-detail")).toContainText(
    "Move quickly",
  );
  await page.getByText("Saved contexts", { exact: false }).click();
  await expect(page.locator(".saved-word-contexts")).toContainText(
    "We run together.",
  );
  await expect(page.locator(".saved-word-contexts time")).toContainText("2023");
  await page.screenshot({
    path: "artifacts/previews/reading-workflow-encounter-light.png",
    fullPage: true,
  });
  await expect(page.locator(".source-panel .workflow-tray")).toHaveCount(0);
  expect(
    await page.evaluate(() =>
      (window as any).calls.filter((c: any) => c.cmd === "vocabulary_collect"),
    ),
  ).toEqual([]);
  expect(await remote(page)).toEqual([]);
  await page.getByRole("button", { name: "Mark as mastered" }).click();
  await expect(page.locator(".encounter-word")).toHaveCount(0);
  await page.getByRole("button", { name: "Edit source", exact: true }).click();
  await expect(await editableSource(page)).toHaveValue(
    "Running through the examples\nhelps the team learn.",
  );
  expect(before).not.toBeNull();
});

test("late encounter matches cannot mark an edited source and pending captures expose no blocked text", async ({
  page,
}) => {
  await setup(page);
  await page.evaluate(() => ((window as any).deferMatches = true));
  await page.evaluate(() => {
    const w = window as any;
    w.task.source_revision++;
    w.task.revision++;
    w.update();
  });
  await expect
    .poll(() => page.evaluate(() => typeof (window as any).resolveMatches))
    .toBe("function");
  await page.getByRole("button", { name: "Edit source", exact: true }).click();
  await (await editableSource(page)).fill("Changed source.");
  await page.evaluate(() =>
    (window as any).resolveMatches({
      matches: [
        { start_utf16: 0, end_utf16: 7, item_id: "learning-run", word: "run" },
      ],
    }),
  );
  await expect(page.locator(".encounter-word")).toHaveCount(0);
  await page.evaluate(() => {
    const w = window as any;
    w.pending = [
      {
        id: "pending",
        task_id: w.task.id,
        expected_revision: 1,
        reason: "sensitive",
        sensitive: "API_SECRET",
      },
    ];
    w.task.revision++;
    w.update();
    w.emit("source-pending-changed", null);
  });
  await expect(
    page.getByRole("button", { name: "Discard capture" }),
  ).toBeVisible();
  await expect(await editableSource(page)).toHaveValue("Changed source.");
  expect(await remote(page)).toEqual([]);
});

test("main translation uses the configured default without a next-service selector", async ({ page }) => {
  await setup(page);
  await expect(page.getByLabel("Service for this query", { exact: true })).toHaveCount(0);
  await page.evaluate(() => {
    const w = window as any;
    w.settings.active_profile = "second";
    w.emit("configuration-changed", structuredClone(w.settings));
  });
  await expect(page.locator(".service-identity")).toContainText("Second model");
  expect(await remote(page)).toEqual([]);
  await page.locator(".disclosure-control").click();
  expect((await remote(page))[0].args.profileId).toBe("second");
});

for (const restored of [false, true]) {
  test(`automatic fallback keeps its result visible and retries the primary service (${restored ? "restored" : "live"})`, async ({ page }) => {
    await setup(page);
    await page.locator(".disclosure-control").click();
    await page.evaluate((restored) => {
      const w = window as any;
      w.task.past_attempts = [{
        number: 1, status: "failed", translation: "", reason: "failover",
        profile: structuredClone(w.settings.profiles[0]),
      }];
      Object.assign(w.task, {
        id: restored ? "restored-fallback" : w.task.id,
        status: "completed", attempt: 2,
        profile: structuredClone(w.settings.profiles[1]),
        translation: "Synthetic fallback translation.",
        translation_source_revision: w.task.source_revision,
        revision: w.task.revision + 1,
      });
      w.update();
    }, restored);
    await expect(page.locator(".service-identity")).toContainText("Second model");
    await expect(page.locator("#translation-body")).toContainText("Synthetic fallback translation.");
    await expect(page.getByText("Previous result", { exact: false })).toHaveCount(0);
    await page.getByRole("button", { name: "Translate again", exact: true }).click();
    const request = (await remote(page)).at(-1);
    expect(request.args.profileId ?? "first").toBe("first");
    expect(await page.evaluate(() => (window as any).settings.active_profile)).toBe("first");
  });
}

test("math enhancement is local and safe, preserves formula source on copy and keeps code and prices literal", async ({
  page,
}) => {
  await setup(page);
  const markdown =
    "Formula $x^2 + 1$. Price $12.50 and $8.00.\n\n`$x$`\n\n$$\n\\frac{a}{b}\n$$\n\n$\\href{javascript:alert(1)}{bad}$\n\n| Item | Amount |\n| --- | --- |\n| A | 12 |";
  await page.evaluate((translation) => {
    const w = window as any;
    Object.assign(w.task, {
      status: "completed",
      translation,
      translation_source_revision: w.task.source_revision,
      revision: 2,
    });
    w.update();
  }, markdown);
  await page.locator(".disclosure-control").click();
  await expect(page.locator(".katex")).toHaveCount(2);
  await expect(page.locator(".translation-prose")).toContainText(
    "Price $12.50 and $8.00.",
  );
  await expect(page.locator(".translation-prose code")).toHaveText("$x$");
  await expect(
    page.locator('.translation-prose a[href^="javascript"]'),
  ).toHaveCount(0);
  await expect(page.locator(".translation-prose table")).toHaveCount(1);
  await page.getByRole("button", { name: "Copy table", exact: true }).click();
  expect(
    await page.evaluate(
      () =>
        (window as any).calls.filter((c: any) => c.cmd === "copy_text").at(-1)
          .args.text,
    ),
  ).toBe("Item\tAmount\nA\t12");
  await page
    .getByRole("button", { name: "Copy translation", exact: true })
    .click();
  const copied = await page.evaluate(
    () =>
      (window as any).calls.filter((c: any) => c.cmd === "copy_text").at(-1)
        .args.text,
  );
  expect(copied).toContain("$x^2 + 1$");
  expect(copied.match(/\\frac/g)).toHaveLength(1);
  await page
    .getByRole("button", { name: "Copy Markdown", exact: true })
    .click();
  expect(
    await page.evaluate(
      () =>
        (window as any).calls.filter((c: any) => c.cmd === "copy_text").at(-1)
          .args.text,
    ),
  ).toBe(markdown);
  expect(await remote(page)).toEqual([]);
  await page.getByRole("button", { name: "Math layout" }).click();
  await expect(page.locator(".katex")).toHaveCount(0);
  await expect(page.locator(".translation-prose table")).toHaveCount(1);
});

test("reply drawer supports dark narrow windows, reduced motion and Escape focus restoration", async ({
  page,
}) => {
  await setup(page);
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.setViewportSize({ width: 360, height: 780 });
  await page.evaluate(() => {
    const w = window as any;
    w.settings.theme = "dark";
    w.emit("configuration-changed", w.settings);
  });
  const trigger = page.getByRole("button", {
    name: "Compose reply",
    exact: true,
  });
  await trigger.click();
  await expect(page.getByRole("dialog")).toBeFocused();
  await page
    .getByRole("textbox", { name: "Reply intent" })
    .fill("A short acknowledgment.");
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth > innerWidth,
    ),
  ).toBe(false);
  expect(
    await page
      .getByRole("button", { name: "Generate reply" })
      .evaluate((el) => getComputedStyle(el).transitionDuration),
  ).toBe("0s");
  await page.screenshot({
    path: "artifacts/previews/reading-workflow-narrow-dark.png",
    fullPage: true,
  });
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(trigger).toBeFocused();
  expect(await remote(page)).toEqual([]);
  expect(
    await page.evaluate(() =>
      (window as any).calls.filter((c: any) => c.cmd === "hide_window"),
    ),
  ).toEqual([]);
});

test("operation events reject old revisions, retain a single past attempt and keep incomplete back-translation separate", async ({
  page,
}) => {
  await setup(page);
  await page
    .getByRole("button", { name: "Compose reply", exact: true })
    .click();
  await page.getByRole("textbox", { name: "Reply intent" }).fill("Thank them.");
  await page
    .getByRole("button", { name: "Generate reply", exact: true })
    .click();
  await page.evaluate(() => {
    const w = window as any;
    w.progress({
      status: "completed",
      primary_text: "Thank you.",
      back_translation_state: "failed",
      error: "回复已生成，未获得完整回译参考。",
      attempts: [
        {
          number: 1,
          profile: w.settings.profiles[0],
          status: "failed",
          error: "服务配置已变化，请重新生成。",
          reason: "manual_retry",
          partial_text: "Earlier partial response",
          source_revision: 1,
          back_translation: "",
        },
      ],
    });
    const old = structuredClone(w.operations.at(-1));
    old.revision = 0;
    old.primary_text = "Wrong stale output";
    w.emit("operation-changed", old);
  });
  await expect(
    page.getByRole("region", { name: "Generated reply" }),
  ).toContainText("Thank you.");
  await expect(
    page.getByRole("region", { name: "Generated reply" }),
  ).not.toContainText("Wrong stale output");
  await expect(page.getByRole("button", { name: "Copy reply" })).toBeEnabled();
  await page.getByText("Previous attempts", { exact: true }).click();
  await expect(page.locator(".operation-attempts")).toContainText(
    "Earlier partial response",
  );
  await expect(page.locator(".operation-attempts")).toContainText(
    "configuration changed",
  );
  await expect(page.locator(".operation-attempts")).toContainText(
    "40 operations",
  );
  await expect(page.locator(".operation-output>.inline-feedback")).toHaveText(
    "The reply is available, but the back-translation is incomplete.",
  );
});

test("multi-image source preview requests only the chosen safe asset", async ({
  page,
}) => {
  await setup(page);
  await page.evaluate(() => {
    const w = window as any;
    w.assets = [
      { id: "image-one", segment_id: "segment-b", legacy: false },
      { id: "image-two", segment_id: "segment-c", legacy: false },
    ];
    w.task.source_segments.push(
      {
        id: "segment-b",
        kind: "screenshot",
        original_text: "First image text.",
        captured_at: 2,
        asset_id: "image-one",
      },
      {
        id: "segment-c",
        kind: "screenshot",
        original_text: "Second image text.",
        captured_at: 3,
        asset_id: "image-two",
      },
    );
    w.task.source_revision++;
    w.task.revision++;
    w.update();
  });
  await page.locator(".source-segments summary").click();
  await expect(page.locator(".source-segments")).toContainText(
    "3 source segments · 2 screenshots",
  );
  expect(
    await page.evaluate(() =>
      (window as any).calls.filter((c: any) => c.cmd === "source_asset"),
    ),
  ).toEqual([]);
  await page
    .getByRole("button", { name: "View screenshot 2", exact: true })
    .click();
  await expect(page.locator(".source-segments img")).toHaveCount(1);
  await expect
    .poll(() =>
      page
        .locator(".source-segments img")
        .evaluate((img: HTMLImageElement) => img.naturalWidth),
    )
    .toBe(1);
  expect(
    await page.evaluate(
      () =>
        (window as any).calls
          .filter((c: any) => c.cmd === "source_asset")
          .at(-1).args,
    ),
  ).toEqual({ taskId: "reading-task", assetId: "image-two" });
  expect(await remote(page)).toEqual([]);
});

test("orphaned pending captures remain actionable without an active draft", async ({
  page,
}) => {
  await setup(page);
  await page.evaluate(() => {
    const w = window as any;
    w.pending = [
      {
        id: "pending-orphan",
        task_id: "deleted-task",
        expected_revision: 4,
        reason: "stale",
        sensitive: null,
      },
    ];
    w.wire({
      type: "snapshot",
      snapshot: { revision: 2, tasks: [] },
      selected: null,
      notice: null,
    });
    w.emit("source-pending-changed", null);
  });
  await expect(
    page.getByRole("button", { name: "Keep as new draft" }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Add to current draft" }),
  ).toBeDisabled();
  expect(
    await page.evaluate(
      () =>
        (window as any).calls
          .filter((c: any) => c.cmd === "source_pending")
          .at(-1).args.taskId,
    ),
  ).toBeNull();
  await page.getByRole("button", { name: "Discard capture" }).click();
  await expect(page.locator(".pending-capture")).toHaveCount(0);
  expect(await remote(page)).toEqual([]);
});

test("append pauses on unsaved source edits, checkpoints before re-enabling and closes on receiver switch", async ({
  page,
}) => {
  await setup(page);
  await page.getByRole("button", { name: "Append", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Appending", exact: true }),
  ).toHaveAttribute("aria-pressed", "true");
  await (await editableSource(page)).fill("Updated local source.");
  await expect(
    page.getByRole("button", { name: "Append", exact: true }),
  ).toHaveAttribute("aria-pressed", "false");
  await expect(
    page.getByText("Append paused after editing.", { exact: false }),
  ).toBeVisible();
  expect(
    await page.evaluate(
      () =>
        (window as any).calls
          .filter((c: any) => c.cmd === "set_append_mode")
          .at(-1).args.enabled,
    ),
  ).toBe(false);
  await page.evaluate(() => ((window as any).calls = []));
  await page.getByRole("button", { name: "Append", exact: true }).click();
  const calls = await page.evaluate(() =>
    (window as any).calls.filter((c: any) =>
      ["source_update", "set_append_mode"].includes(c.cmd),
    ),
  );
  expect(calls[0].cmd).toBe("source_update");
  expect(calls.at(-1)).toMatchObject({
    cmd: "set_append_mode",
    args: { enabled: true, taskId: "reading-task", expectedRevision: 2 },
  });
  await page.evaluate(() => {
    const w = window as any;
    Object.assign(w.task, {
      id: "history-receiver",
      original_text: "Another source.",
      edited_text: null,
      source_revision: 5,
      revision: 6,
    });
    w.update();
  });
  await expect(
    page.getByRole("button", { name: "Append", exact: true }),
  ).toHaveAttribute("aria-pressed", "false");
  expect(
    await page.evaluate(
      () =>
        (window as any).calls
          .filter((c: any) => c.cmd === "set_append_mode")
          .at(-1).args.enabled,
    ),
  ).toBe(false);
  expect(await remote(page)).toEqual([]);
});

test("a late source checkpoint preserves newer local edits", async ({
  page,
}) => {
  await setup(page);
  await (await editableSource(page)).fill("First local edit.");
  await page.evaluate(() => {
    const w = window as any,
      original = w.__TAURI_INTERNALS__.invoke;
    w.__TAURI_INTERNALS__.invoke = (cmd: string, args: any) =>
      cmd === "source_update"
        ? new Promise((resolve) => {
            w.finishUpdate = () => original(cmd, args).then(resolve);
          })
        : original(cmd, args);
  });
  await page.getByRole("button", { name: "Append", exact: true }).click();
  await expect
    .poll(() => page.evaluate(() => typeof (window as any).finishUpdate))
    .toBe("function");
  await (await editableSource(page)).fill("Newer unsaved edit.");
  await page.evaluate(() => (window as any).finishUpdate());
  await expect(await editableSource(page)).toHaveValue("Newer unsaved edit.");
  await expect(
    page.getByRole("button", { name: "Append", exact: true }),
  ).toHaveAttribute("aria-pressed", "false");
  expect(await remote(page)).toEqual([]);
});

for (const change of ["edit", "append"] as const) {
  test(`committed source ${change} marks the previous translation stale until explicit D/S`, async ({ page }) => {
    await setup(page);
    await page.evaluate(() => {
      const w = window as any;
      Object.assign(w.task, {
        status: "completed",
        translation: "Prior synthetic translation.",
        translation_source_revision: 1,
        revision: 2,
      });
      w.update();
    });
    await page.getByRole("button", { name: "Expand translation", exact: true }).click();
    const text = change === "edit"
      ? "Changed synthetic source."
      : "Running through the examples\nhelps the team learn.\n\nAn appended passage.";
    if (change === "edit") {
      await (await editableSource(page)).fill(text);
      await page.getByRole("button", { name: "Finish editing", exact: true }).click();
    } else {
      await page.evaluate((text) => {
        const w = window as any;
        w.task.original_text = text;
        w.task.source_segments.push({ id: "segment-b", kind: "selection", original_text: "An appended passage.", captured_at: 2, asset_id: null });
        w.task.source_revision++;
        w.task.revision++;
        w.update();
      }, text);
    }
    const source = page.getByRole("textbox", { name: "Source reading", exact: true });
    await expect(source).toHaveText(text);
    await expect(page.locator(".previous-translation")).toContainText("Prior synthetic translation.");
    await expect(page.getByRole("region", { name: "Translation", exact: true })).toHaveCount(0);
    await expect(page.getByRole("button", { name: "Translate and expand (D / S)", exact: true })).toBeEnabled();
    expect(await remote(page)).toEqual([]);
    await source.focus();
    await page.keyboard.press("s");
    await expect.poll(() => remote(page).then((calls) => calls.length)).toBe(1);
    expect((await remote(page))[0]).toMatchObject({
      cmd: "retry_task",
      args: { id: "reading-task", editedText: text, sourceLanguage: "en", targetLanguage: "zh-Hans" },
    });
  });
}

test("legacy translations remain readable and first-time drafts wait for explicit submission", async ({ page }) => {
  await setup(page);
  await expect(page.locator(".previous-translation")).toHaveCount(0);
  expect(await remote(page)).toEqual([]);
  await page.evaluate(() => {
    const w = window as any;
    Object.assign(w.task, { status: "completed", translation: "Legacy synthetic translation.", revision: 2 });
    delete w.task.source_revision;
    delete w.task.translation_source_revision;
    w.update();
  });
  await page.getByRole("button", { name: "Expand translation", exact: true }).click();
  await expect(page.getByRole("region", { name: "Translation", exact: true })).toContainText("Legacy synthetic translation.");
  await expect(page.locator(".previous-translation")).toHaveCount(0);
  expect(await remote(page)).toEqual([]);
  await page.evaluate(() => {
    const w = window as any;
    Object.assign(w.task, { edited_text: "Edited legacy source.", source_revision: 1, revision: 3 });
    w.update();
  });
  await expect(page.locator(".previous-translation")).toContainText("Legacy synthetic translation.");
  await expect(page.getByRole("button", { name: "Translate and expand (D / S)", exact: true })).toBeEnabled();
  expect(await remote(page)).toEqual([]);
});

test("captured source opens directly in reading mode and selecting a passage remains local", async ({
  page,
}) => {
  await setup(page);
  const reading = page.getByRole("textbox", {
    name: "Source reading",
    exact: true,
  });
  await expect(reading).toBeVisible();
  await expect(
    page.getByRole("textbox", { name: "Source text", exact: true }),
  ).toHaveCount(0);
  await expect(page.locator(".source-workflow-toolbar button")).toHaveCount(1);
  await expect(page.locator(".encounter-word")).toHaveText("Running");
  await reading.evaluate((el) => {
    const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
    const nodes: Text[] = [];
    let node;
    while ((node = walker.nextNode())) nodes.push(node as Text);
    const all = nodes.map((n) => n.data).join("");
    const offset = all.indexOf("Running");
    let position = 0,
      start: Text | undefined,
      end: Text | undefined,
      startOffset = 0,
      endOffset = 0;
    for (const n of nodes) {
      if (!start && position + n.length > offset) {
        start = n;
        startOffset = offset - position;
      }
      if (position + n.length >= offset + 27) {
        end = n;
        endOffset = offset + 27 - position;
        break;
      }
      position += n.length;
    }
    const r = document.createRange();
    r.setStart(start!, startOffset);
    r.setEnd(end!, endOffset);
    const selection = getSelection()!;
    selection.removeAllRanges();
    selection.addRange(r);
    el.dispatchEvent(new MouseEvent("mouseup", { bubbles: true, button: 0 }));
  });
  await expect(
    page.getByRole("button", { name: "Explain context", exact: true }),
  ).toBeEnabled();
  expect(await remote(page)).toEqual([]);
  await page
    .getByRole("button", { name: "Explain context", exact: true })
    .click();
  await page.getByRole("button", { name: "Generate explanation" }).click();
  expect((await remote(page))[0].args.request).toMatchObject({
    selection_start: 0,
    selection_end: 27,
  });
});

test("unknown reply language points to the existing source language control without adding compose controls", async ({
  page,
}) => {
  await setup(page);
  await page.getByLabel("Source language", { exact: true }).selectOption("");
  await page.evaluate(() => {
    const w = window as any;
    w.task.detected_language = null;
    w.task.source_language = null;
    w.task.original_text = "12345";
    w.task.revision++;
    w.update();
    const original = w.__TAURI_INTERNALS__.invoke;
    w.__TAURI_INTERNALS__.invoke = (cmd: string, args: any) =>
      cmd === "detect_text_language"
        ? Promise.resolve(null)
        : original(cmd, args);
  });
  await page
    .getByRole("button", { name: "Compose reply", exact: true })
    .click();
  await page
    .getByRole("textbox", { name: "Reply intent" })
    .fill("A short acknowledgment.");
  await page.getByRole("button", { name: "Generate reply" }).click();
  await expect(page.getByRole("dialog")).toContainText(
    "Choose the source language above",
  );
  await expect(page.getByRole("dialog").getByRole("combobox")).toHaveCount(0);
  expect(await remote(page)).toEqual([]);
});

test("a single nonlegacy screenshot is visible and loads only when opened", async ({
  page,
}) => {
  await setup(page);
  await page.evaluate(() => {
    const w = window as any;
    w.assets = [
      { id: "new-draft-image", segment_id: "image-segment", legacy: false },
    ];
    w.task.source_segments = [
      {
        id: "image-segment",
        kind: "screenshot",
        original_text: w.task.original_text,
        captured_at: 1,
        asset_id: "new-draft-image",
      },
    ];
    w.task.has_image = true;
    w.task.source_revision++;
    w.task.revision++;
    w.update();
  });
  await expect(page.locator(".source-segments summary")).toHaveText(
    "View screenshot",
  );
  await expect(page.locator(".source-image")).toHaveCount(0);
  await expect(page.locator(".source-segments img")).toHaveCount(0);
  expect(
    await page.evaluate(() =>
      (window as any).calls.filter((c: any) => c.cmd === "source_asset"),
    ),
  ).toEqual([]);
  await page.locator(".source-segments summary").click();
  await expect
    .poll(() =>
      page
        .locator(".source-segments img")
        .evaluate((img: HTMLImageElement) => img.naturalWidth),
    )
    .toBe(1);
  expect(
    await page.evaluate(
      () =>
        (window as any).calls
          .filter((c: any) => c.cmd === "source_asset")
          .at(-1).args.assetId,
    ),
  ).toBe("new-draft-image");
  expect(await remote(page)).toEqual([]);
});

test('reply regeneration sends the latest intent separately from quoted examples and copies only the reply',async({page})=>{
 await setup(page);
 const context='Tell us who handles support ("Alice is our technical contact"). Good news: the node is healthy again.';
 await page.evaluate(context=>{const w=window as any;w.task.original_text=context;w.task.edited_text=null;w.task.source_revision++;w.task.revision++;w.update();},context);
 await page.getByRole('button',{name:'Compose reply',exact:true}).click();
 let previous:string|null=null;
 for(const [index,name] of ['Morgan','Riley'].entries()){
  await page.getByRole('textbox',{name:'Reply intent'}).fill(`${name}是我们的技术联系人。`);
  await page.getByRole('button',{name:index?'Regenerate reply':'Generate reply',exact:true}).click();
  await expect.poll(async()=> (await remote(page)).length).toBe(index+1);
  const request=(await remote(page)).at(-1).args.request;
  expect(request).toMatchObject({source_text:context,intent:`${name}是我们的技术联系人。`,reply_language:'en',target_language:'zh-Hans',include_back_translation:true,previous_operation_id:previous});
  previous=await page.evaluate(()=> (window as any).operations.at(-1).id);
  await page.evaluate(name=>(window as any).progress({status:'completed',primary_text:`${name} is our technical contact.`,back_translation:`${name}是我们的技术联系人。`,back_translation_state:'complete',error:null}),name);
  const output=page.getByRole('region',{name:'Generated reply'});await expect(output).toContainText(`${name} is our technical contact.`);
  await page.getByRole('button',{name:'Copy reply',exact:true}).click();
  expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.cmd==='copy_text').at(-1).args.text)).toBe(`${name} is our technical contact.`);
 }
});
