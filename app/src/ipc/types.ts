// IPC 契约类型。M0 手写镜像 g4u-model；后续由 model 层 JSON Schema 生成（02-1.2）。

export type ErrorKind =
  | "notARepo"
  | "gitNotFound"
  | "noSuchRepo"
  | "indexLocked"
  | "identityMissing"
  | "authFailed"
  | "checkoutConflict"
  | "midStateBlocking"
  | "nothingToCommit"
  | "emptyMessage"
  | "commandFailed"
  | "internal";

export type Risk = "safe" | "caution" | "dangerous";

export interface Suggestion {
  label: string;
  risk: Risk;
  action: string;
  /** 动作的可选目标，例如要更新的凭证 id；旧后端可能不返回。 */
  target?: string | null;
}

export interface G4uError {
  kind: ErrorKind;
  human: string;
  causeChain: string[];
  suggestions: Suggestion[];
  cliEquivalent: string | null;
}

export type HeadKind = "branch" | "detached" | "unborn";

export interface HeadState {
  kind: HeadKind;
  branch: string | null;
  oid: string | null;
}

export type ChangeKind =
  | "unmodified"
  | "added"
  | "modified"
  | "deleted"
  | "renamed"
  | "copied"
  | "untracked"
  | "unmerged";

export interface StatusEntry {
  path: string;
  origPath: string | null;
  indexState: ChangeKind;
  worktreeState: ChangeKind;
  staged: boolean;
  additions: number;
  deletions: number;
}

export interface UpstreamInfo {
  name: string;
  ahead: number;
  behind: number;
  gone: boolean;
}

export interface RepoStatus {
  version: number;
  head: HeadState;
  upstream: UpstreamInfo | null;
  entries: StatusEntry[];
  conflicts: string[];
  midState: string | null;
}

export interface RepoStats {
  /** Git 对象存储（.git）占用，字节；不含工作区文件。 */
  sizeBytes: number;
  commitCount: number;
  branchCount: number;
  tagCount: number;
  lastCommitAt: number | null;
  lastCommitSummary: string | null;
}

export interface RepoSummary {
  id: string;
  name: string;
  path: string;
  valid: boolean;
  head: HeadState | null;
  staged: number;
  unstaged: number;
  conflicts: number;
  ahead: number;
  behind: number;
  midState: string | null;
  stats: RepoStats | null;
  error: string | null;
}

export interface AppInfo {
  appVersion: string;
  gitVersion: string;
  engine: string;
  appDataDir: string;
}

export interface GitignoreFile {
  exists: boolean;
  content: string;
}

export interface FilePreview {
  path: string;
  size: number;
  binary: boolean;
  truncated: boolean;
  content: string;
}

export interface BackupRef {
  name: string;
  oid: string;
}

export interface LargeFile {
  path: string;
  size: number;
}

export interface IgnoreMatch {
  source: string;
  line: number;
  pattern: string;
}

export interface EolFile {
  path: string;
  index: string;
  worktree: string;
  attr: string;
}

export interface EolReport {
  total: number;
  lf: number;
  crlf: number;
  mixed: number;
  files: EolFile[];
}

export interface ObjectStats {
  count: number;
  sizeKb: number;
  inPack: number;
  sizePackKb: number;
  packs: number;
  prunePackable: number;
  garbage: number;
  sizeGarbageKb: number;
}

export interface ReflogEntry {
  oid: string;
  selector: string;
  subject: string;
  at: number;
}

export interface LockInfo {
  modifiedAt: number;
  ageSeconds: number;
}

export interface LockReport {
  indexLock: LockInfo | null;
  midState: string | null;
  stateFiles: string[];
}

export interface EnvReport {
  gitVersion: string;
  workdir: string;
  gitDir: string;
  hooksPath: string | null;
  credentialHelper: string | null;
  httpProxy: string | null;
  httpsProxy: string | null;
  noProxy: string | null;
}

export interface SecretFinding {
  path: string;
  line: number;
  pattern: string;
}

export type PullStrategy = "merge" | "rebase" | "ffOnly";

export interface RepoSettings {
  pullStrategy: PullStrategy;
  protectedBranches: string;
  pushAudit: boolean;
  secretScan: boolean;
  proxyId: string | null;
  autoFetch: boolean;
  autoFetchMinutes: number;
  backupRetentionDays: number;
}

export type SettingsScope = "global" | "repo";

export interface ProxyEntry {
  id: string;
  name: string;
  url: string;
}

export interface ProxyInput {
  id: string | null;
  name: string;
  url: string;
}

export interface SettingsView {
  global: RepoSettings;
  repo: RepoSettings | null;
  effective: RepoSettings;
  proxies: ProxyEntry[];
}

export interface RemoteInfo {
  name: string;
  url: string;
}

export interface AuditFinding {
  kind: string;
  path: string | null;
  detail: string;
}

export interface PushAuditReport {
  branch: string;
  upstream: string | null;
  ahead: number;
  behind: number;
  blockers: AuditFinding[];
  warnings: AuditFinding[];
}

export type CredentialMode = "password" | "token";

export interface CredentialEntry {
  id: string;
  url: string;
  mode: CredentialMode;
  username: string;
  lastUsed: number | null;
  status: string;
}

export interface CredentialInput {
  id: string | null;
  url: string;
  mode: CredentialMode;
  username: string;
  password: string | null;
}

export interface CredentialValidation {
  ok: boolean;
  message: string;
  status: string;
}

export interface CommitSummary {
  oid: string;
  shortOid: string;
  parents: string[];
  summary: string;
  authorName: string;
  authorEmail: string;
  committedAt: number;
  refs: string[];
}

export interface FileStat {
  path: string;
  change: ChangeKind;
  additions: number;
  deletions: number;
}

export interface CommitDetail {
  commit: CommitSummary;
  body: string;
  files: FileStat[];
}

export interface BranchInfo {
  name: string;
  fullName: string;
  oid: string;
  isCurrent: boolean;
  isRemote: boolean;
  remote: string | null;
  upstream: string | null;
  track: string | null;
  ahead: number;
  behind: number;
  gone: boolean;
  updatedAt: number;
}

export interface TagInfo {
  name: string;
  oid: string;
  subject: string;
}

export interface BranchListing {
  branches: BranchInfo[];
  tags: TagInfo[];
}

export type DiffLineKind = "context" | "add" | "del";

export interface DiffLine {
  kind: DiffLineKind;
  oldLineno: number | null;
  newLineno: number | null;
  content: string;
}

export interface DiffHunk {
  header: string;
  oldStart: number;
  oldLines: number;
  newStart: number;
  newLines: number;
  lines: DiffLine[];
}

export interface FileDiff {
  path: string;
  oldPath: string | null;
  binary: boolean;
  additions: number;
  deletions: number;
  hunks: DiffHunk[];
}
