<script setup lang="ts">
import { computed, h, onBeforeUnmount, onMounted, ref, shallowRef, watch } from "vue";
import { NButton, NCheckbox, NInput, useMessage } from "naive-ui";
import DiffViewer from "@/components/DiffViewer.vue";
import GitignoreModal from "@/components/GitignoreModal.vue";
import { api } from "@/ipc";
import type { FileDiff, StatusEntry } from "@/ipc/types";
import { buildFileTree, flattenTree, pathsUnderDirectory, type TreeDirNode } from "@/lib/fileTree";
import { t } from "@/lib/i18n";
import { useProblemReporter } from "@/lib/problems";
import { useRepoStore, useUiStore } from "@/stores/repo";

const store = useRepoStore();
const ui = useUiStore();
const message = useMessage();
const report = useProblemReporter();

const selection = ref<{ path: string; staged: boolean } | null>(null);
const diff = shallowRef<FileDiff | null>(null);
const diffLoading = ref(false);
const mode = computed({
  get: (): "side" | "unified" => ui.settings.diffMode,
  set: (value: "side" | "unified") => {
    ui.settings.diffMode = value;
  },
});
const runHooks = ref(true);
const commitMessage = ref("");

const ignoreOpen = ref(false);

const groups = computed(() => [
  {
    key: "staged",
    title: t("workspace.group.staged"),
    hint: t("workspace.group.stagedHint"),
    entries: store.stagedEntries,
    staged: true,
    action: t("workspace.group.unstageAll"),
  },
  {
    key: "unstaged",
    title: t("workspace.group.unstaged"),
    hint: t("workspace.group.unstagedHint"),
    entries: store.unstagedEntries,
    staged: false,
    action: t("workspace.group.stageAll"),
  },
  {
    key: "conflicts",
    title: t("workspace.group.conflicts"),
    hint: t("workspace.group.conflictsHint"),
    entries: store.conflictEntries,
    staged: false,
    action: "",
  },
]);

const partialPaths = computed(() => {
  const staged = new Set(store.stagedEntries.map((e) => e.path));
  return new Set(store.unstagedEntries.filter((e) => staged.has(e.path)).map((e) => e.path));
});

type DisplayRow =
  | { kind: "file"; entry: StatusEntry; name: string; depth: number }
  | { kind: "dir"; node: TreeDirNode; depth: number };

const viewMode = computed({
  get: (): "list" | "tree" => ui.settings.fileView,
  set: (value: "list" | "tree") => {
    ui.settings.fileView = value;
  },
});
const collapsedDirs = ref<Set<string>>(new Set());

function dirKey(groupKey: string, path: string): string {
  return `${groupKey}:${path}`;
}

function isDirCollapsed(groupKey: string, path: string): boolean {
  return collapsedDirs.value.has(dirKey(groupKey, path));
}

function toggleDir(groupKey: string, path: string): void {
  const key = dirKey(groupKey, path);
  const next = new Set(collapsedDirs.value);
  if (next.has(key)) next.delete(key);
  else next.add(key);
  collapsedDirs.value = next;
}

const rowsByGroup = computed<Record<string, DisplayRow[]>>(() => {
  const result: Record<string, DisplayRow[]> = {};
  for (const group of groups.value) {
    if (viewMode.value === "list") {
      result[group.key] = group.entries.map((entry) => ({
        kind: "file",
        entry,
        name: entry.path,
        depth: 0,
      }));
      continue;
    }
    result[group.key] = flattenTree(
      buildFileTree(group.entries),
      (path) => isDirCollapsed(group.key, path),
    ).map(({ node, depth }) =>
      node.kind === "dir"
        ? { kind: "dir", node, depth }
        : { kind: "file", entry: node.entry, name: node.name, depth },
    );
  }
  return result;
});

const draftKey = computed(() => `git4u.draft.${store.activeId ?? "none"}`);
const canCommit = computed(
  () => store.stagedCount > 0 && commitMessage.value.trim().length > 0,
);
const canAmend = computed(() => commitMessage.value.trim().length > 0);
const commitBlockReason = computed(() => {
  if (store.stagedCount === 0) return t("workspace.blockStageFirst");
  if (!commitMessage.value.trim()) return t("workspace.blockWriteMessage");
  return "";
});

const cliCommand = computed(() => {
  const text = commitMessage.value.trim().split("\n")[0];
  if (!text) return "git commit";
  const flags = `git commit${runHooks.value ? "" : " --no-verify"}`;
  return `${flags} -m "${text.replace(/"/g, '\\"')}"`;
});

const totalAdditions = computed(() =>
  store.stagedEntries.reduce((sum, e) => sum + e.additions, 0),
);
const totalDeletions = computed(() =>
  store.stagedEntries.reduce((sum, e) => sum + e.deletions, 0),
);

watch(
  () => store.activeId,
  () => {
    commitMessage.value = store.activeId ? localStorage.getItem(draftKey.value) ?? "" : "";
    collapsedDirs.value = new Set();
  },
  { immediate: true },
);

watch(commitMessage, (value) => {
  if (store.activeId) localStorage.setItem(draftKey.value, value);
});

watch(
  () => store.status?.version,
  () => {
    if (!selection.value) {
      pickDefault();
      return;
    }
    const current = selection.value;
    const stillThere = store.status?.entries.some(
      (e) =>
        e.path === current.path &&
        (current.staged ? e.staged : !e.staged && e.worktreeState !== "unmodified"),
    );
    if (!stillThere) {
      selection.value = null;
      diff.value = null;
      pickDefault();
    }
  },
  { immediate: true },
);

function pickDefault(): void {
  const entry = store.stagedEntries[0] ?? store.unstagedEntries[0] ?? store.conflictEntries[0];
  if (!entry) return;
  const staged = store.stagedEntries.includes(entry);
  selection.value = { path: entry.path, staged };
  void loadDiff();
}

async function loadDiff(): Promise<void> {
  const current = selection.value;
  if (!current || !store.activeId) return;
  diffLoading.value = true;
  try {
    diff.value = await api.diff(store.activeId!, current.path, current.staged);
  } catch (error) {
    diff.value = null;
    report(error);
  } finally {
    diffLoading.value = false;
  }
}

function select(entry: StatusEntry, staged: boolean): void {
  selection.value = { path: entry.path, staged };
  void loadDiff();
}

function isSelected(entry: StatusEntry, staged: boolean): boolean {
  return selection.value?.path === entry.path && selection.value.staged === staged;
}

function letter(entry: StatusEntry, staged: boolean): string {
  const state = staged ? entry.indexState : entry.worktreeState;
  switch (state) {
    case "added":
      return "A";
    case "modified":
      return "M";
    case "deleted":
      return "D";
    case "renamed":
      return "R";
    case "copied":
      return "C";
    case "unmerged":
      return "!";
    case "untracked":
      return "??";
    default:
      return "·";
  }
}

function iconClass(entry: StatusEntry, staged: boolean): string {
  const state = staged ? entry.indexState : entry.worktreeState;
  if (state === "added" || state === "untracked") return "a";
  if (state === "deleted" || state === "unmerged") return "d";
  if (state === "renamed" || state === "copied") return "r";
  if (state === "modified") return "m";
  return "";
}

async function run(action: () => Promise<void>): Promise<boolean> {
  try {
    await action();
    if (selection.value) await loadDiff();
    return true;
  } catch (error) {
    report(error);
    return false;
  }
}

async function stage(entry: StatusEntry): Promise<void> {
  await run(async () => {
    await store.stage([entry.path]);
    if (selection.value?.path === entry.path) selection.value = { path: entry.path, staged: true };
  });
}

async function unstage(entry: StatusEntry): Promise<void> {
  await run(async () => {
    await store.unstage([entry.path]);
    if (selection.value?.path === entry.path) selection.value = { path: entry.path, staged: false };
  });
}

async function stageAll(staged: boolean): Promise<void> {
  const ok = await run(async () => {
    if (staged) {
      await store.unstage([]);
      selection.value = null;
      diff.value = null;
    } else {
      await store.stage([]);
    }
  });
  if (ok) message.success(staged ? t("workspace.unstagedAllDone") : t("workspace.stagedAllDone"));
}

async function stageDir(groupKey: string, dirPath: string): Promise<void> {
  const group = groups.value.find((item) => item.key === groupKey);
  if (!group) return;

  const paths = pathsUnderDirectory(group.entries, dirPath);
  if (!paths.length) return;

  const ok = await run(async () => {
    if (group.staged) await store.unstage(paths);
    else await store.stage(paths);
    const current = selection.value;
    if (current && paths.includes(current.path)) {
      selection.value = { path: current.path, staged: !group.staged };
    }
  });

  if (ok) {
    message.success(
      group.staged
        ? t("workspace.unstagedDirDone", { path: dirPath, n: paths.length })
        : t("workspace.stagedDirDone", { path: dirPath, n: paths.length }),
    );
  }
}

async function undoCommit(oid: string): Promise<void> {
  try {
    const undone = await store.undoLast(oid);
    if (undone) message.success(t("workspace.undoDone"));
    else message.info(t("workspace.undoNothing"));
    if (selection.value) await loadDiff();
  } catch (error) {
    report(error);
  }
}

async function doCommit(amend: boolean): Promise<void> {
  if (amend ? !canAmend.value : !canCommit.value) return;
  try {
    const oid = await store.commit(commitMessage.value, amend, runHooks.value);
    commitMessage.value = "";
    localStorage.removeItem(draftKey.value);
    selection.value = null;
    diff.value = null;
    const text = t(amend ? "workspace.amendDone" : "workspace.commitDone", {
      oid: oid.slice(0, 7),
    });
    // 首个提交没有 pre_head，无法 soft undo，不显示会失败的按钮。
    const canUndo = amend || store.log.length > 1;
    message.success(
      () =>
        h("span", canUndo
          ? [
              text,
              h(
                "button",
                { class: "problem-toast-action", onClick: () => void undoCommit(oid) },
                t("workspace.undo"),
              ),
            ]
          : [text]),
      { duration: 10000 },
    );
  } catch (error) {
    report(error);
  }
}

async function copy(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    message.success(t("common.copied"));
  } catch {
    message.warning(t("common.copyFailedManual"));
  }
}

function onGlobalKey(event: KeyboardEvent): void {
  if ((event.ctrlKey || event.metaKey) && event.key === "Enter" && canCommit.value) {
    event.preventDefault();
    void doCommit(false);
  }
}

onMounted(() => window.addEventListener("keydown", onGlobalKey));
onBeforeUnmount(() => window.removeEventListener("keydown", onGlobalKey));
</script>

<template>
  <div class="workspace">
    <div class="guide">
      <div class="gstep" :class="store.stagedCount ? 'done' : 'active'">
        <span class="num">{{ store.stagedCount ? "✓" : "1" }}</span>
        <span>
          <b>{{ t("workspace.changeSelection") }}</b>
          <span class="hint">
            {{ store.stagedCount ? t("workspace.stagedCount", { n: store.stagedCount }) : t("workspace.selectFiles") }}
          </span>
        </span>
      </div>
      <span class="faint">→</span>
      <div
        class="gstep"
        :class="commitMessage.trim() && store.stagedCount ? 'done' : store.stagedCount ? 'active' : ''"
      >
        <span class="num">{{ commitMessage.trim() && store.stagedCount ? "✓" : "2" }}</span>
        <span>
          <b>{{ t("workspace.writeMessage") }}</b>
          <span class="hint">{{ commitMessage.trim() ? t("workspace.messageFilled") : t("workspace.messageMissing") }}</span>
        </span>
      </div>
      <span class="faint">→</span>
      <div class="gstep">
        <span class="num">3</span>
        <span><b>{{ t("workspace.pushStep") }}</b><span class="hint">{{ t("workspace.pushLater") }}</span></span>
      </div>
      <span class="grow"></span>
      <span class="faint small">{{ t("workspace.stageExplain") }}</span>
    </div>

    <div class="ws">
      <aside class="filepane">
        <div class="panehead">
          <b>{{ t("workspace.changes") }}</b>
          <span class="badge plain">
            {{ t("workspace.changeCounts", { unstaged: store.unstagedCount, staged: store.stagedCount }) }}
          </span>
          <span class="grow"></span>
          <n-button size="tiny" quaternary @click="ignoreOpen = true">{{ t("workspace.ignoreFiles") }}</n-button>
          <n-button size="tiny" quaternary @click="store.refreshStatus()">{{ t("common.refresh") }}</n-button>
        </div>
        <div class="summary">
          <span style="color: var(--warning)">{{ t("workspace.unstagedCount", { n: store.unstagedCount }) }}</span>
          <span style="color: var(--success)">{{ t("workspace.stagedWillCommit", { n: store.stagedCount }) }}</span>
          <span>{{ t("workspace.conflictCount", { n: store.conflictCount }) }}</span>
          <span class="grow"></span>
          <div class="seg" role="group" :aria-label="t('workspace.viewAria')">
            <button :class="{ on: viewMode === 'list' }" @click="viewMode = 'list'">{{ t("workspace.viewList") }}</button>
            <button :class="{ on: viewMode === 'tree' }" @click="viewMode = 'tree'">{{ t("workspace.viewTree") }}</button>
          </div>
        </div>

        <div class="filelist">
          <template v-for="group in groups" :key="group.key">
            <div class="fgroup">
              ▾ {{ group.title }} ({{ group.entries.length }})
              <span class="faint small">{{ group.hint }}</span>
              <span class="grow"></span>
              <button
                v-if="group.action && group.entries.length"
                class="link"
                @click="stageAll(group.staged)"
              >
                {{ group.action }}
              </button>
            </div>
            <template v-for="row in rowsByGroup[group.key]" :key="row.kind === 'dir' ? `d:${row.node.path}` : `f:${row.entry.path}`">
              <div
                v-if="row.kind === 'dir'"
                class="fitem dir"
                :style="{ paddingLeft: `${10 + row.depth * 14}px` }"
                :title="t('workspace.dirTitle', { path: row.node.path, n: row.node.fileCount })"
                @click="toggleDir(group.key, row.node.path)"
              >
                <span class="chevron">
                  {{ isDirCollapsed(group.key, row.node.path) ? "▸" : "▾" }}
                </span>
                <span class="fname grow ellipsis">{{ row.node.name }}</span>
                <span class="faint small">{{ row.node.fileCount }}</span>
                <span v-if="row.node.additions || row.node.deletions" class="fstat">
                  <span v-if="row.node.additions" class="add">+{{ row.node.additions }}</span>
                  <span v-if="row.node.deletions" class="del">−{{ row.node.deletions }}</span>
                </span>
                <button
                  v-if="group.key === 'staged'"
                  class="row-action"
                  :title="t('workspace.unstageDir')"
                  @click.stop="stageDir(group.key, row.node.path)"
                >
                  {{ t("workspace.unstage") }}
                </button>
                <button
                  v-else-if="group.key === 'unstaged'"
                  class="row-action"
                  :title="t('workspace.stageDir')"
                  @click.stop="stageDir(group.key, row.node.path)"
                >
                  {{ t("workspace.stage") }}
                </button>
              </div>
              <div
                v-else
                class="fitem"
                :class="{ selected: isSelected(row.entry, group.staged) }"
                :style="{ paddingLeft: `${10 + row.depth * 14}px` }"
                :title="row.entry.path"
                @click="select(row.entry, group.staged)"
              >
                <span class="ficon" :class="iconClass(row.entry, group.staged)">
                  {{ letter(row.entry, group.staged) }}
                </span>
                <span class="fname grow ellipsis">{{ row.name }}</span>
                <span v-if="partialPaths.has(row.entry.path)" class="partial-mark">◐</span>
                <span v-if="row.entry.additions || row.entry.deletions" class="fstat">
                  <span v-if="row.entry.additions" class="add">+{{ row.entry.additions }}</span>
                  <span v-if="row.entry.deletions" class="del">−{{ row.entry.deletions }}</span>
                </span>
                <button
                  v-if="group.key === 'staged'"
                  class="row-action"
                  :title="t('workspace.unstageFile')"
                  @click.stop="unstage(row.entry)"
                >
                  {{ t("workspace.unstage") }}
                </button>
                <button
                  v-else-if="group.key === 'unstaged'"
                  class="row-action"
                  :title="t('workspace.stageFile')"
                  @click.stop="stage(row.entry)"
                >
                  {{ t("workspace.stage") }}
                </button>
              </div>
            </template>
            <div v-if="!group.entries.length" class="faint small" style="padding: 5px 10px">
              {{ group.key === "conflicts" ? t("workspace.noConflicts") : t("workspace.noFiles") }}
            </div>
          </template>
        </div>

        <div class="filepane-foot">
          <n-button size="small" :disabled="!store.unstagedCount" @click="stageAll(false)">
            {{ t("workspace.group.stageAll") }}
          </n-button>
          <n-button size="small" :disabled="!store.stagedCount" @click="stageAll(true)">
            {{ t("workspace.group.unstageAll") }}
          </n-button>
        </div>
      </aside>

      <section class="diffpane">
        <div class="difftoolbar">
          <span v-if="selection" class="badge plain">{{ selection.path }}</span>
          <span v-else class="badge plain">{{ t("workspace.diff") }}</span>
          <span v-if="selection && partialPaths.has(selection.path)" class="partial-mark small">
            {{ t("workspace.partialStaged") }}
          </span>
          <span v-if="selection" class="faint small">
            {{ selection.staged ? t("workspace.stagedVsHead") : t("workspace.worktreeVsIndex") }}
          </span>
          <span class="grow"></span>
          <div class="seg">
            <button :class="{ on: mode === 'side' }" @click="mode = 'side'">{{ t("workspace.side") }}</button>
            <button :class="{ on: mode === 'unified' }" @click="mode = 'unified'">{{ t("workspace.unified") }}</button>
          </div>
          <n-button size="tiny" quaternary :disabled="!selection" @click="loadDiff">{{ t("workspace.reloadDiff") }}</n-button>
        </div>
        <div v-if="selection && !selection.staged" class="diffhelp">
          <span>💡</span>
          <span>{{ t("workspace.stageTip") }}</span>
          <span class="grow"></span>
          <span class="faint">{{ t("workspace.stageButtonHint") }}</span>
        </div>
        <div class="diffbody">
          <div v-if="!store.status?.entries.length" class="diff-empty">
            {{ t("workspace.clean") }}
          </div>
          <DiffViewer v-else :diff="diff" :loading="diffLoading" :mode="mode" />
          <div v-if="selection" class="note mt12">
            {{ t("workspace.uncertain") }}
          </div>
        </div>
      </section>

      <aside class="commitpane">
        <div class="panehead">
          <b>{{ t("workspace.commit") }}</b>
          <span class="badge" :class="store.stagedCount ? 'b-green' : 'plain'">
            {{ t("workspace.willCommit", { n: store.stagedCount }) }}
          </span>
        </div>

        <div class="commitbody">
          <div>
            <div v-for="entry in store.stagedEntries" :key="entry.path" class="commit-file">
              <span class="ficon" :class="iconClass(entry, true)">{{ letter(entry, true) }}</span>
              <span class="fname grow ellipsis">{{ entry.path }}</span>
              <span class="fstat">
                <span v-if="entry.additions" class="add">+{{ entry.additions }}</span>
                <span v-if="entry.deletions" class="del">−{{ entry.deletions }}</span>
              </span>
            </div>
            <div v-if="store.stagedCount" class="faint small" style="margin-top: 4px">
              {{ t("workspace.commitTotals", { additions: totalAdditions, deletions: totalDeletions, unstaged: store.unstagedCount }) }}
            </div>
            <div v-else class="faint small">{{ t("workspace.nothingStaged") }}</div>
          </div>

          <div>
            <label class="small muted" style="display: block; margin-bottom: 5px">
              {{ t("workspace.messageLabel") }}
            </label>
            <n-input
              v-model:value="commitMessage"
              type="textarea"
              :rows="5"
              :placeholder="t('workspace.messagePlaceholder')"
            />
            <div class="faint small mt8">{{ t("workspace.draftNote") }}</div>
          </div>

          <n-checkbox v-model:checked="runHooks">{{ t("workspace.runHooks") }}</n-checkbox>

          <div class="commit-check">
            <div class="checkline">
              <span :class="store.stagedCount ? 'ok' : 'warn'">
                {{ store.stagedCount ? "✓" : "!" }}
              </span>
              <span>
                {{
                  store.stagedCount
                    ? t("workspace.checkStaged", { n: store.stagedCount })
                    : t("workspace.checkNotStaged")
                }}
              </span>
            </div>
            <div class="checkline">
              <span :class="commitMessage.trim() ? 'ok' : 'warn'">
                {{ commitMessage.trim() ? "✓" : "!" }}
              </span>
              <span>{{ commitMessage.trim() ? t("workspace.checkMessage") : t("workspace.checkNoMessage") }}</span>
            </div>
            <div v-if="store.unstagedCount" class="checkline">
              <span class="warn">!</span>
              <span>{{ t("workspace.checkUnstaged", { n: store.unstagedCount }) }}</span>
            </div>
          </div>

          <details class="explain">
            <summary>{{ t("workspace.commitExplainer") }}</summary>
            <p>
              {{ t("workspace.commitExplainerBody", { branch: store.currentBranch }) }}
            </p>
          </details>

          <div class="cmdbox" :title="cliCommand">
            <span class="text">{{ cliCommand }}</span>
            <button class="copy" @click="copy(cliCommand)">{{ t("common.copy") }}</button>
          </div>
        </div>

        <div class="commitfoot">
          <n-button type="primary" size="large" block :disabled="!canCommit" @click="doCommit(false)">
            {{ t("workspace.commitButton", { n: store.stagedCount }) }}
          </n-button>
          <div v-if="commitBlockReason" class="faint small" style="text-align: center">
            {{ commitBlockReason }}
          </div>
          <n-button block :disabled="!canAmend" @click="doCommit(true)">{{ t("workspace.amend") }}</n-button>
          <div class="faint small" style="text-align: center">
            {{ t("workspace.commitFootNote") }}
          </div>
        </div>
      </aside>
    </div>

    <GitignoreModal v-model:show="ignoreOpen" />
  </div>
</template>
