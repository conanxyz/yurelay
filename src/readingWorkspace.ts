/** Measure the viewport space reserved for the paired reader. Assistant drawers
 * extend the document below it; opening one must not shrink the reading area. */
export function fitReadingWorkspace(
  node: HTMLElement,
  onMeasure: (height: number) => void,
) {
  const content = node.parentElement!;
  let frame = 0;
  let previous = 0;
  const extent = (element: Element | null) => {
    if (!element) return 0;
    const box = element.getBoundingClientRect();
    if (!box.height) return 0;
    const css = getComputedStyle(element);
    return (
      box.height + parseFloat(css.marginTop) + parseFloat(css.marginBottom)
    );
  };
  function measure() {
    frame = 0;
    if (node.hidden || innerWidth < 1100) return;
    const css = getComputedStyle(node);
    const footer = [...node.children]
      .filter((element) =>
        element.matches(
          ".reading-output-actions,.service-strip,.assistant-entrypoints,.service-setup-hint,.dictionary-guidance",
        ),
      )
      .reduce((height, element) => height + extent(element), 0);
    const available = Math.max(
      240,
      Math.floor(
        innerHeight -
          (node.getBoundingClientRect().top + scrollY) -
          footer -
          parseFloat(css.paddingTop) -
          parseFloat(css.paddingBottom) -
          parseFloat(getComputedStyle(content).paddingBottom) -
          extent(content.querySelector(":scope > .capture-hint")),
      ),
    );
    if (previous !== available) {
      previous = available;
      onMeasure(available);
    }
  }
  function schedule() {
    if (!frame) frame = requestAnimationFrame(measure);
  }
  const resize = new ResizeObserver(schedule);
  resize.observe(content);
  resize.observe(node);
  const mutation = new MutationObserver(schedule);
  mutation.observe(content, { childList: true });
  mutation.observe(node, {
    childList: true,
    attributes: true,
    attributeFilter: ["hidden"],
  });
  window.addEventListener("resize", schedule);
  schedule();
  return {
    destroy() {
      resize.disconnect();
      mutation.disconnect();
      window.removeEventListener("resize", schedule);
      if (frame) cancelAnimationFrame(frame);
    },
  };
}
