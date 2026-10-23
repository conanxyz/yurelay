<script lang="ts">
  import { onMount, type Snippet } from "svelte";
  let {
    label,
    id,
    height = $bindable<number | null>(null),
    children,
    disabled = false,
    onCommit = (_height: number | null) => {},
  }: {
    label: string;
    id: string;
    height?: number | null;
    children: Snippet;
    disabled?: boolean;
    onCommit?: (height: number | null) => void;
  } = $props();
  let box: HTMLDivElement;
  let measured = $state(120);
  let drag: { id: number; y: number; height: number } | null = null;
  const minimum = 80,
    maximum = 4000;
  const clamp = (value: number) =>
    Math.round(Math.min(maximum, Math.max(minimum, value)));
  function start(event: PointerEvent) {
    if (disabled || event.button !== 0 || drag) return;
    event.preventDefault();
    const handle = event.currentTarget as HTMLElement;
    handle.focus({ preventScroll: true });
    handle.setPointerCapture(event.pointerId);
    drag = {
      id: event.pointerId,
      y: event.clientY,
      height: box.getBoundingClientRect().height,
    };
  }
  function move(event: PointerEvent) {
    if (drag?.id !== event.pointerId) return;
    height = clamp(drag.height + event.clientY - drag.y);
  }
  function end(event: PointerEvent) {
    if (drag?.id !== event.pointerId) return;
    drag = null;
    onCommit(height);
    const handle = event.currentTarget as HTMLElement;
    if (handle.hasPointerCapture(event.pointerId))
      handle.releasePointerCapture(event.pointerId);
  }
  function key(event: KeyboardEvent) {
    if (
      disabled ||
      event.ctrlKey ||
      event.altKey ||
      event.metaKey ||
      event.isComposing
    )
      return;
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      event.stopPropagation();
      height = clamp(
        measured +
          (event.key === "ArrowDown" ? 24 : -24) * (event.shiftKey ? 4 : 1),
      );
      onCommit(height);
    } else if (event.key === "Home") {
      event.preventDefault();
      event.stopPropagation();
      reset();
    }
  }
  function reset() {
    if (!disabled) {
      height = null;
      onCommit(null);
    }
  }
  onMount(() => {
    const observer = new ResizeObserver(() => {
      measured = Math.round(box.getBoundingClientRect().height);
    });
    observer.observe(box);
    return () => observer.disconnect();
  });
</script>

<div
  {id}
  class="reader-resize-box"
  class:has-height={height !== null}
  style:height={height === null ? undefined : `${height}px`}
  bind:this={box!}
>
  {@render children()}
</div>
<!-- WAI-ARIA window-splitter semantics require a focusable separator with
     value and keyboard interaction: https://www.w3.org/WAI/ARIA/apg/patterns/windowsplitter/ -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions (ARIA separators are interactive when focusable and expose a size value.) -->
<div
  class="reader-resize-handle"
  role="separator"
  aria-label={label}
  title={label}
  aria-orientation="horizontal"
  aria-controls={id}
  aria-valuemin={minimum}
  aria-valuemax={Math.max(maximum, measured)}
  aria-valuenow={measured}
  aria-disabled={disabled}
  tabindex={disabled ? -1 : 0}
  onpointerdown={start}
  onpointermove={move}
  onpointerup={end}
  onpointercancel={end}
  onlostpointercapture={end}
  onkeydown={key}
  ondblclick={reset}
>
  <span></span>
</div>
