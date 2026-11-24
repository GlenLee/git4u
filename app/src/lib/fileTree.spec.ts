import { describe, expect, it } from "vitest";
import { buildFileTree, buildPathTree, flattenPathTree, flattenTree, pathsUnderDirectory, type PathNode, type TreeDirNode } from "./fileTree";
import type { StatusEntry } from "@/ipc/types";

function entry(path: string, additions = 1, deletions = 0): StatusEntry {
  return {
    path,
    origPath: null,
    indexState: "unmodified",
    worktreeState: "modified",
    staged: false,
    additions,
    deletions,
  };
}

const entries = [
  entry("src/sso/provider.rs", 3, 1),
  entry("src/sso/token.rs", 2, 0),
  entry("src/login.rs", 4, 2),
  entry("docs/deploy.md", 1, 1),
  entry("README.md", 1, 0),
];

describe("buildFileTree", () => {
  it("groups files by directory and sorts directories first", () => {
    const tree = buildFileTree(entries);
    expect(tree.map((node) => node.name)).toEqual(["docs", "src", "README.md"]);

    const src = tree[1] as TreeDirNode;
    expect(src.fileCount).toBe(3);
    expect(src.additions).toBe(9);
    expect(src.deletions).toBe(3);
    expect(src.children.map((node) => node.name)).toEqual(["sso", "login.rs"]);

    const sso = src.children[0] as TreeDirNode;
    expect(sso.fileCount).toBe(2);
    expect(sso.children.map((node) => node.name)).toEqual(["provider.rs", "token.rs"]);
  });

  it("flattens expanded nodes and hides collapsed descendants", () => {
    const tree = buildFileTree(entries);
    const expanded = flattenTree(tree, () => false);
    expect(expanded.map((row) => row.node.name)).toEqual([
      "docs",
      "deploy.md",
      "src",
      "sso",
      "provider.rs",
      "token.rs",
      "login.rs",
      "README.md",
    ]);

    const collapsed = flattenTree(tree, (path) => path === "src" || path === "src/sso");
    expect(collapsed.map((row) => row.node.name)).toEqual([
      "docs",
      "deploy.md",
      "src",
      "README.md",
    ]);
    expect(collapsed.find((row) => row.node.name === "src")?.depth).toBe(0);
    expect(collapsed.find((row) => row.node.name === "deploy.md")?.depth).toBe(1);
  });

  it("handles root-level files without a directory", () => {
    const tree = buildFileTree([entry("a.txt"), entry("b.txt")]);
    expect(tree.every((node) => node.kind === "file")).toBe(true);
    expect(flattenTree(tree, () => false)).toHaveLength(2);
  });

  it("collects files under a directory without matching prefix siblings", () => {
    const withSibling = [...entries, entry("src2/other.rs", 1, 0)];
    expect(pathsUnderDirectory(withSibling, "src")).toEqual([
      "src/sso/provider.rs",
      "src/sso/token.rs",
      "src/login.rs",
    ]);
    expect(pathsUnderDirectory(withSibling, "src/sso")).toEqual([
      "src/sso/provider.rs",
      "src/sso/token.rs",
    ]);
    expect(pathsUnderDirectory(withSibling, "src2")).toEqual(["src2/other.rs"]);
  });
});

describe("buildPathTree", () => {
  it("builds a directory tree from plain paths", () => {
    const tree = buildPathTree(["src/a.ts", "src/nested/b.ts", "README.md"]);
    expect(tree.map((node) => node.name)).toEqual(["src", "README.md"]);
    const src = tree[0] as PathNode;
    expect(src.fileCount).toBe(2);
    expect(src.children.map((node) => node.name)).toEqual(["nested", "a.ts"]);
    expect(flattenPathTree(tree, () => false).map((row) => row.node.path)).toEqual([
      "src",
      "src/nested",
      "src/nested/b.ts",
      "src/a.ts",
      "README.md",
    ]);
  });
});
