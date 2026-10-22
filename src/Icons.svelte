<script lang="ts">
  let { name, size = 20 }: { name: string; size?: number } = $props();
  const paths: Record<string, string> = {
    edit: "m15 4 5 5M4 20l4-1L20 7a2 2 0 0 0-4-4L4 15v5Z",
    copy: "M8 8h12v12H8V8ZM4 16V4h12",
    close: "m6 6 12 12M18 6 6 18",
    book: "M12 5c-3-2-6-2-9-1v15c3-1 6-1 9 1m0-15c3-2 6-2 9-1v15c-3-1-6-1-9 1V5Z",
    speaker:
      "m4 9 4 0 5-4v14l-5-4H4V9Zm12-1a6 6 0 0 1 0 8m3-11a10 10 0 0 1 0 14",
    trash: "M4 6h16M9 6V3h6v3M6 6l1 15h10l1-15M10 10v7m4-7v7",
    expand: "M8 3H3v5m13-5h5v5M3 16v5h5m13-5v5h-5",
    compact: "M3 8h5V3m8 0v5h5M8 21v-5H3m13 5v-5h5",
    hide: "M5 12h14",
    pin: "m9 3 6 0-1 6 4 4v2H6v-2l4-4-1-6Zm3 12v6",
    down: "M12 4v15m-6-6 6 6 6-6",
    stop: "M6 6h12v12H6Z",
    chevron: "m8 5 7 7-7 7",
    sun: "M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8ZM12 2v2m0 16v2M2 12h2m16 0h2M5 5l1 1m12 12 1 1M5 19l1-1M18 6l1-1",
    moon: "M20 14a8 8 0 0 1-10-10 8 8 0 1 0 10 10Z",

    translate:
      "M4 5h10M9 3v2m-3 0c0 5 3 8 7 10M12 5c0 5-3 8-8 10m9 5 4-11 4 11m-6-4h5",
    service:
      "m12 3 8 4.5v9L12 21l-8-4.5v-9L12 3Zm0 9 8-4.5M12 12 4 7.5M12 12v9",
    history: "M4 10a8 8 0 1 1 1 7M4 4v6h6m2-3v5l3 2",
    display: "M4 4h16v12H4V4Zm4 16h8m-4-4v4",
    plus: "M12 5v14M5 12h14",
    search: "M10.5 17a6.5 6.5 0 1 0 0-13 6.5 6.5 0 0 0 0 13Zm5-1 5 5",
    check: "m5 12 4 4L19 6",
    arrow: "M5 12h14m-5-5 5 5-5 5",
    image: "M4 4h16v16H4V4Zm0 12 5-5 4 4 3-3 4 4M8 8h.01",
    keyboard: "M3 6h18v12H3V6Zm4 4h.01M11 10h.01M15 10h.01M18 10h.01M7 14h10",
    shield: "m12 3 8 3v6c0 5-8 9-8 9s-8-4-8-9V6l8-3Zm-4 9 3 3 5-6",
  };
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.6"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"><path d={paths[name] ?? paths.translate} /></svg
>
