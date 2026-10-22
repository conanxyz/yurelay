<script lang="ts">
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import type { Profile, ServiceVerification } from "./types";
  import type { Locale } from "./locale";
  let {
    profile,
    savedProfile,
    keyChanged = false,
    revision = 0,
    locale,
  }: {
    profile: Profile;
    savedProfile?: Profile;
    keyChanged?: boolean;
    revision?: number;
    locale: Locale;
  } = $props();
  const t = (zh: string, en: string) => (locale === "zh" ? zh : en);
  let receipt = $state<ServiceVerification | null>(null);
  const modified = $derived(
    keyChanged || JSON.stringify(profile) !== JSON.stringify(savedProfile),
  );
  $effect(() => {
    const id = profile.id;
    void revision;
    let alive = true;
    receipt = null;
    if (isTauri())
      invoke<ServiceVerification[]>("service_verifications")
        .then((rows) => {
          if (alive)
            receipt = rows?.find((row) => row.profile_id === id) ?? null;
        })
        .catch(() => {});
    return () => {
      alive = false;
    };
  });
</script>

{#if profile.provider !== "system_dictionary"}<p
    class="verification-state"
    data-state={modified ? "changed" : (receipt?.state ?? "untested")}
    role="status"
  >
    {#if modified}{t(
        "表单有未保存更改；已有测试记录不适用于此草稿。",
        "This form has unsaved changes. Existing test records do not apply to this draft.",
      )}{:else if receipt?.state === "verified" && receipt.tested_at}{t(
        "此配置于以下时间通过翻译传输测试：",
        "This configuration passed a translation transport test on ",
      )}<time datetime={new Date(receipt.tested_at).toISOString()}
        >{new Date(receipt.tested_at).toLocaleString(
          locale === "zh" ? "zh-CN" : "en-US",
        )}</time
      >{:else if receipt?.state === "expired"}{t(
        "上次连接测试已过期，此服务仍可使用。",
        "The previous connection test expired. This service remains available.",
      )}{:else if receipt?.state === "failed"}{t(
        "最近一次测试未通过。",
        "The most recent test failed.",
      )}{:else}{t(
        "当前配置尚未验证。",
        "This configuration is unverified.",
      )}{/if}
  </p>{/if}
