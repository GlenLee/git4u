import { afterEach, describe, expect, it } from "vitest";
import { setLanguage } from "./i18n";
import { describeError, midStateGuide } from "./problems";

afterEach(() => setLanguage("zh"));

describe("problems", () => {
  it("keeps the structured error details", () => {
    const description = describeError({
      kind: "authFailed",
      human: "远端认证失败",
      causeChain: ["fatal: Authentication failed"],
      suggestions: [
        { label: "更新凭证", risk: "safe", action: "credentials.update", target: "c123" },
      ],
      cliEquivalent: "git fetch",
    });
    expect(description.kind).toBe("authFailed");
    expect(description.title).toBe("远端认证失败");
    expect(description.causes).toEqual(["fatal: Authentication failed"]);
    expect(description.cli).toBe("git fetch");
    expect(description.suggestions).toHaveLength(1);
    expect(description.suggestions[0].target).toBe("c123");
  });

  it("falls back to the raw message", () => {
    expect(describeError(new Error("boom")).title).toBe("boom");
    expect(describeError("plain").title).toBe("plain");
    expect(describeError(null).causes).toEqual([]);
  });

  it("maps mid-states to copyable commands and a translated label", () => {
    const rebase = midStateGuide("rebase 中");
    expect(rebase.continueCmd).toBe("git rebase --continue");
    expect(rebase.abortCmd).toBe("git rebase --abort");

    setLanguage("en");
    expect(midStateGuide("rebase 中").label).toBe("Rebase in progress");
    expect(midStateGuide("unknown").statusCmd).toBe("git status");
  });
});
