<script lang="ts">
  import type { Settings } from "./types";
  import type { Locale } from "./locale";
  let {
    settings = $bindable(),
    locale,
  }: { settings: Settings; locale: Locale } = $props();
  const t = (zh: string, en: string) => (locale === "zh" ? zh : en);
</script>

<label class="extraction-setting fallback-setting">
  <input
    type="checkbox"
    checked={settings.failover?.enabled ?? false}
    onchange={(e) =>
      (settings.failover = {
        ...(settings.failover ?? { profile_ids: [] }),
        enabled: e.currentTarget.checked,
      })}
  />
  <span
    >{t("自动备用服务", "Automatic fallback services")}<small class="subtle"
      >{t(
        "首段返回前失败时，尝试其他已配置服务；同一内容可能发送给其他供应商并产生费用。",
        "If a request fails before its first result, try another configured service; the same content may reach another provider and incur charges.",
      )}</small
    ></span
  >
</label>
