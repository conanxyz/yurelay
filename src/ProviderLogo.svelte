<script lang="ts">
  import Icon from "./Icons.svelte";
  import { serviceFamily } from "./services";
  import type { Profile } from "./types";
  let {
    provider,
    size = 24,
  }: { provider: Profile["provider"]; size?: number } = $props();
  const files = import.meta.glob("./assets/providers/*.svg", {
    eager: true,
    query: "?raw",
    import: "default",
  }) as Record<string, string>;
  let artwork = $derived(
    files[`./assets/providers/${serviceFamily(provider)}.svg`],
  );
</script>

<span class="provider-logo" style={`--logo-size:${size}px`} aria-hidden="true">
  {#if artwork}<span class="brand-silhouette">{@html artwork}</span>
  {:else if provider === "system_dictionary"}<Icon name="book" {size} />
  {:else if provider === "youdao"}<svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="1.5"
      ><path
        d="M4 6h16M10 3c0 5-3 8-6 10M8 10h11v10M8 10v11m0-7h11M8 18h11"
      /></svg
    >
  {:else}<svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="1.5"
      stroke-linecap="round"
      ><path d="M8 4 3 12l5 8m8-16 5 8-5 8M14 6l-4 12" /><circle
        cx="12"
        cy="12"
        r="2.5"
        fill="var(--surface, white)"
      /></svg
    >{/if}
</span>
