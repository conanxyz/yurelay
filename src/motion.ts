// Native Web Animations keep Glossa free of restricted animation dependencies.
// Content is visible by default; animations never gate rendering or stream updates.
function entrance(
  node: HTMLElement,
  targets: Element[],
  from: Keyframe,
  duration: number,
  scroller?: Element | null,
  stagger = 0,
) {
  const preference = window.matchMedia('(prefers-reduced-motion: reduce)');
  let animations: Animation[] = [];
  let observer: IntersectionObserver | undefined;
  const cancel = () => {
    observer?.disconnect();
    animations.forEach(animation => animation.cancel());
    animations = [];
  };
  const play = () => {
    observer?.disconnect();
    if (preference.matches) return;
    animations = targets.map((target, index) => target.animate(
      [from, { transform: 'none', opacity: 1 }],
      { duration, delay: index * stagger, easing: 'cubic-bezier(.32,.72,0,1)' },
    ));
  };
  if (!preference.matches) {
    if (scroller) {
      observer = new IntersectionObserver(entries => {
        if (entries.some(entry => entry.isIntersecting)) play();
      }, { root: scroller });
      observer.observe(node);
    } else play();
  }
  // Respect a preference change even while an animation is running.
  preference.addEventListener('change', cancel);
  return { destroy() { cancel(); preference.removeEventListener('change', cancel); } };
}
export function panelEntrance(node: HTMLElement) {
  const preferences = Array.from(node.querySelectorAll('.preference-core'));
  const groups = preferences.length ? preferences : Array.from(node.querySelectorAll('.form-section,.settings-card'));
  return entrance(node, groups.length ? groups : [node], { transform: 'translateY(6px)', opacity: .65 }, preferences.length ? 800 : 200, null, 35);
}
export function imageEntrance(node: HTMLElement) {
  return entrance(node, [node], { transform: 'scale(.985)', opacity: .7 }, 200, node.closest('.history-detail'));
}
export function historyEntrance(node: HTMLElement) {
  return entrance(node, [node], { transform: 'translateY(10px)', opacity: .6 }, 800, node.closest('.settings-content'));
}
