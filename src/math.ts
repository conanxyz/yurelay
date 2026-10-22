import type MarkdownIt from "markdown-it";
import type { KatexOptions } from "katex";
import DOMPurify from "dompurify";
let renderer: typeof import("katex") | undefined;
let loading: Promise<void> | undefined;
const options: KatexOptions = {
  trust: false,
  strict: "error",
  throwOnError: true,
  maxExpand: 100,
  maxSize: 10,
  output: "htmlAndMathml",
};
export function ensureMath(text: string): Promise<void> {
  if (!text.includes("$") || renderer) return Promise.resolve();
  return (loading ??= Promise.all([
    import("katex"),
    import("katex/dist/katex.min.css"),
  ])
    .then(([module]) => {
      renderer = module;
    })
    .catch(() => {
      loading = undefined;
    }));
}
type Formula = { source: string; raw: string; display: boolean };
export function installMath(md: ReturnType<typeof MarkdownIt>) {
  md.inline.ruler.after("escape", "local_math", (state, silent) => {
    const pos = state.pos;
    if (state.src[pos] !== "$") return false;
    const display = state.src[pos + 1] === "$";
    const marker = display ? "$$" : "$";
    const begin = pos + marker.length;
    if (!display && /\s/.test(state.src[begin] ?? "")) return false;
    let end = state.src.indexOf(marker, begin);
    while (end >= 0 && state.src[end - 1] === "\\")
      end = state.src.indexOf(marker, end + marker.length);
    if (
      end < 0 ||
      end === begin ||
      end - begin > 4000 ||
      (!display && /\s/.test(state.src[end - 1]))
    )
      return false;
    const source = state.src.slice(begin, end);
    if (source.includes("\n") && !display) return false;
    // A digit-leading amount needs mathematical syntax; plain prices stay text.
    if (
      !display &&
      /^\d/.test(source) &&
      !/[+*=^_{}\\]|\d[a-zA-Z]/.test(source)
    )
      return false;
    if (!silent) {
      const token = state.push("local_math", "", 0);
      token.content = source;
      token.meta = {
        source,
        raw: state.src.slice(pos, end + marker.length),
        display,
      };
    }
    state.pos = end + marker.length;
    return true;
  });
  md.block.ruler.before(
    "fence",
    "local_math_block",
    (state, start, end, silent) => {
      const first = state.src.slice(
        state.bMarks[start] + state.tShift[start],
        state.eMarks[start],
      );
      if (!first.startsWith("$$")) return false;
      let last = start,
        raw = first;
      if (first.length < 4 || !first.slice(2).includes("$$")) {
        for (last = start + 1; last < end; last++) {
          raw +=
            "\n" +
            state.src.slice(
              state.bMarks[last] + state.tShift[last],
              state.eMarks[last],
            );
          if (raw.length > 4004) return false;
          if (raw.endsWith("$$")) break;
        }
      }
      if (last >= end || !raw.endsWith("$$") || raw.length > 4004) return false;
      if (silent) return true;
      const token = state.push("local_math", "", 0);
      token.block = true;
      token.content = raw.slice(2, -2);
      token.meta = { source: token.content, raw, display: true };
      state.line = last + 1;
      return true;
    },
  );
  md.renderer.rules.local_math = (tokens, index, _options, env) => {
    const formula = tokens[index].meta as Formula;
    if (!env) return md.utils.escapeHtml(formula.raw);
    const formulas = (env.formulas ??= []) as Formula[];
    if (formulas.length >= 32) return md.utils.escapeHtml(formula.raw);
    const id = formulas.length;
    formulas.push(formula);
    return `<span data-local-math="${id}">${md.utils.escapeHtml(formula.raw)}</span>`;
  };
}
export function enhanceMath(
  root: DocumentFragment,
  formulas: Formula[],
  enhance = true,
) {
  for (const node of root.querySelectorAll("[data-local-math]")) {
    const formula = formulas[Number(node.getAttribute("data-local-math"))];
    node.removeAttribute("data-local-math");
    if (!formula) continue;
    node.setAttribute("data-math-source", formula.raw);
    node.classList.add("math-source");
    if (
      !enhance ||
      !renderer ||
      /\\(?:href|url|includegraphics|htmlClass|htmlId|htmlStyle|htmlData)\b/.test(
        formula.source,
      )
    )
      continue;
    try {
      const rendered = renderer.renderToString(formula.source, {
        ...options,
        macros: {},
        displayMode: formula.display,
      });
      // This allowlist applies only to KaTeX output, never to model-supplied HTML.
      node.innerHTML = DOMPurify.sanitize(rendered, {
        USE_PROFILES: { html: true, mathMl: true, svg: true },
        FORBID_TAGS: ["a", "img", "script", "iframe", "object", "style"],
        FORBID_ATTR: ["href", "src", "xlink:href"],
        ALLOW_DATA_ATTR: false,
      });
      node.classList.remove("math-source");
    } catch {
      node.textContent = formula.raw;
    }
  }
}
