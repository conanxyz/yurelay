<script lang="ts">
  let { size = 16 }: { size?: number } = $props();
</script>

<svg
  class="translation-loader"
  width={size}
  height={size}
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
