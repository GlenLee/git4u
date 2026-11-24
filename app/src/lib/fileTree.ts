import type { StatusEntry } from "@/ipc/types";

export interface TreeFileNode {
  kind: "file";
  name: string;
  path: string;
  entry: StatusEntry;
  fileCount: number;
  additions: number;
  deletions: number;
}

export interface TreeDirNode {
  kind: "dir";
  name: string;
  path: string;
  children: TreeNode[];
  fileCount: number;
  additions: number;
  deletions: number;
}

export type TreeNode = TreeDirNode | TreeFileNode;

export interface TreeRow {
  node: TreeNode;
  depth: number;
}

function emptyDir(name: string, path: string): TreeDirNode {
  return { kind: "dir", name, path, children: [], fileCount: 0, additions: 0, deletions: 0 };
}

/** 把改动路径按 `/` 折叠成目录树；目录在前，文件在后，均按名称排序。 */
export function buildFileTree(entries: StatusEntry[]): TreeNode[] {
  const root = emptyDir("", "");

  for (const entry of entries) {
    const parts = entry.path.split("/").filter(Boolean);
    if (parts.length === 0) continue;

    let dir = root;
    for (let index = 0; index < parts.length; index++) {
      const part = parts[index];
      const path = parts.slice(0, index + 1).join("/");
      const isFile = index === parts.length - 1;

      if (isFile) {
        dir.children.push({
          kind: "file",
          name: part,
          path,
          entry,
          fileCount: 1,
          additions: entry.additions,
          deletions: entry.deletions,
        });
        continue;
      }

      let child = dir.children.find(
        (node): node is TreeDirNode => node.kind === "dir" && node.name === part,
      );
      if (!child) {
        child = emptyDir(part, path);
        dir.children.push(child);
      }
      dir = child;
    }
  }

  aggregate(root);
  sortTree(root);
  return root.children;
}

function aggregate(dir: TreeDirNode): void {
  for (const child of dir.children) {
    if (child.kind === "dir") aggregate(child);
    dir.fileCount += child.fileCount;
    dir.additions += child.additions;
    dir.deletions += child.deletions;
  }
}

function sortTree(dir: TreeDirNode): void {
  dir.children.sort((a, b) => {
    if (a.kind !== b.kind) return a.kind === "dir" ? -1 : 1;
    return a.name.localeCompare(b.name, "zh-CN");
  });
  for (const child of dir.children) {
    if (child.kind === "dir") sortTree(child);
  }
}

/** 返回目录下所有文件路径（按路径段精确匹配，避免 `src` 命中 `src2`）。 */
export function pathsUnderDirectory(entries: StatusEntry[], dirPath: string): string[] {
  const prefix = `${dirPath}/`;
  return entries.filter((entry) => entry.path.startsWith(prefix)).map((entry) => entry.path);
}

/** 按折叠状态展开为线性行，供 v-for 渲染。 */
export function flattenTree(
  nodes: TreeNode[],
  isCollapsed: (path: string) => boolean,
  depth = 0,
): TreeRow[] {
  const rows: TreeRow[] = [];
  for (const node of nodes) {
    rows.push({ node, depth });
    if (node.kind === "dir" && !isCollapsed(node.path)) {
      rows.push(...flattenTree(node.children, isCollapsed, depth + 1));
    }
  }
  return rows;
}

export interface PathNode {
  kind: "file" | "dir";
  name: string;
  path: string;
  children: PathNode[];
  fileCount: number;
}

export interface PathRow {
  node: PathNode;
  depth: number;
}

/** 工具页用的纯路径树：只关心目录结构与文件数。 */
export function buildPathTree(paths: string[]): PathNode[] {
  const root: PathNode = { kind: "dir", name: "", path: "", children: [], fileCount: 0 };

  for (const path of paths) {
    const parts = path.split("/").filter(Boolean);
    if (parts.length === 0) continue;
    let dir = root;
    for (let index = 0; index < parts.length; index++) {
      const part = parts[index];
      const current = parts.slice(0, index + 1).join("/");
      if (index === parts.length - 1) {
        dir.children.push({
          kind: "file",
          name: part,
          path: current,
          children: [],
          fileCount: 1,
        });
        continue;
      }
      let child = dir.children.find(
        (node): node is PathNode => node.kind === "dir" && node.name === part,
      );
      if (!child) {
        child = { kind: "dir", name: part, path: current, children: [], fileCount: 0 };
        dir.children.push(child);
      }
      dir = child;
    }
  }

  const aggregate = (dir: PathNode): void => {
    dir.fileCount = 0;
    for (const child of dir.children) {
      if (child.kind === "dir") aggregate(child);
      dir.fileCount += child.fileCount;
    }
    dir.children.sort((a, b) => {
      if (a.kind !== b.kind) return a.kind === "dir" ? -1 : 1;
      return a.name.localeCompare(b.name, "zh-CN");
    });
  };
  aggregate(root);
  return root.children;
}

export function flattenPathTree(
  nodes: PathNode[],
  isCollapsed: (path: string) => boolean,
  depth = 0,
): PathRow[] {
  const rows: PathRow[] = [];
  for (const node of nodes) {
    rows.push({ node, depth });
    if (node.kind === "dir" && !isCollapsed(node.path)) {
      rows.push(...flattenPathTree(node.children, isCollapsed, depth + 1));
    }
  }
  return rows;
}
