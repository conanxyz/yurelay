<script lang="ts">
  import { invoke, isTauri } from "@tauri-apps/api/core";

  import type { Profile, ServiceVerification } from "./types";
  import type { Locale } from "./locale";
  import { serviceName } from "./services";
  let {
    profiles,
    selected,
    locale,
    disabled = false,
    llmOnly = false,
    onChange,
  }: {
    profiles: Profile[];
    selected: string;
    locale: Locale;
    disabled?: boolean;
    llmOnly?: boolean;
    onChange: (id: string) => void;
  } = $props();
  let verification = $state<ServiceVerification[]>([]);
  const t = (zh: string, en: string) => (locale === "zh" ? zh : en);
  const choices = $derived(
    profiles.filter(
      (p) => !llmOnly || !["system_dictionary", "youdao"].includes(p.provider),
    ),
  );
  const receipt = $derived(verification.find((v) => v.profile_id === selected));
  async function refresh() {
    if (isTauri())
      try {
        verification =
          (await invoke<ServiceVerification[]>("service_verifications")) ?? [];
      } catch {
        verification = [];
      }
  }
  $effect(() => {
    void profiles.map((p) => `${p.id}:${p.revision}`).join(",");
    void refresh();
  });
</script>

<label class="query-service-picker">
  <span>{t("下次服务", "Next service")}</span>
  <select
    aria-label={t("本次查询服务", "Service for this query")}
    value={selected}
    {disabled}
    onchange={(e) => onChange(e.currentTarget.value)}
    onfocus={refresh}
  >
    {#if !choices.some((p) => p.id === selected)}<option value=""
        >{t("选择服务", "Choose service")}</option
      >{/if}
    {#each choices as profile}<option value={profile.id}
        >{serviceName(profile, locale)}{profile.model
          ? ` · ${profile.model}`
          : ""}</option
      >{/each}
  </select>
  {#if receipt?.state === "verified"}<small
      title={receipt.tested_at
        ? new Date(receipt.tested_at).toLocaleString()
        : ""}>{t("曾通过测试", "Previously tested")}</small
    >{/if}
</label>
