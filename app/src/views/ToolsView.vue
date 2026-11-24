<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { NButton, NInput, NPopconfirm, useMessage } from "naive-ui";
import GitignoreModal from "@/components/GitignoreModal.vue";
import { api } from "@/ipc";
import type {
  BackupRef,
  EnvReport,
  EolReport,
  FilePreview,
  IgnoreMatch,
  LargeFile,
  LockReport,
  ObjectStats,
  ReflogEntry,
  SecretFinding,
} from "@/ipc/types";
import { buildPathTree, flattenPathTree } from "@/lib/fileTree";
import { t } from "@/lib/i18n";
import { useProblemReporter } from "@/lib/problems";
import { useRepoStore } from "@/stores/repo";

const store = useRepoStore();
const message = useMessage();
const report = useProblemReporter();

interface Tool {
  key: string;
  label: string;
  hint: string;
  planned?: boolean;
}

const tools = computed<Tool[]>(() => [
  { key: "overview", label: t("tools.overview"), hint: t("tools.overviewHint") },
  { key: "large", label: t("tools.large"), hint: t("tools.largeHint") },
  { key: "backups", label: t("tools.backups"), hint: t("tools.backupsHint") },
  { key: "gitignore", label: t("tools.gitignore"), hint: t("tools.gitignoreHint") },
  { key: "ignore", label: t("tools.ignore"), hint: t("tools.ignoreHint") },
  { key: "eol", label: t("tools.eol"), hint: t("tools.eolHint") },
  { key: "objects", label: t("tools.objects"), hint: t("tools.objectsHint") },
  { key: "reflog", label: t("tools.reflog"), hint: t("tools.reflogHint") },
  { key: "locks", label: t("tools.locks"), hint: t("tools.locksHint") },
  { key: "env", label: t("tools.env"), hint: t("tools.envHint") },
  { key: "secrets", label: t("tools.secrets"), hint: t("tools.secretsHint") },
  { key: "cleanup", label: t("tools.cleanup"), hint: t("tools.cleanupHint") },
]);

const files = ref<string[]>([]);
const filesLoading = ref(false);
const collapsed = ref<Set<string>>(new Set());
const selectedPath = ref("");
const preview = ref<FilePreview | null>(null);
const previewLoading = ref(false);
const activeTool = ref<string | null>(null);
const largeFiles = ref<LargeFile[]>([]);
const backups = ref<BackupRef[]>([]);
const gitignoreOpen = ref(false);
const ignorePath = ref("");
const ignoreMatch = ref<IgnoreMatch | null>(null);
const ignoreChecked = ref(false);
const eolReport = ref<EolReport | null>(null);
const objectStats = ref<ObjectStats | null>(null);
const fsckOutput = ref("");
const fsckLoading = ref(false);
const reflogEntries = ref<ReflogEntry[]>([]);
const lockReport = ref<LockReport | null>(null);
const envReport = ref<EnvReport | null>(null);
const secretFindings = ref<SecretFinding[]>([]);
const highlightLine = ref<number | null>(null);
const showFullPreview = ref(false);
const toolLoading = ref(false);

const rows = computed(() =>
  // shortcut: 一次性渲染完整文件树；十万级仓库需要虚拟滚动或按目录懒加载。
  flattenPathTree(buildPathTree(files.value), (path) => collapsed.value.has(path)),
);
const activeToolLabel = computed(
  () => tools.value.find((tool) => tool.key === activeTool.value)?.label ?? "",
);

const focused = computed(() => {
  const current = preview.value;
  const line = highlightLine.value;
  if (!current || current.binary || !line) return null;
  const lines = current.content.split("\n");
  if (line > lines.length) return null;
  const start = Math.max(1, line - 20);
  const end = Math.min(lines.length, line + 20);
  return {
    line,
    start,
    end,
    total: lines.length,
    rows: lines.slice(start - 1, end).map((text, index) => ({
      no: start + index,
      text,
      hit: start + index === line,
    })),
  };
});

function toggleDir(path: string): void {
  const next = new Set(collapsed.value);
  if (next.has(path)) next.delete(path);
  else next.add(path);
  collapsed.value = next;
}

async function loadFiles(): Promise<void> {
  const id = store.activeId;
  if (!id) return;
  filesLoading.value = true;
  try {
    files.value = await api.files(id);
  } catch (error) {
    report(error);
  } finally {
    filesLoading.value = false;
  }
}

async function loadBackups(): Promise<void> {
  const id = store.activeId;
  if (!id) return;
  try {
    backups.value = await api.backupRefs(id);
  } catch (error) {
    report(error);
  }
}

async function selectFile(path: string, focusLine: number | null = null): Promise<void> {
  const id = store.activeId;
  if (!id) return;
  selectedPath.value = path;
  activeTool.value = null;
  highlightLine.value = focusLine;
  showFullPreview.value = false;
  previewLoading.value = true;
  try {
    preview.value = await api.filePreview(id, path);
  } catch (error) {
    preview.value = null;
    report(error);
  } finally {
    previewLoading.value = false;
  }
}

async function openFinding(item: SecretFinding): Promise<void> {
  await selectFile(item.path, item.line);
}

async function backToScan(): Promise<void> {
  highlightLine.value = null;
  showFullPreview.value = false;
  activeTool.value = "secrets";
}

async function reloadPreview(): Promise<void> {
  if (selectedPath.value) await selectFile(selectedPath.value, highlightLine.value);
}

async function runTool(tool: Tool): Promise<void> {
  if (tool.planned || !store.activeId) return;
  if (tool.key === "gitignore") {
    gitignoreOpen.value = true;
    return;
  }
  activeTool.value = tool.key;
  toolLoading.value = true;
  const id = store.activeId;
  try {
    if (tool.key === "large") {
      largeFiles.value = await api.largeFiles(id, 50);
    } else if (tool.key === "backups") {
      await loadBackups();
    } else if (tool.key === "ignore") {
      ignorePath.value = selectedPath.value;
      ignoreMatch.value = null;
      ignoreChecked.value = false;
    } else if (tool.key === "eol") {
      eolReport.value = await api.eolReport(id);
    } else if (tool.key === "objects") {
      objectStats.value = await api.objectStats(id);
    } else if (tool.key === "reflog") {
      reflogEntries.value = await api.reflog(id, 100);
    } else if (tool.key === "locks") {
      lockReport.value = await api.lockReport(id);
    } else if (tool.key === "env") {
      envReport.value = await api.envReport(id);
    } else if (tool.key === "secrets") {
      secretFindings.value = await api.secretScan(id, 200);
    } else if (tool.key === "cleanup") {
      await loadBackups();
    }
  } catch (error) {
    report(error);
  } finally {
    toolLoading.value = false;
  }
}

async function runIgnoreCheck(): Promise<void> {
  if (!store.activeId || !ignorePath.value.trim()) return;
  toolLoading.value = true;
  try {
    ignoreMatch.value = await api.checkIgnore(store.activeId, ignorePath.value.trim());
    ignoreChecked.value = true;
  } catch (error) {
    ignoreMatch.value = null;
    ignoreChecked.value = false;
    report(error);
  } finally {
    toolLoading.value = false;
  }
}

async function runFsck(): Promise<void> {
  if (!store.activeId) return;
  fsckLoading.value = true;
  try {
    fsckOutput.value = (await api.fsck(store.activeId)) || t("tools.fsckOk");
  } catch (error) {
    report(error);
  } finally {
    fsckLoading.value = false;
  }
}

async function createRecoveryBranch(oid: string, label: string): Promise<void> {
  if (!store.activeId) return;
  const safe = label.replace(/[^A-Za-z0-9._-]+/g, "-").replace(/^-+/, "").slice(0, 40);
  const name = `recovery/${safe || "restore"}-${oid.slice(0, 7)}`;
  try {
    await store.createBranch(name, oid, false);
    message.success(t("tools.recoveryCreated", { name }));
  } catch (error) {
    report(error);
  }
}

async function copy(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    message.success(t("common.copied"));
  } catch {
    message.warning(t("common.copyFailed"));
  }
}

async function cleanupBackups(): Promise<void> {
  if (!store.activeId) return;
  try {
    const count = await api.cleanupBackups(store.activeId);
    await loadBackups();
    message.success(
      count ? t("tools.cleanupDone", { n: count }) : t("tools.cleanupNone"),
    );
  } catch (error) {
    report(error);
  }
}

function sizeText(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

onMounted(() => {
  void loadFiles();
  void loadBackups();
});

watch(
  () => store.activeId,
  () => {
    selectedPath.value = "";
    preview.value = null;
    activeTool.value = null;
    largeFiles.value = [];
    backups.value = [];
    ignoreMatch.value = null;
    ignoreChecked.value = false;
    eolReport.value = null;
    objectStats.value = null;
    fsckOutput.value = "";
    fsckLoading.value = false;
    reflogEntries.value = [];
    lockReport.value = null;
    envReport.value = null;
    secretFindings.value = [];
    void loadFiles();
    void loadBackups();
  },
);
</script>

<template>
  <div v-if="!store.activeId" class="empty">
    <div class="big">🛠</div>
    <p>{{ t("tools.noRepo") }}</p>
  </div>

  <div v-else class="ws">
    <aside class="filepane">
      <div class="panehead">
        <b>{{ t("tools.files") }}</b>
        <span class="badge plain">{{ files.length }}</span>
        <span class="grow"></span>
        <n-button size="tiny" quaternary @click="loadFiles">{{ t("tools.refresh") }}</n-button>
      </div>
      <div class="filelist">
        <div v-if="filesLoading" class="faint small" style="padding: 8px 10px">
          {{ t("tools.loading") }}
        </div>
        <div v-else-if="!rows.length" class="faint small" style="padding: 8px 10px">
          {{ t("tools.emptyFiles") }}
        </div>
        <template v-for="row in rows" :key="row.node.path">
          <div
            v-if="row.node.kind === 'dir'"
            class="fitem dir"
            :style="{ paddingLeft: `${10 + row.depth * 14}px` }"
            @click="toggleDir(row.node.path)"
          >
            <span class="chevron">{{ collapsed.has(row.node.path) ? "▸" : "▾" }}</span>
            <span class="fname grow ellipsis">{{ row.node.name }}</span>
            <span class="faint small">{{ row.node.fileCount }}</span>
          </div>
          <div
            v-else
            class="fitem"
            :class="{ selected: selectedPath === row.node.path }"
            :style="{ paddingLeft: `${10 + row.depth * 14}px` }"
            :title="row.node.path"
            @click="selectFile(row.node.path)"
          >
            <span class="ficon">·</span>
            <span class="fname grow ellipsis">{{ row.node.name }}</span>
          </div>
        </template>
      </div>
    </aside>

    <section class="diffpane">
      <div class="panehead">
        <b>
          {{
            activeTool
              ? activeToolLabel
              : selectedPath || t("tools.preview")
          }}
        </b>
        <span v-if="activeTool" class="badge plain">{{ t("tools.readonly") }}</span>
        <span class="grow"></span>
        <n-button v-if="activeTool" size="tiny" quaternary @click="activeTool = null">
          {{ t("tools.backToFile") }}
        </n-button>
        <template v-else>
          <n-button v-if="highlightLine" size="tiny" quaternary @click="backToScan">
            {{ t("tools.backToScan") }}
          </n-button>
          <n-button size="tiny" quaternary :disabled="!selectedPath" @click="reloadPreview">
            {{ t("tools.reload") }}
          </n-button>
        </template>
      </div>

      <div class="toolbody">
        <template v-if="activeTool === 'overview'">
          <dl class="facts">
            <dt>{{ t("tools.branch") }}</dt>
            <dd>{{ store.currentBranch || "—" }}</dd>
            <dt>{{ t("tools.head") }}</dt>
            <dd class="mono">{{ store.status?.head.oid?.slice(0, 7) ?? "—" }}</dd>
            <dt>{{ t("tools.upstream") }}</dt>
            <dd class="mono">{{ store.status?.upstream?.name ?? t("tools.none") }}</dd>
            <dt>{{ t("tools.aheadBehind") }}</dt>
            <dd>↑ {{ store.activeRepo?.ahead ?? 0 }} / ↓ {{ store.activeRepo?.behind ?? 0 }}</dd>
            <dt>{{ t("tools.staged") }}</dt>
            <dd>{{ store.stagedCount }}</dd>
            <dt>{{ t("tools.unstaged") }}</dt>
            <dd>{{ store.unstagedCount }}</dd>
            <dt>{{ t("tools.conflicts") }}</dt>
            <dd>{{ store.conflictCount }}</dd>
            <dt>{{ t("tools.midState") }}</dt>
            <dd>{{ store.status?.midState ?? t("tools.none") }}</dd>
            <dt>{{ t("tools.fileCount") }}</dt>
            <dd>{{ files.length }}</dd>
            <dt>{{ t("tools.backupCount") }}</dt>
            <dd>{{ backups.length }}</dd>
            <dt>{{ t("tools.path") }}</dt>
            <dd class="mono">{{ store.activeRepo?.path ?? "—" }}</dd>
          </dl>
          <div class="note mt12">{{ t("tools.overviewNote") }}</div>
        </template>

        <template v-else-if="activeTool === 'large'">
          <div v-if="!largeFiles.length" class="faint small">{{ t("tools.largeEmpty") }}</div>
          <table v-else class="tool-table">
            <thead>
              <tr>
                <th>{{ t("tools.file") }}</th>
                <th class="right">{{ t("tools.size") }}</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="file in largeFiles" :key="file.path">
                <td class="mono ellipsis" :title="file.path">{{ file.path }}</td>
                <td class="right faint small">{{ sizeText(file.size) }}</td>
              </tr>
            </tbody>
          </table>
        </template>

        <template v-else-if="activeTool === 'backups'">
          <div class="row" style="margin-bottom: 12px">
            <span class="faint small">{{ t("tools.backupsHint") }}</span>
            <span class="grow"></span>
            <n-popconfirm
              :positive-text="t('tools.cleanupExpired')"
              :negative-text="t('common.cancel')"
              @positive-click="cleanupBackups"
            >
              <template #trigger>
                <n-button size="tiny" :disabled="!backups.length">
                  {{ t("tools.cleanupExpired") }}
                </n-button>
              </template>
              {{ t("tools.cleanupConfirm") }}
            </n-popconfirm>
          </div>
          <div v-if="!backups.length" class="faint small">{{ t("tools.backupsEmpty") }}</div>
          <table v-else class="tool-table">
            <thead>
              <tr>
                <th>{{ t("tools.backupRef") }}</th>
                <th>{{ t("tools.commit") }}</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="item in backups" :key="item.name">
                <td class="mono ellipsis" :title="item.name">{{ item.name }}</td>
                <td class="mono">{{ item.oid.slice(0, 7) }}</td>
                <td class="right">
                  <n-button size="tiny" quaternary @click="createRecoveryBranch(item.oid, 'backup')">
                    {{ t("tools.createBranch") }}
                  </n-button>
                </td>
              </tr>
            </tbody>
          </table>
        </template>

        <template v-else-if="activeTool === 'ignore'">
          <div class="row" style="margin-bottom: 12px">
            <n-input
              v-model:value="ignorePath"
              size="small"
              :placeholder="t('tools.ignorePlaceholder')"
              style="max-width: 380px"
              @keyup.enter="runIgnoreCheck"
            />
            <n-button size="small" :loading="toolLoading" @click="runIgnoreCheck">
              {{ t("tools.check") }}
            </n-button>
          </div>
          <template v-if="ignoreChecked">
            <dl v-if="ignoreMatch" class="facts">
              <dt>{{ t("tools.ignoreSource") }}</dt>
              <dd class="mono">{{ ignoreMatch.source }}:{{ ignoreMatch.line }}</dd>
              <dt>{{ t("tools.ignorePattern") }}</dt>
              <dd class="mono">{{ ignoreMatch.pattern }}</dd>
            </dl>
            <div v-else class="faint small">{{ t("tools.notIgnored") }}</div>
          </template>
          <div v-else class="faint small">{{ t("tools.ignoreHintDetail") }}</div>
        </template>

        <template v-else-if="activeTool === 'eol'">
          <div v-if="toolLoading" class="faint small">{{ t("tools.loading") }}</div>
          <template v-else-if="eolReport">
            <dl class="facts">
              <dt>{{ t("tools.eolTotal") }}</dt>
              <dd>{{ eolReport.total }}</dd>
              <dt>LF / CRLF</dt>
              <dd>{{ eolReport.lf }} / {{ eolReport.crlf }}</dd>
              <dt>{{ t("tools.eolMixed") }}</dt>
              <dd>{{ eolReport.mixed }}</dd>
            </dl>
            <table v-if="eolReport.files.length" class="tool-table" style="margin-top: 12px">
              <thead>
                <tr>
                  <th>{{ t("tools.file") }}</th>
                  <th>i/</th>
                  <th>w/</th>
                  <th>attr/</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="file in eolReport.files" :key="file.path">
                  <td class="mono ellipsis" :title="file.path">{{ file.path }}</td>
                  <td class="mono">{{ file.index || "—" }}</td>
                  <td class="mono">{{ file.worktree || "—" }}</td>
                  <td class="mono">{{ file.attr || "—" }}</td>
                </tr>
              </tbody>
            </table>
            <div v-else class="faint small">{{ t("tools.eolEmpty") }}</div>
            <div class="note mt12">{{ t("tools.eolNote") }}</div>
          </template>
        </template>

        <template v-else-if="activeTool === 'objects'">
          <div v-if="toolLoading" class="faint small">{{ t("tools.loading") }}</div>
          <template v-else-if="objectStats">
            <dl class="facts">
              <dt>{{ t("tools.objectsLoose") }}</dt>
              <dd>{{ objectStats.count }} · {{ sizeText(objectStats.sizeKb * 1024) }}</dd>
              <dt>{{ t("tools.objectsPacks") }}</dt>
              <dd>{{ objectStats.packs }} · {{ sizeText(objectStats.sizePackKb * 1024) }}</dd>
              <dt>{{ t("tools.objectsInPack") }}</dt>
              <dd>{{ objectStats.inPack }}</dd>
              <dt>{{ t("tools.objectsGarbage") }}</dt>
              <dd>{{ objectStats.garbage }} · {{ sizeText(objectStats.sizeGarbageKb * 1024) }}</dd>
            </dl>
            <div class="note mt12">{{ t("tools.objectsNote") }}</div>
            <div class="row mt12">
              <n-button size="small" :loading="fsckLoading" @click="runFsck">
                {{ t("tools.fsckRun") }}
              </n-button>
            </div>
            <pre v-if="fsckOutput" class="preview" style="margin-top: 12px">{{ fsckOutput }}</pre>
            <div v-else-if="!fsckLoading" class="faint small" style="margin-top: 8px">
              {{ t("tools.fsckNote") }}
            </div>
            <div class="cmdrow">
              <code>git count-objects -v</code>
              <button class="link" @click="copy('git count-objects -v')">
                {{ t("common.copy") }}
              </button>
            </div>
          </template>
        </template>

        <template v-else-if="activeTool === 'reflog'">
          <div v-if="toolLoading" class="faint small">{{ t("tools.loading") }}</div>
          <template v-else-if="reflogEntries.length">
            <table class="tool-table">
              <thead>
                <tr>
                  <th>{{ t("tools.reflogSelector") }}</th>
                  <th>{{ t("tools.reflogSubject") }}</th>
                  <th>{{ t("tools.commit") }}</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="item in reflogEntries" :key="`${item.selector}:${item.oid}`">
                  <td class="mono">{{ item.selector }}</td>
                  <td class="ellipsis" :title="item.subject">{{ item.subject }}</td>
                  <td class="mono">{{ item.oid.slice(0, 7) }}</td>
                  <td class="right">
                    <n-button size="tiny" quaternary @click="createRecoveryBranch(item.oid, 'reflog')">
                      {{ t("tools.createBranch") }}
                    </n-button>
                  </td>
                </tr>
              </tbody>
            </table>
            <div class="note mt12">{{ t("tools.reflogNote") }}</div>
          </template>
          <div v-else class="faint small">{{ t("tools.reflogEmpty") }}</div>
        </template>

        <template v-else-if="activeTool === 'locks'">
          <div v-if="toolLoading" class="faint small">{{ t("tools.loading") }}</div>
          <template v-else-if="lockReport">
            <dl class="facts">
              <dt>{{ t("tools.lockFile") }}</dt>
              <dd>
                <span v-if="lockReport.indexLock" class="badge b-yellow">
                  {{ t("tools.lockPresent", { age: lockReport.indexLock.ageSeconds }) }}
                </span>
                <span v-else class="faint">{{ t("tools.none") }}</span>
              </dd>
              <dt>{{ t("tools.midState") }}</dt>
              <dd>{{ lockReport.midState ?? t("tools.none") }}</dd>
              <dt>{{ t("tools.stateFiles") }}</dt>
              <dd class="mono">{{ lockReport.stateFiles.join(", ") || t("tools.none") }}</dd>
            </dl>
            <div class="note mt12">{{ t("tools.locksNote") }}</div>
            <div class="cmdrow">
              <code>rm .git/index.lock</code>
              <button class="link" @click="copy('rm .git/index.lock')">
                {{ t("common.copy") }}
              </button>
            </div>
            <div class="cmdrow">
              <code>git status</code>
              <button class="link" @click="copy('git status')">{{ t("common.copy") }}</button>
            </div>
          </template>
        </template>

        <template v-else-if="activeTool === 'env'">
          <div v-if="toolLoading" class="faint small">{{ t("tools.loading") }}</div>
          <template v-else-if="envReport">
            <dl class="facts">
              <dt>{{ t("tools.envVersion") }}</dt>
              <dd class="mono">{{ envReport.gitVersion }}</dd>
              <dt>{{ t("tools.envWorkdir") }}</dt>
              <dd class="mono">{{ envReport.workdir }}</dd>
              <dt>{{ t("tools.envGitDir") }}</dt>
              <dd class="mono">{{ envReport.gitDir }}</dd>
              <dt>{{ t("tools.envHooks") }}</dt>
              <dd class="mono">{{ envReport.hooksPath ?? t("tools.none") }}</dd>
              <dt>{{ t("tools.envCredential") }}</dt>
              <dd class="mono">{{ envReport.credentialHelper ?? t("tools.none") }}</dd>
              <dt>{{ t("tools.envProxy") }}</dt>
              <dd class="mono">{{ envReport.httpsProxy ?? envReport.httpProxy ?? t("tools.none") }}</dd>
              <dt>NO_PROXY</dt>
              <dd class="mono">{{ envReport.noProxy ?? t("tools.none") }}</dd>
            </dl>
          </template>
        </template>

        <template v-else-if="activeTool === 'secrets'">
          <div v-if="toolLoading" class="faint small">{{ t("tools.loading") }}</div>
          <template v-else-if="secretFindings.length">
            <table class="tool-table">
              <thead>
                <tr>
                  <th>{{ t("tools.file") }}</th>
                  <th>{{ t("tools.secretLine") }}</th>
                  <th>{{ t("tools.secretPattern") }}</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                <tr
                  v-for="item in secretFindings"
                  :key="`${item.path}:${item.line}:${item.pattern}`"
                  class="clickable"
                  @click="openFinding(item)"
                >
                  <td class="mono ellipsis" :title="item.path">{{ item.path }}</td>
                  <td class="mono">{{ item.line }}</td>
                  <td class="mono">{{ item.pattern }}</td>
                  <td class="right">
                    <button class="link" @click.stop="openFinding(item)">
                      {{ t("tools.view") }}
                    </button>
                  </td>
                </tr>
              </tbody>
            </table>
            <div class="note mt12">{{ t("tools.secretsNote") }}</div>
          </template>
          <div v-else class="faint small">{{ t("tools.secretsEmpty") }}</div>
        </template>

        <template v-else-if="activeTool === 'cleanup'">
          <dl class="facts">
            <dt>{{ t("tools.backupCount") }}</dt>
            <dd>{{ backups.length }}</dd>
          </dl>
          <div class="row mt12">
            <n-popconfirm
              :positive-text="t('tools.cleanupExpired')"
              :negative-text="t('common.cancel')"
              @positive-click="cleanupBackups"
            >
              <template #trigger>
                <n-button size="small" :disabled="!backups.length">
                  {{ t("tools.cleanupExpired") }}
                </n-button>
              </template>
              {{ t("tools.cleanupConfirm") }}
            </n-popconfirm>
          </div>
          <div class="note mt12">{{ t("tools.cleanupNote") }}</div>
          <div class="cmdrow">
            <code>git count-objects -v</code>
            <button class="link" @click="copy('git count-objects -v')">
              {{ t("common.copy") }}
            </button>
          </div>
          <div class="cmdrow">
            <code>git clean -nd</code>
            <button class="link" @click="copy('git clean -nd')">{{ t("common.copy") }}</button>
          </div>
          <div class="cmdrow">
            <code>git gc</code>
            <button class="link" @click="copy('git gc')">{{ t("common.copy") }}</button>
          </div>
        </template>

        <template v-else>
          <div v-if="previewLoading" class="faint small">{{ t("tools.loading") }}</div>
          <template v-else-if="preview">
            <div class="faint small" style="margin-bottom: 10px">
              {{ t("tools.size") }} {{ sizeText(preview.size) }}
              <span v-if="preview.truncated"> · {{ t("tools.truncated") }}</span>
            </div>
            <div v-if="preview.binary" class="faint small">{{ t("tools.binary") }}</div>
            <template v-else-if="focused && !showFullPreview">
              <div class="focus-head">
                <span class="faint small">
                  {{ t("tools.hitAt", { line: focused.line }) }} ·
                  {{ t("tools.focusRange", { start: focused.start, end: focused.end, total: focused.total }) }}
                </span>
                <span class="grow"></span>
                <button class="link" @click="showFullPreview = true">{{ t("tools.showFull") }}</button>
              </div>
              <div class="focus">
                <div
                  v-for="row in focused.rows"
                  :key="row.no"
                  class="focus-line"
                  :class="{ hit: row.hit }"
                >
                  <span class="ln">{{ row.no }}</span>
                  <span class="code">{{ row.text || " " }}</span>
                </div>
              </div>
            </template>
            <template v-else>
              <div v-if="focused" class="focus-head">
                <span class="faint small">{{ t("tools.hitAt", { line: focused.line }) }}</span>
                <span class="grow"></span>
                <button class="link" @click="showFullPreview = false">{{ t("tools.showFocus") }}</button>
              </div>
              <pre class="preview">{{ preview.content }}</pre>
            </template>
          </template>
          <div v-else class="faint small">{{ t("tools.pickFile") }}</div>
        </template>
      </div>
    </section>

    <aside class="commitpane">
      <div class="panehead">
        <b>{{ t("tools.tools") }}</b>
        <span class="grow"></span>
        <span class="badge plain">{{ t("tools.safeFirst") }}</span>
      </div>
      <div class="toolbody">
        <button
          v-for="item in tools"
          :key="item.key"
          class="toolitem"
          :class="{ active: activeTool === item.key }"
          :disabled="item.planned"
          @click="runTool(item)"
        >
          <span class="toolhead">
            <b>{{ item.label }}</b>
            <span v-if="item.planned" class="badge plain">{{ t("tools.planned") }}</span>
          </span>
          <span class="toolhint">{{ item.hint }}</span>
        </button>
        <p class="faint small" style="margin: 12px 0 0">{{ t("tools.planNote") }}</p>
      </div>
    </aside>
  </div>

  <GitignoreModal v-model:show="gitignoreOpen" />
</template>

<style scoped>
.toolbody {
  flex: 1;
  min-height: 0;
  overflow: auto;
  padding: 14px 18px;
}
.toolitem {
  display: flex;
  flex-direction: column;
  gap: 3px;
  width: 100%;
  padding: 9px 10px;
  margin-bottom: 6px;
  border: 1px solid var(--border);
  border-radius: 4px;
  background: none;
  color: var(--text);
  font-family: inherit;
  font-size: 12.5px;
  text-align: left;
  cursor: pointer;
}
.toolitem:hover:not(:disabled) {
  background: var(--bg-hover);
}
.toolitem.active {
  border-color: var(--primary);
  background: var(--accent-dim);
}
.toolitem:disabled {
  cursor: default;
  opacity: 0.6;
}
.toolhead {
  display: flex;
  align-items: center;
  gap: 6px;
}
.toolhint {
  font-size: 11.5px;
  line-height: 1.5;
  color: var(--text-faint);
}
.facts {
  display: grid;
  grid-template-columns: 140px 1fr;
  gap: 8px 16px;
  margin: 0;
  font-size: 12.5px;
}
.facts dt {
  color: var(--text-faint);
}
.facts dd {
  margin: 0;
  overflow-wrap: anywhere;
}
.tool-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 12.5px;
}
.tool-table th,
.tool-table td {
  padding: 7px 8px;
  border-bottom: 1px solid var(--border);
  text-align: left;
}
.tool-table th {
  color: var(--text-faint);
  font-weight: 500;
  font-size: 11.5px;
}
.preview {
  margin: 0;
  padding: 12px;
  border: 1px solid var(--border);
  border-radius: 4px;
  background: var(--bg-sunken);
  color: var(--text-muted);
  font-family: var(--mono);
  font-size: 12px;
  line-height: 1.6;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}
.cmdrow {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 8px;
  font-size: 11.5px;
}
.cmdrow code {
  flex: 1;
  min-width: 0;
  padding: 4px 6px;
  overflow-wrap: anywhere;
  border: 1px solid var(--border);
  border-radius: 3px;
  background: var(--bg-sunken);
  color: var(--text-muted);
  font-family: var(--mono);
}
.link {
  border: none;
  background: none;
  padding: 0;
  color: var(--primary);
  font: inherit;
  cursor: pointer;
  white-space: nowrap;
}
.clickable {
  cursor: pointer;
}
.clickable:hover {
  background: var(--bg-hover);
}
.focus-head {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 8px 0;
}
.focus {
  border: 1px solid var(--border);
  border-radius: 4px;
  background: var(--bg-sunken);
  overflow: auto;
  max-height: 60vh;
}
.focus-line {
  display: flex;
  gap: 10px;
  padding: 1px 8px;
  font-family: var(--mono);
  font-size: 12px;
  line-height: 1.6;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}
.focus-line .ln {
  width: 46px;
  flex: none;
  text-align: right;
  color: var(--text-faint);
  user-select: none;
}
.focus-line.hit {
  background: color-mix(in srgb, var(--warning) 18%, transparent);
}
</style>
