import { describe, it, expect } from "vitest";
import MarkdownIt from "markdown-it";
import { installMath } from "../src/math";
const md = new MarkdownIt({ html: false });
installMath(md);
const parse = (text: string) => {
  const env: { formulas: { source: string; raw: string; display: boolean }[] } =
    { formulas: [] };
  return { html: md.render(text, env), formulas: env.formulas };
};
describe("bounded math recognition", () => {
  it("recognizes complete inline and multiline display formulas", () => {
    const result = parse("Result $x^2 + 1$.\n\n$$\n\\frac{a}{b}\n$$");
    expect(result.formulas).toHaveLength(2);
    expect(result.formulas[0].raw).toBe("$x^2 + 1$");
    expect(result.formulas[1].display).toBe(true);
  });
  it("leaves prices, code, escapes and incomplete streamed formulas unchanged", () => {
    const result = parse(
      "Price $12.50 and $8.00. `x $a$` \\$x\\$ and $unfinished\n\n```tex\n$$x$$\n```",
    );
    expect(result.formulas).toEqual([]);
    expect(result.html).toContain("$unfinished");
    expect(result.html).toContain("$12.50");
  });
  it("enforces per-formula and per-document budgets without dropping source", () => {
    const many = parse(Array.from({ length: 40 }, () => "$x$").join(" "));
    expect(many.formulas).toHaveLength(32);
    expect(many.html).toContain("$x$");
    const long = parse("$" + "x".repeat(4001) + "$");
    expect(long.formulas).toEqual([]);
    expect(long.html).toContain("x".repeat(4001));
  });
  it("accepts numeric math without treating currency amounts as formulas", () => {
    expect(parse("$2x$ and $2+2$ and $2\\pi$").formulas).toHaveLength(3);
    expect(parse("Costs $20 and $30. Also $20$.").formulas).toEqual([]);
  });
  it("retains untrusted formula commands as inert parser content", () => {
    const result = parse("$\\href{javascript:alert(1)}{x}$");
    expect(result.html).not.toContain("<a");
    expect(result.formulas[0].source).toBe("\\href{javascript:alert(1)}{x}");
  });
});
