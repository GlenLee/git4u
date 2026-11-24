<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import {
  NButton,
  NCheckbox,
  NInput,
  NInputNumber,
  NModal,
  NPopconfirm,
  NRadio,
  NRadioGroup,
  NSelect,
  useMessage,
} from "naive-ui";
import { useRoute, useRouter } from "vue-router";
import { api, closeSettingsWindow, errorText, isTauri, notifySettingsChanged, onUiSettingsChanged } from "@/ipc";import type {
  AppInfo,
  CredentialEntry,
  CredentialInput,
  CredentialMode,
  ProxyEntry,
  ProxyInput,
  RepoSettings,
  SettingsScope,
  SettingsView,
} from "@/ipc/types";
import { LANGUAGES, t } from "@/lib/i18n";
import { useProblemReporter } from "@/lib/problems";
import { useRepoStore, useUiStore, type UiTheme } from "@/stores/repo";

const ui = useUiStore();
const repo = useRepoStore();
const router = useRouter();
const route = useRoute();
const message = useMessage();
const report = useProblemReporter();
const windowRepoId = computed(() => (route.query.repo as string) || repo.activeId);
const standalone = computed(() => route.path === "/settings-window");

type SectionKey =
  | "appearance"
  | "workspace"
  | "repo"
  | "proxy"
  | "credentials"
  | "shortcuts"
  | "gitconfig"
  | "external"
  | "ai"
  | "advanced"
  | "about";

const sections = computed<Array<{ key: SectionKey; label: string; hint: string }>>(() => [
  { key: "appearance", label: t("settings.sec.appearance"), hint: t("settings.sec.appearanceHint") },
  { key: "credentials", label: t("settings.sec.credentials"), hint: t("settings.sec.credentialsHint") },
  { key: "proxy", label: t("settings.sec.proxy"), hint: t("settings.sec.proxyHint") },
  { key: "repo", label: t("settings.sec.repo"), hint: t("settings.sec.repoHint") },
  { key: "shortcuts", label: t("settings.sec.shortcuts"), hint: t("settings.sec.shortcutsHint") },
  { key: "gitconfig", label: t("settings.sec.gitconfig"), hint: t("settings.sec.gitconfigHint") },
  { key: "external", label: t("settings.sec.external"), hint: t("settings.sec.externalHint") },
  { key: "ai", label: t("settings.sec.ai"), hint: t("settings.sec.aiHint") },
  { key: "advanced", label: t("settings.sec.advanced"), hint: t("settings.sec.advancedHint") },
  { key: "workspace", label: t("settings.sec.workspace"), hint: t("settings.sec.workspaceHint") },
  { key: "about", label: t("settings.sec.about"), hint: t("settings.sec.aboutHint") },
]);

const fieldIndex: Record<SectionKey, string[]> = {
  credentials: ["凭证", "账号", "密码", "Token", "钥匙串", "验证", "credential", "account", "password", "keychain", "token"],
  proxy: ["代理", "协议", "主机", "端口", "proxy", "http", "https", "socks5", "port"],
  repo: ["Pull", "保护分支", "推送审计", "密钥扫描", "自动 Fetch", "备份", "protected", "audit", "secret", "fetch", "backup"],
  appearance: ["主题", "暗色", "亮色", "语言", "字体", "色板", "theme", "dark", "light", "language", "font", "palette"],
  shortcuts: ["快捷键", "Ctrl", "改键", "shortcut", "keybind", "rebind"],
  gitconfig: ["gitconfig", "全局配置", "仓库配置", "config", "global", "repository"],
  external: ["Diff", "Merge", "Editor", "外部工具", "external", "tool"],
  ai: ["AI", "Provider", "模型", "密钥", "model", "key"],
  advanced: ["缓存", "日志", "实验", "cache", "log", "experiment"],
  workspace: ["文件视图", "Diff", "刷新", "file", "view", "refresh"],
  about: ["版本", "Git", "数据目录", "version", "data", "directory"],
};

const themes = computed<Array<{ value: UiTheme; label: string; hint: string }>>(() => [
  { value: "system", label: t("settings.theme.system"), hint: t("settings.theme.systemHint") },
  { value: "dark", label: t("settings.theme.dark"), hint: t("settings.theme.darkHint") },
  { value: "light", label: t("settings.theme.light"), hint: t("settings.theme.lightHint") },
]);

const refreshOptions = computed(() => [
  { label: t("settings.workspace.refreshOff"), value: 0 },
  { label: t("settings.workspace.refresh3s"), value: 3000 },
  { label: t("settings.workspace.refresh5s"), value: 5000 },
  { label: t("settings.workspace.refresh10s"), value: 10000 },
]);

const autoFetchOptions = computed(() => [
  { label: t("settings.repo.autoFetch1"), value: 1 },
  { label: t("settings.repo.autoFetch5"), value: 5 },
  { label: t("settings.repo.autoFetch30"), value: 30 },
]);

const activeSection = ref<SectionKey>("appearance");
const settingsSearch = ref("");
const filteredSections = computed(() => {
  const query = settingsSearch.value.trim().toLowerCase();
  if (!query) return sections.value;
  return sections.value.filter((section) =>
    `${section.label} ${section.hint} ${fieldIndex[section.key].join(" ")}`
      .toLowerCase()
      .includes(query),
  );
});
watch(settingsSearch, () => {
  if (!filteredSections.value.some((section) => section.key === activeSection.value)) {
    activeSection.value = filteredSections.value[0]?.key ?? activeSection.value;
  }
});
const currentSection = computed(
  () => sections.value.find((section) => section.key === activeSection.value) ?? sections.value[0],
);
const themeHint = computed(
  () => themes.value.find((theme) => theme.value === ui.settings.theme)?.hint ?? "",
);

const info = ref<AppInfo | null>(null);
const infoError = ref("");

const DEFAULT_REPO_SETTINGS: RepoSettings = {
  pullStrategy: "merge",
  protectedBranches: "^(main|master|develop|release/.*)$",
  pushAudit: true,
  secretScan: true,
  proxyId: null,
  autoFetch: false,
  autoFetchMinutes: 5,
  backupRetentionDays: 30,
};

const settingsScope = ref<SettingsScope>("global");
const settingsView = ref<SettingsView | null>(null);
const settingsLoading = ref(false);
const repoDraft = ref<RepoSettings>({ ...DEFAULT_REPO_SETTINGS });
let settingsLoaded = false;
let saveTimer: ReturnType<typeof setTimeout> | undefined;

function applyScope(): void {
  if (!settingsView.value) return;
  settingsLoaded = false;
  if (saveTimer) clearTimeout(saveTimer);
  const source =
    settingsScope.value === "global"
      ? settingsView.value.global
      : settingsView.value.repo ?? settingsView.value.effective;
  repoDraft.value = { ...source };
  void nextTick(() => {
    settingsLoaded = true;
  });
}

async function loadRepoSettings(): Promise<void> {
  settingsLoading.value = true;
  try {
    settingsView.value = await api.settings(windowRepoId.value);
    applyScope();
  } catch (error) {
    report(error);
  } finally {
    settingsLoading.value = false;
  }
}

async function saveRepoSettings(): Promise<void> {
  if (!settingsLoaded) return;
  try {
    settingsView.value = await api.saveSettings(
      settingsScope.value,
      windowRepoId.value,
      { ...repoDraft.value },
    );
    if (repo.activeId) await repo.loadSettings();
    await notifySettingsChanged(windowRepoId.value);
  } catch (error) {
    report(error);
  }
}

const proxies = computed(() => settingsView.value?.proxies ?? []);
const proxyChoice = computed({
  get: () => repoDraft.value.proxyId ?? "direct",
  set: (value: string) => {
    repoDraft.value.proxyId = value === "direct" ? null : value;
  },
});
const proxyOptions = computed(() => [
  { label: t("settings.repo.direct"), value: "direct" },
  ...proxies.value.map((proxy) => ({ label: `${proxy.name} · ${proxy.url}`, value: proxy.id })),
]);
const globalProxyChoice = computed({
  get: () => settingsView.value?.global.proxyId ?? "direct",
  set: (value: string) => void setGlobalProxy(value === "direct" ? null : value),
});

/** 全局默认代理在独立的「代理」分区维护，不影响仓库行为草稿。 */
async function setGlobalProxy(proxyId: string | null): Promise<void> {
  const view = settingsView.value;
  if (!view) return;
  try {
    settingsView.value = await api.saveSettings("global", null, { ...view.global, proxyId });
    if (settingsScope.value === "global") {
      settingsLoaded = false;
      repoDraft.value = { ...settingsView.value.global };
      void nextTick(() => {
        settingsLoaded = true;
      });
    }
    if (repo.activeId) await repo.loadSettings();
    await notifySettingsChanged(windowRepoId.value);
  } catch (error) {
    report(error);
  }
}

const proxyProtocols = ["http", "https", "socks5", "socks5h"];
const proxyProtocolOptions = proxyProtocols.map((value) => ({ label: value, value }));
const proxyOpen = ref(false);
const proxySaving = ref(false);
const proxyEditing = ref<ProxyEntry | null>(null);
const proxyForm = ref({ name: "", protocol: "http", host: "", port: null as number | null, auth: "" });

function splitProxyUrl(raw: string): { protocol: string; host: string; port: number | null; auth: string } {
  try {
    const url = new URL(raw);
    return {
      protocol: url.protocol.replace(":", ""),
      host: url.hostname,
      port: url.port ? Number(url.port) : null,
      auth: url.username ? `${url.username}${url.password ? `:${url.password}` : ""}@` : "",
    };
  } catch {
    return { protocol: "http", host: raw, port: null, auth: "" };
  }
}

function openProxy(entry?: ProxyEntry): void {
  proxyEditing.value = entry ?? null;
  const parsed = splitProxyUrl(entry?.url ?? "");
  proxyForm.value = { name: entry?.name ?? "", ...parsed };
  proxyOpen.value = true;
}

async function saveProxy(): Promise<void> {
  const { name, protocol, host, port, auth } = proxyForm.value;
  if (!name.trim() || !host.trim()) {
    message.warning(t("settings.proxy.required"));
    return;
  }
  const input: ProxyInput = {
    id: proxyEditing.value?.id ?? null,
    name: name.trim(),
    url: `${protocol}://${auth}${host.trim()}${port ? `:${port}` : ""}`,
  };
  proxySaving.value = true;
  try {
    const list = await api.saveProxy(input);
    if (settingsView.value) {
      settingsView.value = { ...settingsView.value, proxies: list };
    }
    proxyOpen.value = false;
    message.success(t("settings.proxy.saved"));
    await notifySettingsChanged(windowRepoId.value);
  } catch (error) {
    report(error);
  } finally {
    proxySaving.value = false;
  }
}

async function removeProxy(entry: ProxyEntry): Promise<void> {
  try {
    await api.deleteProxy(entry.id);
    await loadRepoSettings();
    message.success(t("settings.proxy.deleted"));
    await notifySettingsChanged(windowRepoId.value);
  } catch (error) {
    report(error);
  }
}

async function cleanupNow(): Promise<void> {
  if (!windowRepoId.value) {
    message.warning(t("settings.repo.cleanupNoRepo"));
    return;
  }
  try {
    const count = await api.cleanupBackups(windowRepoId.value);
    message.success(
      count ? t("settings.repo.cleanupDone", { n: count }) : t("settings.repo.cleanupNone"),
    );
  } catch (error) {
    report(error);
  }
}

const overriddenLabels = computed(() => {
  if (settingsScope.value !== "repo" || !settingsView.value?.repo) return [];
  const global = settingsView.value.global;
  const draft = repoDraft.value;
  const labels: string[] = [];
  if (draft.pullStrategy !== global.pullStrategy) labels.push(t("settings.repo.pullStrategy"));
  if (draft.protectedBranches !== global.protectedBranches) labels.push(t("settings.repo.protected"));
  if (draft.pushAudit !== global.pushAudit) labels.push(t("settings.repo.pushAudit"));
  if (draft.secretScan !== global.secretScan) labels.push(t("settings.repo.secretScan"));
  if (draft.proxyId !== global.proxyId) labels.push(t("settings.repo.currentProxy"));
  if (draft.autoFetch !== global.autoFetch) labels.push(t("settings.repo.autoFetch"));
  if (draft.autoFetchMinutes !== global.autoFetchMinutes) labels.push(t("settings.repo.interval"));
  if (draft.backupRetentionDays !== global.backupRetentionDays) labels.push(t("settings.repo.backupRetention"));
  return labels;
});

function isOverridden(key: keyof RepoSettings): boolean {
  if (settingsScope.value !== "repo" || !settingsView.value?.repo) return false;
  return JSON.stringify(repoDraft.value[key]) !== JSON.stringify(settingsView.value.global[key]);
}


watch(
  repoDraft,
  () => {
    if (!settingsLoaded) return;
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(() => void saveRepoSettings(), 400);
  },
  { deep: true },
);

watch(settingsScope, applyScope);

watch(
  () => repo.activeId,
  () => {
    if (!repo.activeId && !windowRepoId.value) settingsScope.value = "global";
    void loadRepoSettings();
  },
);

let unlistenUiSettings: (() => void) | undefined;

onBeforeUnmount(() => {
  if (saveTimer) clearTimeout(saveTimer);
  unlistenUiSettings?.();
});

const credentials = ref<CredentialEntry[]>([]);
const hasExpiredCredential = computed(() =>
  credentials.value.some((entry) => entry.status === "expired"),
);
const credOpen = ref(false);
const credSaving = ref(false);
const credEditing = ref<CredentialEntry | null>(null);
const credForm = ref<{
  url: string;
  mode: CredentialMode;
  username: string;
  password: string;
}>({ url: "", mode: "password", username: "", password: "" });
const credMessage = ref("");

async function loadCredentials(): Promise<void> {
  try {
    credentials.value = await api.credentials();
  } catch (error) {
    report(error);
  }
}

function openCredential(entry?: CredentialEntry): void {
  credEditing.value = entry ?? null;
  credForm.value = {
    url: entry?.url ?? "",
    mode: entry?.mode ?? "password",
    username: entry?.username ?? "",
    password: "",
  };
  credMessage.value = "";
  credOpen.value = true;
}

/** 从问题抽屉深链到具体分区/凭证；同一 query 只应用一次。 */
let appliedDeepLink = "";
function applyDeepLink(): void {
  const section = typeof route.query.section === "string" ? route.query.section : "";
  const credentialId = typeof route.query.credential === "string" ? route.query.credential : "";
  const focus = typeof route.query.focus === "string" ? route.query.focus : "";
  const key = `${section}:${credentialId}:${focus}`;
  if (key === appliedDeepLink) return;
  const entry = credentialId
    ? credentials.value.find((item) => item.id === credentialId)
    : undefined;
  if (credentialId && !entry) return;
  if (section && sections.value.some((item) => item.key === section)) {
    activeSection.value = section as SectionKey;
  }
  if (entry) openCredential(entry);
  appliedDeepLink = key;
}

watch(
  () => [route.query.section, route.query.credential, route.query.focus],
  () => applyDeepLink(),
);

async function saveCredential(): Promise<void> {
  const form = credForm.value;
  if (!form.url.trim() || !form.username.trim()) {
    message.warning(t("settings.cred.required"));
    return;
  }
  if (!credEditing.value && !form.password) {
    message.warning(t("settings.cred.passwordRequired"));
    return;
  }
  const input: CredentialInput = {
    id: credEditing.value?.id ?? null,
    url: form.url.trim(),
    mode: form.mode,
    username: form.username.trim(),
    password: form.password ? form.password : null,
  };
  credSaving.value = true;
  try {
    await api.saveCredential(input);
    await loadCredentials();
    credOpen.value = false;
    message.success(t("settings.cred.saved"));
  } catch (error) {
    report(error);
  } finally {
    credSaving.value = false;
  }
}

async function removeCredential(entry: CredentialEntry): Promise<void> {
  try {
    await api.deleteCredential(entry.id);
    await loadCredentials();
    message.success(t("settings.cred.deleted"));
  } catch (error) {
    report(error);
  }
}

async function validateCredential(entry: CredentialEntry): Promise<void> {
  if (!windowRepoId.value) {
    message.warning(t("settings.cred.validateNoRepo"));
    return;
  }
  credMessage.value = t("settings.cred.validating");
  try {
    const result = await api.validateCredential(windowRepoId.value, entry.id);
    await loadCredentials();
    credMessage.value = result.message;
    if (result.ok) message.success(result.message);
    else message.error(result.message, { duration: 7000 });
  } catch (error) {
    credMessage.value = "";
    report(error);
  }
}

function credentialStatus(entry: CredentialEntry): string {
  if (entry.status === "ok") return t("settings.cred.normal");
  if (entry.status === "expired") return t("settings.cred.expired");
  return t("settings.cred.unverified");
}

function credentialStatusClass(entry: CredentialEntry): string {
  if (entry.status === "ok") return "b-green";
  if (entry.status === "expired") return "b-red";
  return "plain";
}

function lastUsedText(entry: CredentialEntry): string {
  if (!entry.lastUsed) return t("settings.cred.never");
  const seconds = Math.max(0, Math.floor(Date.now() / 1000) - entry.lastUsed);
  if (seconds < 60) return t("time.justNow");
  if (seconds < 3600) return t("time.minutesAgo", { n: Math.floor(seconds / 60) });
  if (seconds < 86400) return t("time.hoursAgo", { n: Math.floor(seconds / 3600) });
  return t("time.daysAgo", { n: Math.floor(seconds / 86400) });
}

async function loadInfo(): Promise<void> {
  try {
    info.value = await api.info();
    infoError.value = "";
  } catch (error) {
    infoError.value = errorText(error);
  }
}

onMounted(async () => {
  if (route.query.repo) settingsScope.value = "repo";
  await Promise.all([loadInfo(), loadRepoSettings(), loadCredentials()]);
  applyDeepLink();
  try {
    unlistenUiSettings = await onUiSettingsChanged(() => ui.reloadSettings());
  } catch {
    // 浏览器预览没有跨窗口事件。
  }
});

function close(): void {
  if (standalone.value && isTauri) {
    void closeSettingsWindow();
    return;
  }
  void router.push(repo.activeId ? "/workspace" : "/dashboard");
}

async function reset(): Promise<void> {
  ui.resetSettings();
  try {
    settingsView.value = await api.resetRepoSettings();
    settingsScope.value = "global";
    applyScope();
    await notifySettingsChanged(windowRepoId.value);
    message.success(t("settings.resetDone"));
  } catch (error) {
    report(error);
  }
}

async function resetSection(): Promise<void> {
  if (activeSection.value === "appearance") {
    ui.settings.theme = "system";
    ui.settings.language = "zh";
    message.success(t("settings.appearance.resetDone"));
    return;
  }
  if (activeSection.value === "workspace") {
    ui.settings.fileView = "list";
    ui.settings.diffMode = "side";
    ui.settings.refreshInterval = 5000;
    message.success(t("settings.workspace.resetDone"));
    return;
  }
  if (activeSection.value === "repo") {
    try {
      if (settingsScope.value === "global") {
        settingsView.value = await api.saveSettings("global", null, DEFAULT_REPO_SETTINGS);
      } else if (settingsView.value) {
        repoDraft.value = { ...settingsView.value.global };
        await saveRepoSettings();
      }
      applyScope();
      message.success(t("settings.repo.resetDone"));
    } catch (error) {
      report(error);
    }
    return;
  }
  message.info(t("settings.noDefaults"));
}
</script>

<template>
  <div class="settings-page">
    <nav class="settings-nav" :aria-label="t('nav.settings')">
      <div class="settings-title">{{ t("settings.title") }}</div>
      <n-input
        v-model:value="settingsSearch"
        size="small"
        clearable
        :placeholder="t('settings.search')"
        class="settings-search"
      />
      <button
        v-for="section in filteredSections"
        :key="section.key"
        class="settings-navitem"
        :class="{ active: activeSection === section.key }"
        @click="activeSection = section.key"
      >
        <span>{{ section.label }}</span>
        <span class="hint">{{ section.hint }}</span>
      </button>
      <div v-if="!filteredSections.length" class="settings-navfoot">{{ t("settings.noMatch") }}</div>
      <div class="settings-navfoot">
        {{ t("settings.navFoot") }}
      </div>
    </nav>

    <section class="settings-main">
      <header class="settings-head">
        <div>
          <h2>{{ currentSection.label }}</h2>
          <p>{{ currentSection.hint }}</p>
        </div>
        <span class="grow"></span>
        <n-button size="small" @click="resetSection">{{ t("settings.restoreSection") }}</n-button>
        <n-button size="small" @click="close">{{ t("common.done") }}</n-button>
      </header>

      <div class="settings-body">
        <template v-if="activeSection === 'appearance'">
          <div class="settings-field">
            <label>{{ t("settings.appearance.theme") }}</label>
            <n-radio-group v-model:value="ui.settings.theme" class="settings-radios">
              <n-radio v-for="theme in themes" :key="theme.value" :value="theme.value">
                {{ theme.label }}<span class="radio-hint">{{ theme.hint }}</span>
              </n-radio>
            </n-radio-group>
            <div class="affect">
              {{ t("settings.appearance.themeAffect", { hint: themeHint }) }}
            </div>
          </div>

          <div class="settings-field">
            <label>{{ t("settings.appearance.language") }}</label>
            <n-radio-group v-model:value="ui.settings.language" class="settings-radios">
              <n-radio v-for="item in LANGUAGES" :key="item.value" :value="item.value">
                {{ item.label }}
              </n-radio>
            </n-radio-group>
            <div class="affect">
              {{ t("settings.appearance.languageAffect") }}
            </div>
          </div>
        </template>

        <template v-else-if="activeSection === 'workspace'">
          <div class="settings-field">
            <label>{{ t("settings.workspace.fileView") }}</label>
            <n-radio-group v-model:value="ui.settings.fileView" class="settings-radios">
              <n-radio value="list">{{ t("settings.workspace.fileViewList") }}<span class="radio-hint">{{ t("settings.workspace.fileViewListHint") }}</span></n-radio>
              <n-radio value="tree">{{ t("settings.workspace.fileViewTree") }}<span class="radio-hint">{{ t("settings.workspace.fileViewTreeHint") }}</span></n-radio>
            </n-radio-group>
            <div class="affect">
              {{ t("settings.workspace.fileViewAffect") }}
            </div>
          </div>

          <div class="settings-field">
            <label>{{ t("settings.workspace.diffMode") }}</label>
            <n-radio-group v-model:value="ui.settings.diffMode" class="settings-radios">
              <n-radio value="side">{{ t("settings.workspace.diffSide") }}<span class="radio-hint">{{ t("settings.workspace.diffSideHint") }}</span></n-radio>
              <n-radio value="unified">{{ t("settings.workspace.diffUnified") }}<span class="radio-hint">{{ t("settings.workspace.diffUnifiedHint") }}</span></n-radio>
            </n-radio-group>
            <div class="affect">{{ t("settings.workspace.diffAffect") }}</div>
          </div>

          <div class="settings-field">
            <label>{{ t("settings.workspace.refresh") }}</label>
            <n-select
              v-model:value="ui.settings.refreshInterval"
              :options="refreshOptions"
              style="width: 220px"
            />
            <div class="affect">
              {{ t("settings.workspace.refreshAffect") }}
            </div>
          </div>
        </template>

        <template v-else-if="activeSection === 'repo'">
          <div class="settings-field">
            <label>{{ t("settings.repo.scope") }}</label>
            <n-radio-group v-model:value="settingsScope" class="settings-radios">
              <n-radio value="global">{{ t("settings.repo.scopeGlobal") }}<span class="radio-hint">{{ t("settings.repo.scopeGlobalHint") }}</span></n-radio>
              <n-radio value="repo" :disabled="!windowRepoId">
                {{ t("settings.repo.scopeRepo") }}<span class="radio-hint">
                  {{ windowRepoId ? t("settings.repo.scopeRepoHint") : t("settings.repo.scopeRepoDisabled") }}
                </span>
              </n-radio>
            </n-radio-group>
            <div class="affect">
              {{ t("settings.repo.scopeAffect") }}
              <span v-if="settingsScope === 'repo' && settingsView && !settingsView.repo">
                {{ t("settings.repo.scopeRepoNew") }}
              </span>
            </div>
            <div v-if="overriddenLabels.length" class="override-summary">
              {{ t("settings.repo.overrides", { items: overriddenLabels.join(" · ") }) }}
            </div>
          </div>

          <div v-if="settingsLoading" class="faint small">{{ t("common.loading") }}</div>

          <template v-else>
            <div v-if="settingsScope === 'repo'" class="settings-field">
              <label>
                {{ t("settings.repo.currentProxy") }}
                <span v-if="isOverridden('proxyId')" class="badge plain">{{ t("settings.repo.overridden") }}</span>
              </label>
              <n-select v-model:value="proxyChoice" :options="proxyOptions" style="width: 380px" />
              <div class="affect">
                {{ t("settings.repo.proxyAffect") }}
              </div>
            </div>
            <div v-else class="settings-field">
              <label>{{ t("settings.repo.globalProxy") }}</label>
              <div class="affect">{{ t("settings.repo.proxyInProxySection") }}</div>
            </div>

            <div class="settings-field">
              <label>
                {{ t("settings.repo.pullStrategy") }}
                <span v-if="isOverridden('pullStrategy')" class="badge plain">{{ t("settings.repo.overridden") }}</span>
              </label>
              <n-radio-group v-model:value="repoDraft.pullStrategy" class="settings-radios">
                <n-radio value="merge">
                  merge<span class="radio-hint">{{ t("settings.repo.pullMergeHint") }}</span>
                </n-radio>
                <n-radio value="rebase">
                  rebase<span class="radio-hint">{{ t("settings.repo.pullRebaseHint") }}</span>
                </n-radio>
                <n-radio value="ffOnly">
                  ff-only<span class="radio-hint">{{ t("settings.repo.pullFfHint") }}</span>
                </n-radio>
              </n-radio-group>
              <div class="affect">
                {{ t("settings.repo.pullAffect") }}
              </div>
            </div>

            <div class="settings-field">
              <label>
                {{ t("settings.repo.protected") }}
                <span v-if="isOverridden('protectedBranches')" class="badge plain">{{ t("settings.repo.overridden") }}</span>
              </label>
              <n-input
                v-model:value="repoDraft.protectedBranches"
                placeholder="^(main|master|develop|release/.*)$"
              />
              <div class="affect">
                {{ t("settings.repo.protectedAffect") }}
              </div>
            </div>

            <div class="settings-field">
              <label>
                {{ t("settings.repo.pushAudit") }}
                <span v-if="isOverridden('pushAudit')" class="badge plain">{{ t("settings.repo.overridden") }}</span>
              </label>
              <n-checkbox v-model:checked="repoDraft.pushAudit">
                {{ t("settings.repo.pushAuditToggle") }}
              </n-checkbox>
              <div class="affect">
                {{ t("settings.repo.pushAuditAffect") }}
              </div>
            </div>

            <div class="settings-field">
              <label>
                {{ t("settings.repo.secretScan") }}
                <span v-if="isOverridden('secretScan')" class="badge plain">{{ t("settings.repo.overridden") }}</span>
              </label>
              <n-checkbox v-model:checked="repoDraft.secretScan">
                {{ t("settings.repo.secretScanToggle") }}
              </n-checkbox>
              <div class="affect">
                {{ t("settings.repo.secretScanAffect") }}
              </div>
            </div>

            <div class="settings-field">
              <label>
                {{ t("settings.repo.autoFetch") }}
                <span v-if="isOverridden('autoFetch')" class="badge plain">{{ t("settings.repo.overridden") }}</span>
              </label>
              <n-checkbox v-model:checked="repoDraft.autoFetch">
                {{ t("settings.repo.autoFetchToggle") }}
              </n-checkbox>
              <div v-if="repoDraft.autoFetch" class="row mt8">
                <span class="faint small">{{ t("settings.repo.interval") }}</span>
                <n-select
                  v-model:value="repoDraft.autoFetchMinutes"
                  :options="autoFetchOptions"
                  style="width: 160px"
                />
              </div>
              <div class="affect">
                {{ t("settings.repo.autoFetchAffect") }}
              </div>
            </div>

            <div class="settings-field">
              <label>
                {{ t("settings.repo.backupRetention") }}
                <span v-if="isOverridden('backupRetentionDays')" class="badge plain">{{ t("settings.repo.overridden") }}</span>
              </label>
              <div class="row">
                <n-input-number
                  v-model:value="repoDraft.backupRetentionDays"
                  :min="0"
                  :max="3650"
                  style="width: 140px"
                />
                <span class="faint small">{{ t("settings.repo.days") }}</span>
                <span class="grow"></span>
                <n-button size="small" :disabled="!windowRepoId" @click="cleanupNow">
                  {{ t("settings.repo.cleanupNow") }}
                </n-button>
              </div>
              <div class="affect">
                {{ t("settings.repo.backupAffect") }}
              </div>
            </div>
          </template>
        </template>

        <template v-else-if="activeSection === 'proxy'">
          <div class="settings-field">
            <label>{{ t("settings.proxy.default") }}</label>
            <n-select v-model:value="globalProxyChoice" :options="proxyOptions" style="width: 380px" />
            <div class="affect">
              {{ t("settings.proxy.defaultAffect") }}
            </div>
          </div>

          <div class="settings-field">
            <div class="cred-head">
              <label>{{ t("settings.repo.proxies") }}</label>
              <span class="grow"></span>
              <n-button size="small" @click="openProxy()">{{ t("settings.repo.addProxy") }}</n-button>
            </div>
            <div class="faint small">
              {{ t("settings.repo.proxiesHint") }}
            </div>
            <div v-if="!proxies.length" class="note">
              {{ t("settings.repo.noProxies") }}
            </div>
            <div v-else class="cred-scroll">
              <table class="cred-table">
                <thead>
                  <tr>
                    <th>{{ t("settings.repo.proxyName") }}</th>
                    <th>{{ t("settings.repo.proxyUrl") }}</th>
                    <th></th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="proxy in proxies" :key="proxy.id">
                    <td>{{ proxy.name }}</td>
                    <td class="mono">{{ proxy.url }}</td>
                    <td class="cred-actions">
                      <n-button size="tiny" quaternary @click="openProxy(proxy)">{{ t("common.edit") }}</n-button>
                      <n-popconfirm
                        :positive-text="t('common.delete')"
                        :negative-text="t('common.cancel')"
                        @positive-click="removeProxy(proxy)"
                      >
                        <template #trigger>
                          <n-button size="tiny" quaternary type="error">{{ t("common.delete") }}</n-button>
                        </template>
                        {{ t("settings.proxy.deleteConfirm", { name: proxy.name }) }}
                      </n-popconfirm>
                    </td>
                  </tr>
                </tbody>
              </table>
            </div>
          </div>
        </template>

        <template v-else-if="activeSection === 'credentials'">
          <div class="settings-field">
            <div class="cred-head">
              <label>{{ t("settings.cred.title") }}</label>
              <span class="grow"></span>
              <n-button size="small" @click="openCredential()">{{ t("settings.cred.add") }}</n-button>
            </div>
            <div class="faint small">
              {{ t("settings.cred.intro") }}
            </div>
            <div v-if="hasExpiredCredential" class="note">
              {{ t("settings.cred.expiredHint") }}
            </div>
            <div v-if="!credentials.length" class="note">
              {{ t("settings.cred.empty") }}
            </div>
            <div v-else class="cred-scroll">
              <table class="cred-table">
                <thead>
                  <tr>
                    <th>{{ t("settings.cred.url") }}</th>
                    <th>{{ t("settings.cred.mode") }}</th>
                    <th>{{ t("settings.cred.username") }}</th>
                    <th>{{ t("settings.cred.lastUsed") }}</th>
                    <th>{{ t("settings.cred.status") }}</th>
                    <th></th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="entry in credentials" :key="entry.id">
                    <td class="mono">{{ entry.url }}</td>
                    <td>{{ entry.mode === "token" ? t("settings.cred.modeToken") : t("settings.cred.modePassword") }}</td>
                    <td class="mono">{{ entry.username }}</td>
                    <td class="faint small">{{ lastUsedText(entry) }}</td>
                    <td>
                      <span class="badge" :class="credentialStatusClass(entry)">
                        {{ credentialStatus(entry) }}
                      </span>
                    </td>
                    <td class="cred-actions">
                      <n-button size="tiny" quaternary @click="openCredential(entry)">{{ t("common.edit") }}</n-button>
                      <n-button
                        size="tiny"
                        quaternary
                        :disabled="!windowRepoId"
                        @click="validateCredential(entry)"
                      >
                        {{ t("settings.cred.validate") }}
                      </n-button>
                      <n-popconfirm
                        :positive-text="t('common.delete')"
                        :negative-text="t('common.cancel')"
                        @positive-click="removeCredential(entry)"
                      >
                        <template #trigger>
                          <n-button size="tiny" quaternary type="error">{{ t("common.delete") }}</n-button>
                        </template>
                        {{ t("settings.cred.deleteConfirm", { url: entry.url }) }}
                      </n-popconfirm>
                    </td>
                  </tr>
                </tbody>
              </table>
            </div>
            <div v-if="credMessage" class="faint small">{{ credMessage }}</div>
            <div class="affect">
              {{ t("settings.cred.matching") }}
            </div>
          </div>
        </template>

        <template v-else-if="activeSection === 'shortcuts'">
          <div class="settings-field">
            <label>{{ t("settings.shortcuts.title") }}</label>
            <table class="cred-table">
              <tbody>
                <tr><td>Ctrl+,</td><td>{{ t("settings.shortcuts.settings") }}</td></tr>
                <tr><td>Ctrl+Enter</td><td>{{ t("settings.shortcuts.commit") }}</td></tr>
              </tbody>
            </table>
            <div class="affect">
              {{ t("settings.shortcuts.note") }}
            </div>
          </div>
        </template>

        <template v-else-if="activeSection === 'gitconfig'">
          <div class="settings-field">
            <label>{{ t("settings.gitconfig.title") }}</label>
            <div class="note">
              {{ t("settings.gitconfig.note") }}
            </div>
            <div class="affect">{{ t("settings.gitconfig.affect") }}</div>
          </div>
        </template>

        <template v-else-if="activeSection === 'external'">
          <div class="settings-field">
            <label>{{ t("settings.external.title") }}</label>
            <div class="note">
              {{ t("settings.external.note") }}
            </div>
            <div class="affect">{{ t("settings.external.affect") }}</div>
          </div>
        </template>

        <template v-else-if="activeSection === 'ai'">
          <div class="settings-field">
            <label>{{ t("settings.ai.title") }}</label>
            <div class="note">
              {{ t("settings.ai.note") }}
            </div>
            <div class="affect">{{ t("settings.ai.affect") }}</div>
          </div>
        </template>

        <template v-else-if="activeSection === 'advanced'">
          <div class="settings-field">
            <label>{{ t("settings.advanced.title") }}</label>
            <div class="note">{{ t("settings.advanced.note") }}</div>
            <div class="affect">{{ t("settings.advanced.affect") }}</div>
          </div>
        </template>

        <template v-else>
          <dl class="settings-kv">
            <dt>Git4U</dt>
            <dd>{{ info?.appVersion ?? t("common.loading") }}</dd>
            <dt>Git</dt>
            <dd>{{ info?.gitVersion ?? t("common.loading") }}</dd>
            <dt>{{ t("settings.about.engine") }}</dt>
            <dd>{{ info?.engine ?? t("common.loading") }}</dd>
            <dt>{{ t("settings.about.dataDir") }}</dt>
            <dd class="mono">{{ info?.appDataDir ?? t("common.loading") }}</dd>
          </dl>
          <div v-if="infoError" class="settings-error">{{ infoError }}</div>
          <div class="note mt12">
            {{ t("settings.about.note") }}
          </div>
          <div class="settings-actions">
            <n-button size="small" @click="loadInfo">{{ t("settings.about.reload") }}</n-button>
          </div>
        </template>
      </div>

      <footer class="settings-foot">
        <span class="faint small">{{ t("settings.footerNote") }}</span>
        <span class="grow"></span>
        <n-popconfirm
          :positive-text="t('settings.reset')"
          :negative-text="t('common.cancel')"
          @positive-click="reset"
        >
          <template #trigger>
            <n-button size="small">{{ t("settings.reset") }}</n-button>
          </template>
          {{ t("settings.resetConfirm") }}
        </n-popconfirm>
        <n-button size="small" type="primary" @click="close">{{ t("common.done") }}</n-button>
      </footer>
    </section>

    <n-modal
      v-model:show="credOpen"
      preset="card"
      :title="credEditing ? t('settings.cred.titleEdit') : t('settings.cred.titleNew')"
      style="width: min(560px, 92vw)"
    >
      <div class="cred-form">
        <label>{{ t("settings.cred.urlLabel") }}</label>
        <n-input v-model:value="credForm.url" :placeholder="t('settings.cred.urlPlaceholder')" />
        <div class="affect">
          {{ t("settings.cred.urlHint") }}
        </div>

        <label>{{ t("settings.cred.method") }}</label>
        <n-radio-group v-model:value="credForm.mode" class="settings-radios">
          <n-radio value="password">{{ t("settings.cred.modePassword") }}</n-radio>
          <n-radio value="token">{{ t("settings.cred.modeToken") }}</n-radio>
        </n-radio-group>

        <label>{{ t("settings.cred.username") }}</label>
        <n-input v-model:value="credForm.username" placeholder="zhangsan" />

        <label>{{ t("settings.cred.password") }}</label>
        <n-input
          v-model:value="credForm.password"
          type="password"
          show-password-on="click"
          :placeholder="credEditing ? t('settings.cred.passwordPlaceholderEdit') : t('settings.cred.passwordPlaceholderNew')"
        />
        <div class="faint small">
          {{ t("settings.cred.storage") }}
        </div>
      </div>
      <template #footer>
        <div class="cred-foot">
          <span class="faint small">{{ t("settings.cred.injectNote") }}</span>
          <span class="grow"></span>
          <n-button size="small" @click="credOpen = false">{{ t("common.cancel") }}</n-button>
          <n-button size="small" type="primary" :loading="credSaving" @click="saveCredential">
            {{ t("common.save") }}
          </n-button>
        </div>
      </template>
    </n-modal>

    <n-modal
      v-model:show="proxyOpen"
      preset="card"
      :title="proxyEditing ? t('settings.proxy.titleEdit') : t('settings.proxy.titleNew')"
      style="width: min(520px, 92vw)"
    >
      <div class="cred-form">
        <label>{{ t("settings.proxy.name") }}</label>
        <n-input v-model:value="proxyForm.name" :placeholder="t('settings.proxy.namePlaceholder')" />
        <label>{{ t("settings.proxy.protocol") }}</label>
        <n-select v-model:value="proxyForm.protocol" :options="proxyProtocolOptions" style="width: 160px" />
        <label>{{ t("settings.proxy.host") }}</label>
        <n-input v-model:value="proxyForm.host" :placeholder="t('settings.proxy.hostPlaceholder')" />
        <label>{{ t("settings.proxy.port") }}</label>
        <n-input-number
          v-model:value="proxyForm.port"
          :min="1"
          :max="65535"
          :placeholder="t('settings.proxy.portPlaceholder')"
          style="width: 160px"
        />
        <div class="affect">
          {{ t("settings.proxy.urlHint") }}
        </div>
      </div>
      <template #footer>
        <div class="cred-foot">
          <span class="grow"></span>
          <n-button size="small" @click="proxyOpen = false">{{ t("common.cancel") }}</n-button>
          <n-button size="small" type="primary" :loading="proxySaving" @click="saveProxy">
            {{ t("common.save") }}
          </n-button>
        </div>
      </template>
    </n-modal>
  </div>
</template>

<style scoped>
.settings-page {
  display: flex;
  flex: 1;
  min-height: 0;
  min-width: 0;
  background: var(--bg);
}
.settings-nav {
  width: 220px;
  flex: none;
  background: var(--bg-sunken);
  border-right: 1px solid var(--border);
  padding: 14px 10px;
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-height: 0;
  overflow: auto;
}
.settings-title {
  font-weight: 600;
  font-size: 13.5px;
  padding: 2px 10px 12px;
}
.settings-search {
  margin: 0 6px 8px;
}
.settings-navitem {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 8px 10px;
  border: none;
  border-radius: 3px;
  background: none;
  color: var(--text-muted);
  text-align: left;
  cursor: pointer;
  font-family: inherit;
  font-size: 12.5px;
}
.settings-navitem:hover {
  background: var(--bg-hover);
  color: var(--text);
}
.settings-navitem.active {
  background: var(--accent-dim);
  color: var(--primary);
}
.settings-navitem .hint {
  font-size: 11px;
  color: var(--text-faint);
}
.settings-navitem.active .hint {
  color: var(--text-muted);
}
.settings-navfoot {
  margin-top: auto;
  padding: 12px 10px 0;
  font-size: 11.5px;
  line-height: 1.6;
  color: var(--text-faint);
}
.settings-main {
  flex: 1;
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
}
.settings-head {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 14px 24px;
  border-bottom: 1px solid var(--border);
  flex: none;
}
.settings-head h2 {
  margin: 0;
  font-size: 15px;
}
.settings-head p {
  margin: 3px 0 0;
  font-size: 12px;
  color: var(--text-muted);
}
.settings-body {
  flex: 1;
  overflow: auto;
  padding: 18px 24px 28px;
  max-width: 860px;
}
.settings-field {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-bottom: 24px;
}
.settings-field > label {
  font-size: 12.5px;
  color: var(--text);
  font-weight: 500;
}
.settings-radios {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 6px;
}
.radio-hint {
  margin-left: 8px;
  font-size: 11.5px;
  color: var(--text-faint);
}
.affect {
  font-size: 11.5px;
  color: var(--text-faint);
  line-height: 1.6;
  border-top: 1px dashed var(--border);
  padding-top: 6px;
}
.settings-kv {
  display: grid;
  grid-template-columns: 130px 1fr;
  gap: 8px 16px;
  font-size: 12.5px;
}
.settings-kv dt {
  color: var(--text-faint);
}
.settings-kv dd {
  margin: 0;
  overflow-wrap: anywhere;
}
.settings-error {
  margin-top: 10px;
  font-size: 12px;
  color: var(--error);
  white-space: pre-wrap;
}
.settings-actions {
  margin-top: 12px;
}
.settings-foot {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 24px;
  border-top: 1px solid var(--border);
  background: var(--bg-panel);
  flex: none;
}
.override-summary {
  font-size: 11.5px;
  color: var(--primary);
  border-top: 1px dashed var(--border);
  padding-top: 6px;
}
.cred-head {
  display: flex;
  align-items: center;
  gap: 8px;
}
.cred-scroll {
  overflow: auto;
}
.cred-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 12.5px;
}
.cred-table th,
.cred-table td {
  padding: 7px 8px;
  border-bottom: 1px solid var(--border);
  text-align: left;
  vertical-align: middle;
}
.cred-table th {
  color: var(--text-faint);
  font-weight: 500;
  font-size: 11.5px;
}
.cred-actions {
  white-space: nowrap;
  text-align: right;
}
.cred-form {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.cred-form > label {
  font-size: 12.5px;
  color: var(--text);
  font-weight: 500;
}
.cred-foot {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
}
</style>
