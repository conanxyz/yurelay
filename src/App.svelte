<script lang="ts">
  import SettingsWindow from "./SettingsWindow.svelte";
  import ResultsWindow from "./ResultsWindow.svelte";
  const settingsWindow =
    new URLSearchParams(location.search).get("window") === "settings";
</script>

{#if settingsWindow}<SettingsWindow />{:else}<ResultsWindow />{/if}
