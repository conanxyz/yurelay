import MarkdownIt from "markdown-it";
import DOMPurify from "dompurify";
import { installMath, enhanceMath } from "./math";

export function externalLink(value: string): string | null {
  try {
    const url = new URL(value);
    return ["http:", "https:"].includes(url.protocol) &&
      !!url.hostname &&
      !url.username &&
      !url.password &&
      value.length <= 8192
      ? url.href
      : null;
  } catch {
    return null;
  }
}
const md = new MarkdownIt({
  html: false,
  breaks: true,
  linkify: true,
  typographer: false,
});
installMath(md);
// Indentation alone is common in translated prose. Keep its literal whitespace,
// but do not force it into a non-wrapping code panel. Fenced code is unchanged.
md.renderer.rules.code_block = (tokens, index) =>
  `<pre data-indented-code="true">${md.utils.escapeHtml(tokens[index].content)}</pre>\n`;
md.validateLink = (value) => externalLink(value) !== null;
md.renderer.rules.image = (tokens, index) =>
  md.utils.escapeHtml(tokens[index].content || "");
const tags = [
  "p",
  "br",
  "strong",
  "em",
  "s",
  "blockquote",
  "ul",
  "ol",
  "li",
  "h1",
  "h2",
  "h3",
  "h4",
  "h5",
  "h6",
  "pre",
  "code",
  "a",
  "hr",
  "table",
  "thead",
  "tbody",
  "tr",
  "th",
  "td",
  "span",
];
export function readingBlocks(text: string, enhance = true): string[] {
  const env = { formulas: [] };
  const clean = DOMPurify.sanitize(md.render(text, env), {
    ALLOWED_TAGS: tags,
    ALLOWED_ATTR: ["href", "title", "start", "data-local-math", "data-indented-code"],
    ALLOW_DATA_ATTR: false,
  });
  const template = document.createElement("template");
  template.innerHTML = clean;
  enhanceMath(template.content, env.formulas, enhance);
  return Array.from(template.content.childNodes)
    .filter((n) => n.nodeType !== Node.TEXT_NODE || n.textContent?.trim())
    .map((n) =>
      n instanceof Element
        ? n.outerHTML
        : md.utils.escapeHtml(n.textContent || ""),
    );
}
export function translationPlainText(text: string): string {
  const template = document.createElement("template");
  template.innerHTML = readingBlocks(text, false).join("");
  function walk(node: Node): string {
    if (!(node instanceof Element)) return node.textContent || "";
    const tag = node.tagName;
    if (node.hasAttribute("data-math-source"))
      return node.getAttribute("data-math-source") || "";
    if (tag === "BR") return "\n";
    if (tag === "PRE") return (node.textContent || "") + "\n\n";
    if (tag === "HR") return "\n";
    const content = Array.from(node.childNodes)
      .map((child) => {
        // Markdown renders a soft break as <br> followed by a formatting newline.
        // The BR already contributes the clipboard newline.
        if (
          child.nodeType === Node.TEXT_NODE &&
          child.previousSibling?.nodeName === "BR"
        )
          return (child.textContent || "").replace(/^\n/, "");
        return walk(child);
      })
      .join("");
    if (tag === "LI") {
      const parent = node.parentElement;
      const marker =
        parent?.tagName === "OL"
          ? `${Number(parent.getAttribute("start") || 1) + Array.from(parent.children).indexOf(node)}.`
          : "•";
      return `${marker} ${content.trim()}\n`;
    }
    if (tag === "TH" || tag === "TD") return content + "\t";
    if (tag === "TR") return content.trimEnd() + "\n";
    if (/^(P|H[1-6]|BLOCKQUOTE|UL|OL|TABLE)$/.test(tag))
      return content + "\n\n";
    return content;
  }
  return Array.from(template.content.childNodes)
    .map(walk)
    .join("")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}

export type TextHighlight = { start: number; end: number };
export function highlightedText(text: string, ranges: TextHighlight[]) {
  const valid = ranges
    .filter(
      (range) =>
        Number.isInteger(range.start) &&
        Number.isInteger(range.end) &&
        range.start >= 0 &&
        range.end > range.start &&
        range.end <= text.length,
    )
    .sort((a, b) => a.start - b.start);
  const parts: { text: string; highlighted: boolean }[] = [];
  let end = 0;
  for (const range of valid) {
    if (range.start < end) continue;
    if (range.start > end)
      parts.push({ text: text.slice(end, range.start), highlighted: false });
    parts.push({ text: text.slice(range.start, range.end), highlighted: true });
    end = range.end;
  }
  if (end < text.length)
    parts.push({ text: text.slice(end), highlighted: false });
  return parts;
}

// Map trusted raw-text highlights onto sanitized Markdown text nodes. No model
// HTML is added, and emphasis/links remain intact even when a match spans nodes.
export function highlightedReading(
  blocks: string[],
  raw: string,
  ranges: TextHighlight[],
): string[] {
  if (!ranges.length || !blocks.length) return blocks;
  const roots = blocks.map((block) => {
    const element = document.createElement("div");
    element.innerHTML = block;
    return element;
  });
  const nodes: { node: Text; start: number; end: number }[] = [];
  let visible = "";
  for (const root of roots) {
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    let node: Node | null;
    while ((node = walker.nextNode())) {
      const start = visible.length;
      visible += node.textContent ?? "";
      nodes.push({ node: node as Text, start, end: visible.length });
    }
  }
  const matches: TextHighlight[] = [];
  for (const part of highlightedText(raw, ranges).filter(
    (part) => part.highlighted,
  )) {
    const start = visible.indexOf(part.text);
    if (start >= 0 && start === visible.lastIndexOf(part.text))
      matches.push({ start, end: start + part.text.length });
  }
  for (const entry of nodes) {
    const local = matches
      .filter((range) => range.start < entry.end && range.end > entry.start)
      .map((range) => ({
        start: Math.max(0, range.start - entry.start),
        end: Math.min(entry.end, range.end) - entry.start,
      }));
    if (!local.length) continue;
    const fragment = document.createDocumentFragment();
    for (const part of highlightedText(entry.node.data, local)) {
      if (part.highlighted) {
        const mark = document.createElement("mark");
        mark.className = "translation-word-highlight";
        mark.textContent = part.text;
        fragment.append(mark);
      } else fragment.append(document.createTextNode(part.text));
    }
    entry.node.replaceWith(fragment);
  }
  return roots.map((root) => root.innerHTML);
}

export function readingTableText(block: string): string {
  const template = document.createElement("template");
  template.innerHTML = block;
  for (const node of template.content.querySelectorAll("[data-math-source]"))
    node.textContent = node.getAttribute("data-math-source") || "";
  return Array.from(template.content.querySelectorAll("tr"))
    .map((row) =>
      Array.from(row.querySelectorAll("th,td"))
        .map((cell) => cell.textContent?.trim() || "")
        .join("\t"),
    )
    .join("\n");
}
