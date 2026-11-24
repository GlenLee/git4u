<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { NButton, NModal, NPopconfirm, useMessage } from "naive-ui";
import DashboardView from "@/views/DashboardView.vue";
import { isTauri, onSettingsChanged, onUiSettingsChanged, openSettingsWindow } from "@/ipc";
import type { PushAuditReport } from "@/ipc/types";
import { midStateLabel, t } from "@/lib/i18n";
import { openProblems, problemCount, useProblemReporter } from "@/lib/problems";
import { useRepoStore, useUiStore } from "@/stores/repo";

const repo = useRepoStore();
const ui = useUiStore();
const route = useRoute();
const router = useRouter();
const message = useMessage();
const report = useProblemReporter();

const isSettings = computed(() => route.path === "/settings");
const showDashboard = computed(
  () => !isSettings.value && (!repo.activeId || route.path === "/dashboard"),
);
const workspaceCount = computed(() => repo.stagedCount + repo.unstagedCount);
const branchCount = computed(() => repo.localBranches.length);
const rescueLabel = computed(() => {
  if (problemCount.value) return t("problem.count", { n: problemCount.value });
  if (repo.status?.midState) return midStateLabel(repo.status.midState);
  return t("app.noActiveScene");
});

let timer: ReturnType<typeof setInterval> | undefined;
let autoFetchTimer: ReturnType<typeof setInterval> | undefined;
let settingsTimer: ReturnType<typeof setInterval> | undefined;
let unlistenSettings: (() => void) | undefined;
let unlistenUiSettings: (() => void) | undefined;

const syncBusy = ref(false);
const auditOpen = ref(false);
const auditReport = ref<PushAuditReport | null>(null);
const auditHasBlockers = computed(() => (auditReport.value?.blockers.length ?? 0) > 0);


async function syncAction(label: string, action: () => Promise<void>): Promise<boolean> {
  if (syncBusy.value) return false;
  syncBusy.value = true;
  try {
    await action();
    message.success(label);
    return true;
  } catch (error) {
    report(error);
    return false;
  } finally {
    syncBusy.value = false;
  }
}

async function doFetch(): Promise<void> {
  await syncAction(t("app.fetchDone"), () => repo.fetch());
}

async function doPull(): Promise<void> {
  await syncAction(t("app.pullDone"), () => repo.pull());
}

async function doPush(): Promise<void> {
  if (syncBusy.value) return;
  syncBusy.value = true;
  try {
    const report = await repo.pushAudit();
    auditReport.value = report;
    if (!report.blockers.length && !report.warnings.length) {
      await repo.push(false);
      message.success(t("app.pushed"));
    } else {
      auditOpen.value = true;
    }
  } catch (error) {
    report(error);
  } finally {
    syncBusy.value = false;
  }
}

async function confirmPush(): Promise<void> {
  const ok = await syncAction(t("app.pushed"), () => repo.push(true));
  if (ok) auditOpen.value = false;
}

function startPolling(): void {
  if (timer) clearInterval(timer);
  timer = undefined;
  const interval = ui.settings.refreshInterval;
  if (interval <= 0) return;
  timer = setInterval(() => {
    if (document.visibilityState === "visible" && repo.activeId) {
      void repo.refreshStatus();
    }
  }, interval);
}

async function openSettings(): Promise<void> {
  try {
    const opened = await openSettingsWindow(repo.activeId);
    if (!opened) void router.push("/settings");
  } catch (error) {
    report(error);
  }
}

function onKeydown(event: KeyboardEvent): void {
  if ((event.ctrlKey || event.metaKey) && event.key === ",") {
    event.preventDefault();
    void openSettings();
  }
}

function startAutoFetch(): void {
  if (autoFetchTimer) clearInterval(autoFetchTimer);
  autoFetchTimer = undefined;
  const current = repo.settings?.effective;
  if (!current?.autoFetch) return;
  const interval = Math.max(1, current.autoFetchMinutes) * 60_000;
  autoFetchTimer = setInterval(() => {
    if (document.visibilityState !== "visible" || !repo.activeId || syncBusy.value) return;
    void repo.fetch().catch(() => {
      // 后台 Fetch 失败保持静默；下次手动操作会展示错误。
    });
  }, interval);
}

watch(
  () => [
    repo.settings?.effective.autoFetch,
    repo.settings?.effective.autoFetchMinutes,
  ],
  startAutoFetch,
  { immediate: true },
);

watch(() => ui.settings.refreshInterval, startPolling);

// 后台/自动刷新失败也要进问题抽屉，否则状态栏有红字但抽屉是空的。
watch(
  () => repo.lastError,
  (text) => {
    if (text) report(text);
  },
);

async function refresh(): Promise<void> {
  await repo.refreshAll(true);
}

onMounted(async () => {
  await repo.hydrate();
  startPolling();
  startAutoFetch();
  settingsTimer = setInterval(() => void repo.loadSettings(), 30_000);
  try {
    unlistenSettings = await onSettingsChanged(() => void repo.loadSettings());
  } catch {
    // 事件监听失败不影响 30s 轮询兜底。
  }
  try {
    unlistenUiSettings = await onUiSettingsChanged(() => ui.reloadSettings());
  } catch {
    // 浏览器预览没有跨窗口事件。
  }
  window.addEventListener("keydown", onKeydown);
});

onBeforeUnmount(() => {
  if (timer) clearInterval(timer);
  if (autoFetchTimer) clearInterval(autoFetchTimer);
  if (settingsTimer) clearInterval(settingsTimer);
  unlistenSettings?.();
  unlistenUiSettings?.();
  window.removeEventListener("keydown", onKeydown);
});

function dotClass(id: string): string {
  const summary = repo.repos.find((r) => r.id === id);
  if (!summary || !summary.valid) return "gray";
  if (summary.conflicts > 0) return "r";
  if (summary.midState) return "y";
  if (summary.staged + summary.unstaged > 0) return "y";
  return "g";
}

async function openRepo(): Promise<void> {
  try {
    const result = await repo.openRepo();
    if (result === "preview") {
      message.info(t("app.previewOnly"), {
        duration: 5000,
      });
    } else if (result === "opened") {
      message.success(t("app.repoOpened"));
      void router.push("/workspace");
    }
  } catch (error) {
    report(error);
  }
}

async function enterRepo(id: string): Promise<void> {
  await repo.activate(id);
  if (route.path === "/dashboard") void router.push("/workspace");
}

/** 没有活动仓库时，仓库页签不可用（仪表盘与仓库卡片仍可打开仓库）。 */
function guardNav(event: MouseEvent): void {
  if (!repo.activeId) event.preventDefault();
}

async function removeRepo(id: string): Promise<void> {
  try {
    await repo.removeRepo(id);
    message.success(t("app.repoRemoved"));
  } catch (error) {
    report(error);
  }
}
</script>

<template>
  <div class="app">
    <header class="topbar">
      <button class="brand" :title="t('nav.dashboardTitle')" @click="router.push('/dashboard')">Git4U</button>

      <template v-if="repo.activeRepo">
        <span class="badge b-blue" :title="repo.activeRepo.path">{{ repo.activeRepo.name }}</span>
        <span class="badge b-green">{{ repo.currentBranch }}</span>
        <span v-if="repo.activeRepo.ahead" class="badge b-blue">↑ {{ repo.activeRepo.ahead }}</span>
        <span v-if="repo.activeRepo.behind" class="badge b-purple">↓ {{ repo.activeRepo.behind }}</span>
        <span v-if="repo.status?.midState" class="badge b-yellow">⟳ {{ repo.status.midState }}</span>
      </template>
      <span v-else class="badge plain">{{ t("nav.dashboard") }}</span>

      <nav class="topnav">
        <router-link class="navitem" :class="{ active: showDashboard }" to="/dashboard">
          {{ t("nav.dashboard") }}
        </router-link>
        <router-link
          class="navitem"
          :class="{
            active: !showDashboard && route.path === '/workspace',
            disabled: !repo.activeId,
          }"
          to="/workspace"
          @click="guardNav"
        >
          {{ t("nav.workspace") }}<span class="n">{{ workspaceCount }}</span>
        </router-link>
        <router-link
          class="navitem"
          :class="{
            active: !showDashboard && route.path === '/tools',
            disabled: !repo.activeId,
          }"
          to="/tools"
          @click="guardNav"
          :title="t('nav.toolsTitle')"
        >
          {{ t("nav.tools") }}
        </router-link>
        <router-link
          class="navitem"
          :class="{
            active: !showDashboard && route.path === '/history',
            disabled: !repo.activeId,
          }"
          to="/history"
          @click="guardNav"
        >
          {{ t("nav.history") }}
        </router-link>
        <router-link
          class="navitem"
          :class="{
            active: !showDashboard && route.path === '/branches',
            disabled: !repo.activeId,
          }"
          to="/branches"
          @click="guardNav"
        >
          {{ t("nav.branches") }}<span class="n">{{ branchCount }}</span>
        </router-link>
        <button
          type="button"
          class="navitem"
          :class="{ active: isSettings }"
          :title="t('nav.settingsTitle')"
          @click="openSettings"
        >
          {{ t("nav.settings") }}
        </button>
      </nav>

      <span class="grow"></span>

      <n-button v-if="repo.activeRepo" size="small" quaternary :title="t('app.refreshTitle')" @click="refresh">
        ⟳ {{ t("common.refresh") }}
      </n-button>
      <template v-if="repo.activeRepo">
        <n-button size="small" quaternary :disabled="syncBusy" @click="doFetch">Fetch</n-button>
        <n-button size="small" quaternary :disabled="syncBusy" @click="doPull">Pull</n-button>
        <n-button size="small" quaternary :disabled="syncBusy" @click="doPush">Push</n-button>
      </template>
      <n-button size="small" quaternary :title="ui.isDark ? t('app.toLight') : t('app.toDark')" @click="ui.toggle()">
        {{ ui.isDark ? "☀" : "☾" }}
      </n-button>
      <n-button size="small" type="primary" @click="openRepo">{{ t("app.openRepo") }}</n-button>
    </header>

    <div class="body">
      <nav v-if="!isSettings" class="sidebar">
        <div class="rtree">
          <div class="grp">{{ t("app.openedCount", { n: repo.repos.length }) }}</div>
          <div
            v-for="item in repo.repos"
            :key="item.id"
            class="ri"
            :class="{ active: item.id === repo.activeId }"
            :title="item.path"
            @click="enterRepo(item.id)"
          >
            <span class="dot" :class="dotClass(item.id)"></span>
            <span class="name">{{ item.name }}</span>
            <span v-if="!item.valid" class="fstat">⚠</span>
            <span v-else-if="item.conflicts" class="fstat" style="color: var(--error)">
              ⚠{{ item.conflicts }}
            </span>
            <span v-else-if="item.staged + item.unstaged" class="fstat">
              ●{{ item.staged + item.unstaged }}
            </span>
            <span v-else-if="item.ahead" class="fstat">↑ {{ item.ahead }}</span>
            <span v-else-if="item.behind" class="fstat">↓ {{ item.behind }}</span>

            <n-popconfirm
              @positive-click="removeRepo(item.id)"
              :positive-text="t('common.remove')"
              :negative-text="t('common.cancel')"
            >
              <template #trigger>
                <button class="remove" :title="t('app.removeFromList')" @click.stop>×</button>
              </template>
              {{ t("app.removeConfirm", { name: item.name }) }}
            </n-popconfirm>
          </div>
          <div v-if="!repo.repos.length" class="empty-tip">
            {{ t("app.noRepos") }}
          </div>
        </div>
        <div class="sidebar-foot">
          <n-button size="small" block @click="openRepo">{{ t("app.openLocalRepo") }}</n-button>
        </div>
      </nav>

      <main class="main">
        <template v-if="!showDashboard">
          <div v-if="!isSettings && repo.status?.midState" class="banner warn">
            <span>⚠</span>
            <span>
              {{ t("app.midStateBanner", { state: midStateLabel(repo.status.midState) }) }}
            </span>
            <span class="grow"></span>
            <n-button size="tiny" @click="openProblems">{{ t("problem.open") }}</n-button>
          </div>

          <div class="content">
            <router-view v-if="isSettings || repo.status" />
            <div v-else class="empty">
              <div class="big">◌</div>
              <p>{{ repo.lastError || t("app.loadingStatus") }}</p>
            </div>
          </div>
        </template>

        <DashboardView v-else />
      </main>
    </div>

    <footer class="statusbar">
      <button class="cell rescue" :title="t('problem.title')" @click="openProblems">
        <span v-if="repo.lastError" style="color: var(--error)">⚠ {{ repo.lastError }}</span>
        <template v-else>🛟 {{ rescueLabel }}</template>
      </button>
      <span class="grow"></span>
      <template v-if="repo.activeRepo">
        <span class="cell">
          {{ t("app.statusCounts", { unstaged: repo.unstagedCount, staged: repo.stagedCount, conflicts: repo.conflictCount }) }}
        </span>
        <span class="cell">
          {{ repo.currentBranch }}
          <template v-if="repo.activeRepo.ahead">↑ {{ repo.activeRepo.ahead }}</template>
          <template v-if="repo.activeRepo.behind">↓ {{ repo.activeRepo.behind }}</template>
        </span>
      </template>
      <span class="cell">{{ isTauri ? t("app.systemEngine") : t("app.designPreview") }}</span>
    </footer>

    <n-modal
      v-model:show="auditOpen"
      preset="card"
      :title="t('audit.title')"
      style="width: min(620px, 92vw)"
    >
      <div v-if="auditReport" class="audit-body">
        <div class="faint small">
          {{ t("audit.branch") }} <b>{{ auditReport.branch }}</b>
          <template v-if="auditReport.upstream"> · {{ t("audit.upstream") }} {{ auditReport.upstream }}</template>
          · ↑ {{ auditReport.ahead }} / ↓ {{ auditReport.behind }}
        </div>

        <div v-if="auditReport.blockers.length" class="audit-group block">
          <b>{{ t("audit.needsConfirm") }}</b>
          <ul>
            <li v-for="(item, index) in auditReport.blockers" :key="`b${index}`">
              {{ item.detail }}
              <span v-if="item.path" class="faint"> · {{ item.path }}</span>
            </li>
          </ul>
        </div>

        <div v-if="auditReport.warnings.length" class="audit-group warn">
          <b>{{ t("audit.warnings") }}</b>
          <ul>
            <li v-for="(item, index) in auditReport.warnings" :key="`w${index}`">
              {{ item.detail }}
              <span v-if="item.path" class="faint"> · {{ item.path }}</span>
            </li>
          </ul>
        </div>

        <div v-if="!auditReport.blockers.length && !auditReport.warnings.length" class="faint small">
          {{ t("audit.passed") }}
        </div>
      </div>
      <template #footer>
        <div class="audit-foot">
          <span class="grow"></span>
          <n-button size="small" @click="auditOpen = false">{{ t("common.cancel") }}</n-button>
          <n-button size="small" type="primary" :loading="syncBusy" @click="confirmPush">
            {{ auditHasBlockers ? t("audit.pushAnyway") : t("audit.push") }}
          </n-button>
        </div>
      </template>
    </n-modal>
  </div>
</template>

<style scoped>
.audit-body {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.audit-group {
  border-left: 3px solid var(--border);
  padding-left: 10px;
}
.audit-group.block {
  border-color: var(--error);
}
.audit-group.warn {
  border-color: var(--warning);
}
.audit-group ul {
  margin: 6px 0 0;
  padding-left: 18px;
  font-size: 12.5px;
  line-height: 1.7;
}
.audit-foot {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
}
.rescue {
  border: none;
  background: none;
  padding: 0;
  font: inherit;
  color: inherit;
  cursor: pointer;
}
.rescue:hover {
  color: var(--text);
}
</style>
