import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { t } from "@/lib/i18n";
import { mockBackend } from "./mock";
import type {
  AppInfo,
  BackupRef,
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
  G4uError,
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
} from "./types";

export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

interface Backend {
  info(): Promise<AppInfo>;
  list(): Promise<RepoSummary[]>;
  open(path: string): Promise<RepoSummary>;
  remove(id: string): Promise<void>;
  status(repoId: string): Promise<RepoStatus>;
  log(repoId: string, limit?: number): Promise<CommitSummary[]>;
  branches(repoId: string): Promise<BranchListing>;
  diff(repoId: string, path: string, staged: boolean): Promise<FileDiff>;
  gitignore(repoId: string): Promise<GitignoreFile>;
  writeGitignore(repoId: string, content: string, append: boolean): Promise<void>;
  files(repoId: string): Promise<string[]>;
  filePreview(repoId: string, path: string): Promise<FilePreview>;
  largeFiles(repoId: string, limit: number): Promise<LargeFile[]>;
  backupRefs(repoId: string): Promise<BackupRef[]>;
  checkIgnore(repoId: string, path: string): Promise<IgnoreMatch | null>;
  eolReport(repoId: string): Promise<EolReport>;
  objectStats(repoId: string): Promise<ObjectStats>;
  fsck(repoId: string): Promise<string>;
  reflog(repoId: string, limit: number): Promise<ReflogEntry[]>;
  lockReport(repoId: string): Promise<LockReport>;
  envReport(repoId: string): Promise<EnvReport>;
  secretScan(repoId: string, limit: number): Promise<SecretFinding[]>;
  detail(repoId: string, oid: string): Promise<CommitDetail>;
  stage(repoId: string, paths: string[]): Promise<void>;
  unstage(repoId: string, paths: string[]): Promise<void>;
  commit(repoId: string, message: string, amend: boolean, hooks: boolean): Promise<string>;
  undoLast(repoId: string, expectedAfter: string): Promise<string | null>;
  checkout(repoId: string, name: string): Promise<void>;
  createBranch(
    repoId: string,
    name: string,
    start: string | null,
    checkout: boolean,
  ): Promise<void>;
  settings(repoId: string | null): Promise<SettingsView>;
  saveSettings(
    scope: SettingsScope,
    repoId: string | null,
    settings: RepoSettings,
  ): Promise<SettingsView>;
  remotes(repoId: string): Promise<RemoteInfo[]>;
  fetch(repoId: string, remote?: string | null): Promise<void>;
  pull(repoId: string): Promise<void>;
  pushAudit(repoId: string): Promise<PushAuditReport>;
  push(
    repoId: string,
    remote: string | null,
    setUpstream: boolean,
    allowRisky: boolean,
  ): Promise<void>;
  credentials(): Promise<CredentialEntry[]>;
  saveCredential(input: CredentialInput): Promise<CredentialEntry>;
  deleteCredential(id: string): Promise<void>;
  validateCredential(repoId: string, id: string): Promise<CredentialValidation>;
  saveProxy(input: ProxyInput): Promise<ProxyEntry[]>;
  deleteProxy(id: string): Promise<ProxyEntry[]>;
  resetRepoSettings(): Promise<SettingsView>;
  cleanupBackups(repoId: string): Promise<number>;
}

const tauriBackend: Backend = {
  info: () => invoke<AppInfo>("app_info"),
  list: () => invoke<RepoSummary[]>("repo_list"),
  open: (path) => invoke<RepoSummary>("repo_open", { path }),
  remove: (repoId) => invoke<void>("repo_remove", { repoId }),
  status: (repoId) => invoke<RepoStatus>("repo_status", { repoId }),
  log: (repoId, limit) => invoke<CommitSummary[]>("repo_log", { repoId, limit }),
  branches: (repoId) => invoke<BranchListing>("repo_branches", { repoId }),
  diff: (repoId, path, staged) => invoke<FileDiff>("repo_diff", { repoId, path, staged }),
  gitignore: (repoId) => invoke<GitignoreFile>("repo_gitignore", { repoId }),
  writeGitignore: (repoId, content, append) =>
    invoke<void>("repo_write_gitignore", { repoId, content, append }),
  files: (repoId) => invoke<string[]>("repo_files", { repoId }),
  filePreview: (repoId, path) => invoke<FilePreview>("repo_file_preview", { repoId, path }),
  largeFiles: (repoId, limit) => invoke<LargeFile[]>("repo_large_files", { repoId, limit }),
  backupRefs: (repoId) => invoke<BackupRef[]>("repo_backup_refs", { repoId }),
  checkIgnore: (repoId, path) =>
    invoke<IgnoreMatch | null>("repo_check_ignore", { repoId, path }),
  eolReport: (repoId) => invoke<EolReport>("repo_eol_report", { repoId }),
  objectStats: (repoId) => invoke<ObjectStats>("repo_object_stats", { repoId }),
  fsck: (repoId) => invoke<string>("repo_fsck", { repoId }),
  reflog: (repoId, limit) => invoke<ReflogEntry[]>("repo_reflog", { repoId, limit }),
  lockReport: (repoId) => invoke<LockReport>("repo_lock_report", { repoId }),
  envReport: (repoId) => invoke<EnvReport>("repo_env_report", { repoId }),
  secretScan: (repoId, limit) => invoke<SecretFinding[]>("repo_secret_scan", { repoId, limit }),
  detail: (repoId, oid) => invoke<CommitDetail>("repo_commit_detail", { repoId, oid }),
  stage: (repoId, paths) => invoke<void>("index_stage", { repoId, paths }),
  unstage: (repoId, paths) => invoke<void>("index_unstage", { repoId, paths }),
  commit: (repoId, message, amend, runHooks) =>
    invoke<string>("commit_create", { repoId, message, amend, runHooks }),
  undoLast: (repoId, expectedAfter) =>
    invoke<string | null>("repo_undo_last", { repoId, expectedAfter }),
  checkout: (repoId, name) => invoke<void>("branch_checkout", { repoId, name }),
  createBranch: (repoId, name, start, checkout) =>
    invoke<void>("branch_create", { repoId, name, start, checkout }),
  settings: (repoId) => invoke<SettingsView>("repo_settings", { repoId }),
  saveSettings: (scope, repoId, settings) =>
    invoke<SettingsView>("repo_save_settings", { scope, repoId, settings }),
  remotes: (repoId) => invoke<RemoteInfo[]>("repo_remotes", { repoId }),
  fetch: (repoId, remote) => invoke<void>("repo_fetch", { repoId, remote: remote ?? null }),
  pull: (repoId) => invoke<void>("repo_pull", { repoId }),
  pushAudit: (repoId) => invoke<PushAuditReport>("repo_push_audit", { repoId }),
  push: (repoId, remote, setUpstream, allowRisky) =>
    invoke<void>("repo_push", { repoId, remote, setUpstream, allowRisky }),
  credentials: () => invoke<CredentialEntry[]>("credential_list"),
  saveCredential: (input) => invoke<CredentialEntry>("credential_save", { input }),
  deleteCredential: (id) => invoke<void>("credential_delete", { id }),
  validateCredential: (repoId, id) =>
    invoke<CredentialValidation>("credential_validate", { repoId, id }),
  saveProxy: (input) => invoke<ProxyEntry[]>("settings_save_proxy", { input }),
  deleteProxy: (id) => invoke<ProxyEntry[]>("settings_delete_proxy", { id }),
  resetRepoSettings: () => invoke<SettingsView>("settings_reset"),
  cleanupBackups: (repoId) => invoke<number>("repo_cleanup_backups", { repoId }),
};

const backend = isTauri ? tauriBackend : mockBackend;

export const api = backend;

/** 系统目录选择；浏览器预览时不提供。 */
export async function pickFolder(): Promise<string | null> {
  if (!isTauri) return null;
  const selected = await openDialog({
    directory: true,
    multiple: false,
    title: "选择 Git 仓库目录",
  });
  return typeof selected === "string" ? selected : null;
}

/** 打开设置窗口；浏览器预览时由调用方回退到路由。 */
export async function openSettingsWindow(
  repoId: string | null,
  section?: string,
  credential?: string | null,
): Promise<boolean> {
  if (!isTauri) return false;
  await invoke<void>("open_settings_window", {
    repoId,
    title: t("settings.windowTitle"),
    section: section ?? null,
    credential: credential ?? null,
  });
  return true;
}

export async function closeSettingsWindow(): Promise<void> {
  if (isTauri) await invoke<void>("close_settings_window");
}

export async function notifySettingsChanged(repoId: string | null): Promise<void> {
  if (isTauri) await emit("g4u:settings-changed", { repoId });
}

export async function onSettingsChanged(handler: () => void): Promise<() => void> {
  if (!isTauri) return () => {};
  return listen("g4u:settings-changed", () => handler());
}

export async function notifyUiSettingsChanged(): Promise<void> {
  if (isTauri) await emit("g4u:ui-settings-changed");
}

export async function onUiSettingsChanged(handler: () => void): Promise<() => void> {
  if (!isTauri) return () => {};
  return listen("g4u:ui-settings-changed", () => handler());
}

export function errorText(error: unknown): string {
  if (error && typeof error === "object" && "human" in error) {
    const shaped = error as G4uError;
    const human = shaped.human || t("common.operationFailed");
    const suggestions = (shaped.suggestions ?? []).map((item) => item.label).filter(Boolean);
    return suggestions.length
      ? `${human}${t("common.suggestions", { items: suggestions.join(t("common.suggestionSeparator")) })}`
      : human;
  }
  if (error instanceof Error) return error.message;
  return typeof error === "string" ? error : t("common.operationFailedRetry");
}
