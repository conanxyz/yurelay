<script lang="ts">
  import { onMount, type Snippet } from "svelte";
  let {
    label,
    id,
    height = $bindable<number | null>(null),
    viewportHeight = $bindable<number | null>(null),
    linkedHeight = null,
    linkedMaximum = 4000,
    onLinkedResize = (_height: number | null, _commit: boolean) => {},
    children,
    disabled = false,
    onCommit = (_height: number | null, _viewportHeight: number | null) => {},
  }: {
    label: string;
    id: string;
    height?: number | null;
    viewportHeight?: number | null;
    linkedHeight?: number | null;
    linkedMaximum?: number;
    onLinkedResize?: (height: number | null, commit: boolean) => void;
    children: Snippet;
    disabled?: boolean;
    onCommit?: (height: number | null, viewportHeight: number | null) => void;
  } = $props();
  const minimum = 80,
    maximum = 4000;
  const clamp = (value: number) =>
    Math.round(Math.min(maximum, Math.max(minimum, value)));
  let windowHeight = $state(560);
  // Use the original default window height as the reference for legacy pixel preferences.
  const referenceHeight = $derived(Math.max(240, windowHeight));
  const displayedHeight = $derived(
    height === null
      ? null
      : clamp((height * referenceHeight) / (viewportHeight ?? 560)),
  );
  function setHeight(value: number, commit = false) {
    if (linkedHeight !== null) {
      onLinkedResize(value, commit);
      return;
    }
    height = clamp(value);
    viewportHeight = referenceHeight;
    if (commit) onCommit(height, viewportHeight);
  }
  let box: HTMLDivElement;
  let measured = $state(120);
  let drag: { id: number; y: number; height: number; latest?: number } | null =
    null;
  function start(event: PointerEvent) {
    if (disabled || event.button !== 0 || drag) return;
    event.preventDefault();
    const handle = event.currentTarget as HTMLElement;
    handle.focus({ preventScroll: true });
    handle.setPointerCapture(event.pointerId);
    drag = {
      id: event.pointerId,
      y: event.clientY,
      height: linkedHeight ?? box.getBoundingClientRect().height,
    };
  }
  function move(event: PointerEvent) {
    if (drag?.id !== event.pointerId) return;
    drag.latest = drag.height + event.clientY - drag.y;
    setHeight(drag.latest);
  }
  function end(event: PointerEvent) {
    if (drag?.id !== event.pointerId) return;
    const finalHeight = drag.latest ?? linkedHeight;
    drag = null;
    if (linkedHeight !== null) onLinkedResize(finalHeight, true);
    else onCommit(height, viewportHeight);
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
      setHeight(
        (linkedHeight ?? box.getBoundingClientRect().height) +
          (event.key === "ArrowDown" ? 24 : -24) * (event.shiftKey ? 4 : 1),
        true,
      );
    } else if (event.key === "Home") {
      event.preventDefault();
      event.stopPropagation();
      reset();
    }
  }
  function reset() {
    if (!disabled) {
      if (linkedHeight !== null) {
        onLinkedResize(null, true);
        return;
      }
      height = null;
      viewportHeight = null;
      onCommit(null, null);
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

<svelte:window bind:innerHeight={windowHeight} />

<div
  {id}
  class="reader-resize-box"
  class:has-height={height !== null}
  style:height={linkedHeight !== null || displayedHeight === null
    ? undefined
    : `${displayedHeight}px`}
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
  aria-valuemin={linkedHeight !== null ? 240 : minimum}
  aria-valuemax={linkedHeight !== null
    ? Math.max(linkedMaximum, linkedHeight)
    : Math.max(maximum, measured)}
  aria-valuenow={linkedHeight ?? measured}
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
