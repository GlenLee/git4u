import { describe, expect, it } from "vitest";
import { GITIGNORE_TEMPLATES, gitignoreBlock } from "./gitignoreTemplates";

describe("gitignore templates", () => {
  it("uses unique ids and non-empty content", () => {
    const ids = new Set(GITIGNORE_TEMPLATES.map((template) => template.id));
    expect(ids.size).toBe(GITIGNORE_TEMPLATES.length);
    for (const template of GITIGNORE_TEMPLATES) {
      expect(template.content.trim().length).toBeGreaterThan(0);
    }
  });

  it("renders one titled block ending with a newline", () => {
    const block = gitignoreBlock(GITIGNORE_TEMPLATES[0]);
    expect(block.startsWith("# Node.js\n")).toBe(true);
    expect(block.endsWith("\n")).toBe(true);
  });
});
