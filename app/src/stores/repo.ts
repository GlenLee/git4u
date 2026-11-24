import { computed, ref, shallowRef, watch } from "vue";
import { defineStore } from "pinia";
import { api, errorText, isTauri, notifyUiSettingsChanged, pickFolder } from "@/ipc";
import type {
  BranchInfo,
  CommitSummary,
  PushAuditReport,
  RepoStatus,
  RepoSummary,
  SettingsView,
  TagInfo,
} from "@/ipc/types";
import { setLanguage, t, type UiLanguage } from "@/lib/i18n";
import { applyTheme, naiveTheme, type ThemeMode } from "@/styles/tokens";

export type UiTheme = ThemeMode | "system";

export interface UiSettings {
  theme: UiTheme;
  language: UiLanguage;
  fileView: "list" | "tree";
  diffMode: "side" | "unified";
  refreshInterval: number;
}

const UI_SETTINGS_KEY = "git4u.settings";
const UI_DEFAULTS: UiSettings = {
  theme: "system",
  language: "zh",
  fileView: "list",
  diffMode: "side",
  refreshInterval: 5000,
};

function loadUiSettings(): UiSettings {
  const settings = { ...UI_DEFAULTS };
  const raw = localStorage.getItem(UI_SETTINGS_KEY);
  let hasSaved = false;

  if (raw) {
    try {
      const parsed = JSON.parse(raw) as Partial<UiSettings>;
      if (parsed.theme === "dark" || parsed.theme === "light" || parsed.theme === "system") {
        settings.theme = parsed.theme;
      }
      if (parsed.language === "zh" || parsed.language === "en") {
        settings.language = parsed.language;
      }
      if (parsed.fileView === "list" || parsed.fileView === "tree") {
        settings.fileView = parsed.fileView;
      }
      if (parsed.diffMode === "side" || parsed.diffMode === "unified") {
        settings.diffMode = parsed.diffMode;
      }
      if (typeof parsed.refreshInterval === "number") {
        settings.refreshInterval = parsed.refreshInterval;
      }
      hasSaved = true;
    } catch {
      // 损坏的本地设置回退到默认值。
    }
  }

  // 从 M0 的单键存储迁移。
  if (!hasSaved) {
    const legacyTheme = localStorage.getItem("git4u.theme");
    if (legacyTheme === "dark" || legacyTheme === "light") settings.theme = legacyTheme;
    const legacyView = localStorage.getItem("git4u.fileView");
    if (legacyView === "list" || legacyView === "tree") settings.fileView = legacyView;
  }

  return settings;
}

export const useUiStore = defineStore("ui", () => {
  const settings = ref<UiSettings>(loadUiSettings());
  let lastUiSettings = JSON.stringify(settings.value);
  const media = window.matchMedia("(prefers-color-scheme: dark)");
  const systemDark = ref(media.matches);
  media.addEventListener("change", (event) => {
    systemDark.value = event.matches;
  });

  const isDark = computed(
    () =>
      settings.value.theme === "dark" ||
      (settings.value.theme === "system" && systemDark.value),
  );
  const overrides = computed(() => naiveTheme(isDark.value ? "dark" : "light"));

  watch(
    settings,
    (value) => {
      const text = JSON.stringify(value);
      localStorage.setItem(UI_SETTINGS_KEY, text);
      if (text !== lastUiSettings) {
        lastUiSettings = text;
        void notifyUiSettingsChanged();
      }
    },
    { deep: true },
  );
  watch(isDark, (dark) => applyTheme(dark ? "dark" : "light"), { immediate: true });
  watch(() => settings.value.language, setLanguage, { immediate: true });

  function reloadSettings(): void {
    const next = loadUiSettings();
    lastUiSettings = JSON.stringify(next);
    settings.value = next;
  }

  function toggle(): void {
    settings.value.theme = isDark.value ? "light" : "dark";
  }

  function resetSettings(): void {
    settings.value = { ...UI_DEFAULTS };
    localStorage.removeItem("git4u.theme");
    localStorage.removeItem("git4u.fileView");
    localStorage.setItem(UI_SETTINGS_KEY, JSON.stringify(settings.value));
  }

  return { settings, isDark, overrides, toggle, resetSettings, reloadSettings };
});

export const useRepoStore = defineStore("repo", () => {
  const repos = ref<RepoSummary[]>([]);
  const activeId = ref<string | null>(null);
  const status = shallowRef<RepoStatus | null>(null);
  const log = shallowRef<CommitSummary[]>([]);
  const branches = ref<BranchInfo[]>([]);
  const tags = ref<TagInfo[]>([]);
  const loading = ref(false);
  const hydrating = ref(true);
  const lastError = ref<string | null>(null);
  const settings = shallowRef<SettingsView | null>(null);

  const activeRepo = computed(() => repos.value.find((r) => r.id === activeId.value) ?? null);
  const currentBranch = computed(
    () => status.value?.head.branch ?? activeRepo.value?.head?.branch ?? "",
  );
  const stagedEntries = computed(() =>
    (status.value?.entries ?? []).filter((e) => e.staged),
  );
  const unstagedEntries = computed(() =>
    (status.value?.entries ?? []).filter(
      (e) => !e.staged && e.worktreeState !== "unmodified" && e.worktreeState !== "unmerged",
    ),
  );
  const conflictEntries = computed(() =>
    (status.value?.entries ?? []).filter((e) => e.worktreeState === "unmerged"),
  );
  const unstagedCount = computed(() => unstagedEntries.value.length);
  const stagedCount = computed(() => stagedEntries.value.length);
  const conflictCount = computed(() => conflictEntries.value.length);
  const localBranches = computed(() => branches.value.filter((b) => !b.isRemote));

  async function loadSettings(): Promise<void> {
    const id = activeId.value;
    if (!id) {
      settings.value = null;
      return;
    }
    try {
      const next = await api.settings(id);
      if (activeId.value === id) settings.value = next;
    } catch {
      // 设置读取失败不阻断主流程；设置页会展示具体错误。
    }
  }

  async function hydrate(): Promise<void> {
    hydrating.value = true;
    try {
      repos.value = await api.list();
      if (!activeId.value && repos.value.length > 0) {
        const first = repos.value.find((r) => r.valid);
        if (first) {
          activeId.value = first.id;
          await refreshAll(true);
          await loadSettings();
        }
      }
    } catch (error) {
      lastError.value = errorText(error);
    } finally {
      hydrating.value = false;
    }
  }

  async function openRepo(): Promise<"opened" | "cancelled" | "preview"> {
    if (!isTauri) return "preview";
    const path = await pickFolder();
    if (!path) return "cancelled";
    const summary = await api.open(path);
    repos.value = await api.list();
    activeId.value = summary.id;
    await refreshAll(true);
    await loadSettings();
    return "opened";
  }

  async function activate(id: string): Promise<void> {
    if (activeId.value === id && status.value) return;
    activeId.value = id;
    await refreshAll(true);
    await loadSettings();
  }

  function showDashboard(): void {
    activeId.value = null;
    status.value = null;
    log.value = [];
    branches.value = [];
    tags.value = [];
    settings.value = null;
  }

  async function removeRepo(id: string): Promise<void> {
    await api.remove(id);
    repos.value = repos.value.filter((r) => r.id !== id);
    if (activeId.value === id) {
      showDashboard();
      const next = repos.value.find((r) => r.valid);
      if (next) await activate(next.id);
    }
  }

  async function refreshAll(showSpinner = false): Promise<void> {
    const id = activeId.value;
    if (!id) return;
    if (showSpinner) loading.value = true;
    try {
      const [nextStatus, nextLog, listing] = await Promise.all([
        api.status(id),
        api.log(id, 500),
        api.branches(id),
      ]);
      if (activeId.value !== id) return;
      status.value = nextStatus;
      log.value = nextLog;
      branches.value = listing.branches;
      tags.value = listing.tags;
      syncSummary(nextStatus);
      lastError.value = null;
    } catch (error) {
      lastError.value = errorText(error);
    } finally {
      loading.value = false;
    }
  }

  /** watcher/polling 用的轻量刷新：只拉状态，不阻塞界面。 */
  async function refreshStatus(): Promise<void> {
    const id = activeId.value;
    if (!id) return;
    try {
      const nextStatus = await api.status(id);
      if (activeId.value !== id) return;
      status.value = nextStatus;
      syncSummary(nextStatus);
    } catch {
      // 轮询失败保持静默；用户主动操作时的错误才提示。
    }
  }

  function syncSummary(next: RepoStatus): void {
    const repo = repos.value.find((r) => r.id === activeId.value);
    if (!repo) return;
    repo.head = next.head;
    repo.staged = next.entries.filter((e) => e.staged).length;
    repo.unstaged = next.entries.filter(
      (e) => !e.staged && e.worktreeState !== "unmodified" && e.worktreeState !== "unmerged",
    ).length;
    repo.conflicts = next.conflicts.length;
    repo.ahead = next.upstream?.ahead ?? 0;
    repo.behind = next.upstream?.behind ?? 0;
    repo.midState = next.midState;
  }

  async function stage(paths: string[]): Promise<void> {
    const id = requireActive();
    await api.stage(id, paths);
    await refreshStatus();
  }

  async function unstage(paths: string[]): Promise<void> {
    const id = requireActive();
    await api.unstage(id, paths);
    await refreshStatus();
  }

  async function commit(message: string, amend: boolean, runHooks: boolean): Promise<string> {
    const id = requireActive();
    const oid = await api.commit(id, message, amend, runHooks);
    await refreshAll(true);
    return oid;
  }

  /** 撤销最近一次提交/Amend（soft reset，保留暂存区）。 */
  async function undoLast(expectedAfter: string): Promise<boolean> {
    const id = requireActive();
    const restored = await api.undoLast(id, expectedAfter);
    await refreshAll(true);
    return restored !== null;
  }

  async function checkout(name: string): Promise<void> {
    const id = requireActive();
    await api.checkout(id, name);
    await refreshAll(true);
  }

  async function createBranch(
    name: string,
    start: string | null,
    checkoutAfter: boolean,
  ): Promise<void> {
    const id = requireActive();
    await api.createBranch(id, name, start, checkoutAfter);
    await refreshAll(true);
  }

  async function fetch(remote?: string | null): Promise<void> {
    const id = requireActive();
    await api.fetch(id, remote);
    await refreshAll(true);
  }

  async function pull(): Promise<void> {
    const id = requireActive();
    await api.pull(id);
    await refreshAll(true);
  }

  async function pushAudit(): Promise<PushAuditReport> {
    return api.pushAudit(requireActive());
  }

  async function push(allowRisky: boolean): Promise<void> {
    const id = requireActive();
    const report = await api.pushAudit(id);
    await api.push(id, null, !report.upstream, allowRisky);
    await refreshAll(true);
  }

  function requireActive(): string {
    if (!activeId.value) throw new Error(t("common.openRepoFirst"));
    return activeId.value;
  }

  return {
    repos,
    activeId,
    status,
    log,
    branches,
    tags,
    loading,
    hydrating,
    lastError,
    settings,
    activeRepo,
    currentBranch,
    stagedEntries,
    unstagedEntries,
    conflictEntries,
    stagedCount,
    unstagedCount,
    conflictCount,
    localBranches,
    hydrate,
    openRepo,
    activate,
    showDashboard,
    removeRepo,
    refreshAll,
    refreshStatus,
    loadSettings,
    stage,
    unstage,
    commit,
    undoLast,
    checkout,
    createBranch,
    fetch,
    pull,
    pushAudit,
    push,
  };
});
