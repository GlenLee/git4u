import { afterEach, describe, expect, it } from "vitest";
import { GITIGNORE_TEMPLATES } from "./gitignoreTemplates";
import { language, messageKeys, missingKeys, setLanguage, t, type MessageKey } from "./i18n";

afterEach(() => setLanguage("zh"));

describe("i18n", () => {
  it("keeps both dictionaries in sync", () => {
    expect(missingKeys("zh")).toEqual([]);
    expect(missingKeys("en")).toEqual([]);
    expect(messageKeys().length).toBeGreaterThan(100);
  });

  it("switches language and interpolates params", () => {
    expect(t("workspace.commitDone", { oid: "abc1234" })).toBe("已提交 abc1234");
    setLanguage("en");
    expect(t("workspace.commitDone", { oid: "abc1234" })).toBe("Committed abc1234");
    expect(t("workspace.stagedCount", { n: 1 })).toBe("1 file staged");
    expect(t("workspace.stagedCount", { n: 3 })).toBe("3 files staged");
    expect(language.value).toBe("en");
  });

  it("resolves every gitignore template hint", () => {
    for (const template of GITIGNORE_TEMPLATES) {
      expect(t(template.hint as MessageKey)).not.toBe(template.hint);
    }
  });
});
