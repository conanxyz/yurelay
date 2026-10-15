import MarkdownIt from "markdown-it";
import DOMPurify from "dompurify";

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
];
export function readingBlocks(text: string): string[] {
  const clean = DOMPurify.sanitize(md.render(text), {
    ALLOWED_TAGS: tags,
    ALLOWED_ATTR: ["href", "title", "start"],
    ALLOW_DATA_ATTR: false,
  });
  const template = document.createElement("template");
  template.innerHTML = clean;
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
  template.innerHTML = readingBlocks(text).join("");
  function walk(node: Node): string {
    if (!(node instanceof Element)) return node.textContent || "";
    const tag = node.tagName;
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
