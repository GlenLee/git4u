// 浏览器设计预览用的内存后端。Tauri 运行时永远不会加载这份数据。
import type {
  AppInfo,
  BackupRef,
  BranchInfo,
  BranchListing,
  CommitDetail,
  CommitSummary,
  CredentialEntry,
  CredentialInput,
  CredentialValidation,
  EnvReport,
  EolReport,
  FileDiff,
  FilePreview,
  GitignoreFile,
  IgnoreMatch,
  LargeFile,
  LockReport,
  ObjectStats,
  ProxyEntry,
  ProxyInput,
  PushAuditReport,
  ReflogEntry,
  RemoteInfo,
  RepoSettings,
  RepoStatus,
  RepoSummary,
  SecretFinding,
  SettingsScope,
  SettingsView,
  StatusEntry,
  TagInfo,
} from "./types";

const oid = (seed: string) => (seed + "0".repeat(40)).slice(0, 40);
const now = Math.floor(Date.now() / 1000);
const c = (seed: string) => oid(seed);

const head = {
  kind: "branch" as const,
  branch: "feature/sso",
  oid: c("a1b2c3f4"),
};

const baseEntries: StatusEntry[] = [
  entry("src/login.rs", "modified", "unmodified", true, 12, 3),
  entry("src/sso.rs", "added", "unmodified", true, 26, 0),
  entry("docs/deploy.md", "unmodified", "modified", false, 2, 2),
  entry("src/token.rs", "unmodified", "modified", false, 4, 5),
  entry("Cargo.toml", "unmodified", "modified", false, 1, 1),
  entry("src/login_test.rs", "unmodified", "untracked", false, 0, 0),
  entry("assets/logo.svg", "unmodified", "untracked", false, 0, 0),
];

function entry(
  path: string,
  indexState: StatusEntry["indexState"],
  worktreeState: StatusEntry["worktreeState"],
  staged: boolean,
  additions: number,
  deletions: number,
): StatusEntry {
  return { path, origPath: null, indexState, worktreeState, staged, additions, deletions };
}

let entries = baseEntries.map((e) => ({ ...e }));
let lastCommit: {
  commits: CommitSummary[];
  entries: StatusEntry[];
  headOid: string;
} | null = null;
const gitignoreByRepo = new Map<string, string>();
const defaultSettings: RepoSettings = {
  pullStrategy: "merge",
  protectedBranches: "^(main|master|develop|release/.*)$",
  pushAudit: true,
  secretScan: true,
  proxyId: null,
  autoFetch: false,
  autoFetchMinutes: 5,
  backupRetentionDays: 30,
};
let globalSettings: RepoSettings = { ...defaultSettings };
const repoSettings = new Map<string, RepoSettings>();
let proxies: ProxyEntry[] = [
  { id: "p-demo-1", name: "公司 HTTP 代理", url: "http://proxy.company.com:8080" },
  { id: "p-demo-2", name: "本地 SOCKS5", url: "socks5://127.0.0.1:1080" },
];
let credentials: CredentialEntry[] = [
  {
    id: "c-demo-1",
    url: "https://github.com/git4u",
    mode: "password",
    username: "zhangsan",
    lastUsed: Math.floor(Date.now() / 1000) - 120,
    status: "ok",
  },
  {
    id: "c-demo-2",
    url: "https://gitlab.company.com",
    mode: "token",
    username: "ci-bot",
    lastUsed: Math.floor(Date.now() / 1000) - 3 * 86400,
    status: "unverified",
  },
];

let commits: CommitSummary[] = [
  commit("a1b2c3f4", ["9f8e7d10"], "feat: 支持 SSO 单点登录", "张三", now - 180, ["HEAD -> feature/sso"]),
  commit("9f8e7d10", ["d4e5f620"], "fix: token 过期自动刷新", "张三", now - 3600, []),
  commit("d4e5f620", ["c3d4e550"], "chore: 升级依赖 2026.11", "李四", now - 7200, ["main"]),
  commit("c3d4e550", ["77aa8890", "e9f0a140"], "merge: release/2.3 → main", "李四", now - 26 * 3600, []),
  commit("77aa8890", ["b7c8d930"], "release: 2.3.0", "王五", now - 27 * 3600, ["tag: v2.3.0", "release/2.3"]),
  commit("e9f0a140", ["f1a2b360"], "feat: 新增 CSV 导出", "王五", now - 30 * 3600, ["origin/feature/api"]),
  commit("b7c8d930", ["f1a2b360"], "fix: 导出大文件内存占用", "王五", now - 2 * 86400, []),
  commit("f1a2b360", ["3d4e5f70"], "docs: 更新部署手册", "赵六", now - 3 * 86400, []),
  commit("3d4e5f70", ["5f6a7b30"], "refactor: 抽取配置加载", "赵六", now - 5 * 86400, []),
  commit("5f6a7b30", [], "init: 项目初始化", "张三", now - 21 * 86400, []),
];

function commit(
  seed: string,
  parents: string[],
  summary: string,
  author: string,
  committedAt: number,
  refs: string[],
): CommitSummary {
  return {
    oid: c(seed),
    shortOid: seed.slice(0, 7),
    parents: parents.map(c),
    summary,
    authorName: author,
    authorEmail: `${author}@example.com`,
    committedAt,
    refs,
  };
}

let branches: BranchInfo[] = [
  branch("feature/sso", "a1b2c3f4", true, false, "origin/feature/sso", "[ahead 2]", 2, 0),
  branch("main", "d4e5f620", false, false, "origin/main", "[behind 1]", 0, 1),
  branch("develop", "b7c8d930", false, false, "origin/develop", "[behind 3]", 0, 3),
  branch("release/2.3", "77aa8890", false, false, "origin/release/2.3", "", 0, 0),
  branch("origin/main", "c3d4e550", false, true),
  branch("origin/develop", "b7c8d930", false, true),
  branch("origin/feature/api", "e9f0a140", false, true),
  branch("origin/feature/sso", "a1b2c3f4", false, true),
  branch("origin/release/2.3", "77aa8890", false, true),
];

function branch(
  name: string,
  seed: string,
  isCurrent: boolean,
  isRemote: boolean,
  upstream: string | null = null,
  track = "",
  ahead = 0,
  behind = 0,
): BranchInfo {
  return {
    name,
    fullName: (isRemote ? "refs/remotes/" : "refs/heads/") + name,
    oid: c(seed),
    isCurrent,
    isRemote,
    remote: isRemote ? name.split("/")[0] : null,
    upstream,
    track: track || null,
    ahead,
    behind,
    gone: false,
    updatedAt: now - 600,
  };
}

const tags: TagInfo[] = [
  { name: "v2.3.0", oid: c("77aa8890"), subject: "release: 2.3.0" },
  { name: "v2.2.1", oid: c("b7c8d930"), subject: "fix: 导出大文件内存占用" },
];

function currentStatus(): RepoStatus {
  return {
    version: 1,
    head: { ...head },
    upstream: { name: "origin/feature/sso", ahead: 2, behind: 0, gone: false },
    entries: entries.map((e) => ({ ...e })),
    conflicts: [],
    midState: null,
  };
}

function summary(): RepoSummary {
  return {
    id: "r-demo",
    name: "git4u",
    path: "C:\\dev\\sourcecode\\git4u",
    valid: true,
    head: { ...head },
    staged: entries.filter((e) => e.staged).length,
    unstaged: entries.filter((e) => !e.staged && e.worktreeState !== "unmodified").length,
    conflicts: 0,
    ahead: 2,
    behind: 0,
    midState: null,
    stats: {
      sizeBytes: 48_234_496,
      commitCount: 1284,
      branchCount: 6,
      tagCount: 2,
      lastCommitAt: now - 180,
      lastCommitSummary: "feat: 支持 SSO 单点登录",
    },
    error: null,
  };
}

function loginDiff(): FileDiff {
  return {
    path: "src/login.rs",
    oldPath: null,
    binary: false,
    additions: 12,
    deletions: 3,
    hunks: [
      {
        header: "@@ -12,7 +12,9 @@ fn login(u: &User) {",
        oldStart: 12,
        oldLines: 7,
        newStart: 12,
        newLines: 9,
        lines: [
          line("context", 12, 12, "fn login(u: &User) {"),
          line("del", 13, null, "    let t = get(u);"),
          line("add", null, 13, "    let t = get(u).refresh()?;"),
          line("add", null, 14, "    audit(u);"),
          line("context", 14, 15, "    session::open(&t)"),
          line("context", 15, 16, "}"),
        ],
      },
      {
        header: "@@ -31,4 +33,6 @@",
        oldStart: 31,
        oldLines: 4,
        newStart: 33,
        newLines: 6,
        lines: [
          line("add", null, 33, "#[cfg(test)]"),
          line("add", null, 34, "mod tests {}"),
        ],
      },
    ],
  };
}

function line(
  kind: "context" | "add" | "del",
  oldLineno: number | null,
  newLineno: number | null,
  content: string,
) {
  return { kind, oldLineno, newLineno, content };
}

function genericDiff(path: string, staged: boolean): FileDiff {
  return {
    path,
    oldPath: null,
    binary: false,
    additions: 3,
    deletions: 1,
    hunks: [
      {
        header: "@@ -8,5 +8,7 @@",
        oldStart: 8,
        oldLines: 5,
        newStart: 8,
        newLines: 7,
        lines: [
          line("context", 8, 8, `// ${path}`),
          line("del", 9, null, staged ? "// old staged content" : "// old content"),
          line("add", null, 9, "// new content"),
          line("add", null, 10, "// newly introduced line"),
          line("context", 10, 11, "}" ),
        ],
      },
    ],
  };
}

function detail(selected: CommitSummary): CommitDetail {
  return {
    commit: selected,
    body:
      selected.summary.startsWith("feat: 支持 SSO")
        ? "接入企业 IdP（OIDC），登录页增加 SSO 入口，票据自动刷新。"
        : "",
    files:
      selected.summary.startsWith("feat: 支持 SSO")
        ? [
            { path: "src/login.rs", change: "modified", additions: 52, deletions: 9 },
            { path: "src/sso.rs", change: "added", additions: 26, deletions: 0 },
            { path: "src/token.rs", change: "modified", additions: 4, deletions: 5 },
          ]
        : [{ path: "src/app.rs", change: "modified", additions: 3, deletions: 1 }],
  };
}

export const mockBackend = {
  async info(): Promise<AppInfo> {
    return {
      appVersion: "0.1.0",
      gitVersion: "2.55.0",
      engine: "系统 git CLI",
      appDataDir: "浏览器预览（无本地数据目录）",
    };
  },
  async list(): Promise<RepoSummary[]> {
    return [summary()];
  },
  async open(path: string): Promise<RepoSummary> {
    void path;
    return summary();
  },
  async remove(id: string): Promise<void> {
    void id;
  },
  async status(repoId: string): Promise<RepoStatus> {
    void repoId;
    return currentStatus();
  },
  async log(repoId: string, limit?: number): Promise<CommitSummary[]> {
    void repoId;
    return commits.slice(0, limit ?? 500);
  },
  async branches(repoId: string): Promise<BranchListing> {
    void repoId;
    return { branches: branches.map((b) => ({ ...b })), tags: tags.map((t) => ({ ...t })) };
  },
  async diff(repoId: string, path: string, staged: boolean): Promise<FileDiff> {
    void repoId;
    if (path === "src/login.rs") return loginDiff();
    if (path === "src/sso.rs") {
      return {
        path,
        oldPath: null,
        binary: false,
        additions: 4,
        deletions: 0,
        hunks: [
          {
            header: "@@ -0,0 +1,4 @@",
            oldStart: 0,
            oldLines: 0,
            newStart: 1,
            newLines: 4,
            lines: [
              line("add", null, 1, "pub struct SsoProvider {"),
              line("add", null, 2, "    endpoint: Url,"),
              line("add", null, 3, "    client_id: String,"),
              line("add", null, 4, "}"),
            ],
          },
        ],
      };
    }
    return genericDiff(path, staged);
  },
  async gitignore(repoId: string): Promise<GitignoreFile> {
    const content = gitignoreByRepo.get(repoId);
    return { exists: content !== undefined, content: content ?? "" };
  },
  async writeGitignore(repoId: string, content: string, append: boolean): Promise<void> {
    const existing = gitignoreByRepo.get(repoId) ?? "";
    const base = append && existing ? existing.replace(/\s*$/, "") + "\n\n" : "";
    gitignoreByRepo.set(repoId, `${base}${content.trim()}\n`);
  },
  async files(repoId: string): Promise<string[]> {
    void repoId;
    return [...new Set(entries.map((entry) => entry.path))].sort();
  },
  async filePreview(repoId: string, path: string): Promise<FilePreview> {
    void repoId;
    const content = `// 预览（演示数据）\n// ${path}\n`;
    return {
      path,
      size: content.length,
      binary: false,
      truncated: false,
      content,
    };
  },
  async largeFiles(repoId: string, limit: number): Promise<LargeFile[]> {
    void repoId;
    return entries
      .filter((entry) => entry.additions + entry.deletions > 0)
      .map((entry) => ({
        path: entry.path,
        size: (entry.additions + entry.deletions) * 1024,
      }))
      .sort((a, b) => b.size - a.size)
      .slice(0, limit);
  },
  async backupRefs(repoId: string): Promise<BackupRef[]> {
    void repoId;
    return [];
  },
  async checkIgnore(repoId: string, path: string): Promise<IgnoreMatch | null> {
    void repoId;
    return path.includes("node_modules")
      ? { source: ".gitignore", line: 1, pattern: "node_modules/" }
      : null;
  },
  async eolReport(repoId: string): Promise<EolReport> {
    void repoId;
    return {
      total: entries.length,
      lf: Math.max(0, entries.length - 1),
      crlf: 0,
      mixed: 1,
      files: [
        {
          path: entries[0]?.path ?? "README.md",
          index: "lf",
          worktree: "mixed",
          attr: "",
        },
      ],
    };
  },
  async objectStats(repoId: string): Promise<ObjectStats> {
    void repoId;
    return {
      count: 42,
      sizeKb: 128,
      inPack: 1200,
      sizePackKb: 4096,
      packs: 1,
      prunePackable: 0,
      garbage: 0,
      sizeGarbageKb: 0,
    };
  },
  async fsck(repoId: string): Promise<string> {
    void repoId;
    return "";
  },
  async reflog(repoId: string, limit: number): Promise<ReflogEntry[]> {
    void repoId;
    return commits.slice(0, limit).map((item, index) => ({
      oid: item.oid,
      selector: `HEAD@{${index}}`,
      subject: item.summary,
      at: item.committedAt,
    }));
  },
  async lockReport(repoId: string): Promise<LockReport> {
    void repoId;
    return { indexLock: null, midState: null, stateFiles: [] };
  },
  async envReport(repoId: string): Promise<EnvReport> {
    void repoId;
    return {
      gitVersion: "git version 2.45.0 (demo)",
      workdir: "C:/demo/repo",
      gitDir: "C:/demo/repo/.git",
      hooksPath: null,
      credentialHelper: "manager",
      httpProxy: null,
      httpsProxy: null,
      noProxy: null,
    };
  },
  async secretScan(repoId: string, limit: number): Promise<SecretFinding[]> {
    void repoId;
    return limit > 0
      ? [{ path: "src/config.ts", line: 12, pattern: "secretAssignment" }]
      : [];
  },
  async detail(repoId: string, oidValue: string): Promise<CommitDetail> {
    void repoId;
    const selected = commits.find((x) => x.oid === oidValue) ?? commits[0];
    return detail(selected);
  },
  async stage(repoId: string, paths: string[]): Promise<void> {
    void repoId;
    entries = entries.map((e) =>
      paths.includes(e.path)
        ? e.worktreeState === "untracked"
          ? { ...e, indexState: "added", worktreeState: "unmodified", staged: true }
          : { ...e, staged: true, indexState: e.worktreeState }
        : e,
    );
  },
  async unstage(repoId: string, paths: string[]): Promise<void> {
    void repoId;
    entries = entries.map((e) =>
      paths.includes(e.path)
        ? e.indexState === "added"
          ? { ...e, indexState: "unmodified", worktreeState: "untracked", staged: false }
          : { ...e, staged: false, indexState: "unmodified" }
        : e,
    );
  },
  async commit(repoId: string, message: string, amend: boolean, hooks: boolean): Promise<string> {
    void repoId;
    void hooks;
    lastCommit = {
      commits: commits.map((item) => ({
        ...item,
        parents: [...item.parents],
        refs: [...item.refs],
      })),
      entries: entries.map((entry) => ({ ...entry })),
      headOid: head.oid,
    };
    const firstLine = message.split("\n")[0] || "chore: commit";
    const next = c("f" + Math.random().toString(16).slice(2, 8));
    if (amend && commits[0]) {
      commits[0] = { ...commits[0], summary: firstLine, oid: next, shortOid: next.slice(0, 7) };
    } else {
      commits = [
        {
          oid: next,
          shortOid: next.slice(0, 7),
          parents: [head.oid!],
          summary: firstLine,
          authorName: "我",
          authorEmail: "me@example.com",
          committedAt: Math.floor(Date.now() / 1000),
          refs: ["HEAD -> feature/sso"],
        },
        ...commits.map((x) => ({ ...x, refs: x.refs.filter((r) => !r.startsWith("HEAD")) })),
      ];
    }
    entries = entries.filter((e) => !e.staged);
    head.oid = next;
    return next;
  },
  async undoLast(repoId: string, expectedAfter: string): Promise<string | null> {
    void repoId;
    if (!lastCommit || commits[0]?.oid !== expectedAfter) return null;
    commits = lastCommit.commits;
    entries = lastCommit.entries;
    head.oid = lastCommit.headOid;
    lastCommit = null;
    return head.oid;
  },
  async checkout(repoId: string, name: string): Promise<void> {
    void repoId;
    branches = branches.map((b) => ({ ...b, isCurrent: !b.isRemote && b.name === name }));
    head.branch = name;
    head.oid = branches.find((b) => !b.isRemote && b.name === name)?.oid ?? head.oid;
  },
  async createBranch(
    repoId: string,
    name: string,
    start: string | null,
    checkout: boolean,
  ): Promise<void> {
    void repoId;
    const from = start ?? head.oid ?? c("a1b2c3f4");
    branches = [
      { ...branch(name, from.slice(0, 8), checkout, false), oid: from },
      ...branches.map((b) => ({ ...b, isCurrent: checkout ? false : b.isCurrent })),
    ];
    if (checkout) {
      head.branch = name;
      head.oid = from;
    }
  },
  async settings(repoId: string | null): Promise<SettingsView> {
    const repo = repoId ? repoSettings.get(repoId) ?? null : null;
    return {
      global: { ...globalSettings },
      repo: repo ? { ...repo } : null,
      effective: { ...(repo ?? globalSettings) },
      proxies: proxies.map((proxy) => ({ ...proxy })),
    };
  },
  async saveSettings(
    scope: SettingsScope,
    repoId: string | null,
    settings: RepoSettings,
  ): Promise<SettingsView> {
    if (scope === "global") {
      globalSettings = { ...settings };
    } else if (repoId) {
      if (JSON.stringify(settings) === JSON.stringify(globalSettings)) {
        repoSettings.delete(repoId);
      } else {
        repoSettings.set(repoId, { ...settings });
      }
    }
    return this.settings(repoId);
  },
  async remotes(repoId: string): Promise<RemoteInfo[]> {
    void repoId;
    return [{ name: "origin", url: "https://github.com/git4u/git4u.git" }];
  },
  async fetch(repoId: string, remote?: string | null): Promise<void> {
    void repoId;
    void remote;
  },
  async pull(repoId: string): Promise<void> {
    void repoId;
  },
  async pushAudit(repoId: string): Promise<PushAuditReport> {
    void repoId;
    return {
      branch: head.branch ?? "",
      upstream: "origin/feature/sso",
      ahead: 2,
      behind: 0,
      blockers: [],
      warnings: [],
    };
  },
  async push(
    repoId: string,
    remote: string | null,
    setUpstream: boolean,
    allowRisky: boolean,
  ): Promise<void> {
    void repoId;
    void remote;
    void setUpstream;
    void allowRisky;
  },
  async credentials(): Promise<CredentialEntry[]> {
    return credentials.map((entry) => ({ ...entry }));
  },
  async saveCredential(input: CredentialInput): Promise<CredentialEntry> {
    const id = input.id ?? `c-demo-${credentials.length + 1}`;
    const entry: CredentialEntry = {
      id,
      url: input.url.replace(/\.git$/, "").replace(/\/$/, ""),
      mode: input.mode,
      username: input.username,
      lastUsed: credentials.find((item) => item.id === id)?.lastUsed ?? null,
      status: "unverified",
    };
    credentials = [entry, ...credentials.filter((item) => item.id !== id)];
    return { ...entry };
  },
  async deleteCredential(id: string): Promise<void> {
    credentials = credentials.filter((entry) => entry.id !== id);
  },
  async validateCredential(repoId: string, id: string): Promise<CredentialValidation> {
    void repoId;
    credentials = credentials.map((entry) =>
      entry.id === id
        ? { ...entry, status: "ok", lastUsed: Math.floor(Date.now() / 1000) }
        : entry,
    );
    return { ok: true, message: "验证通过（演示数据）。", status: "ok" };
  },
  async saveProxy(input: ProxyInput): Promise<ProxyEntry[]> {
    const entry: ProxyEntry = {
      id: input.id ?? `p-demo-${proxies.length + 1}`,
      name: input.name,
      url: input.url,
    };
    proxies = [entry, ...proxies.filter((proxy) => proxy.id !== entry.id)];
    return proxies.map((proxy) => ({ ...proxy }));
  },
  async deleteProxy(id: string): Promise<ProxyEntry[]> {
    proxies = proxies.filter((proxy) => proxy.id !== id);
    if (globalSettings.proxyId === id) globalSettings = { ...globalSettings, proxyId: null };
    for (const [repoId, settings] of repoSettings) {
      if (settings.proxyId === id) repoSettings.set(repoId, { ...settings, proxyId: null });
    }
    return proxies.map((proxy) => ({ ...proxy }));
  },
  async resetRepoSettings(): Promise<SettingsView> {
    globalSettings = { ...defaultSettings };
    repoSettings.clear();
    return this.settings(null);
  },
  async cleanupBackups(repoId: string): Promise<number> {
    void repoId;
    return 0;
  },
};
