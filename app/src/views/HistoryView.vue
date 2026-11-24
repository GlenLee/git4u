<script setup lang="ts">
import { computed, ref, shallowRef, watch } from "vue";
import { NButton, NInput, useMessage } from "naive-ui";
import { api } from "@/ipc";
import type { CommitDetail, CommitSummary } from "@/ipc/types";
import { relativeTime } from "@/lib/format";
import { buildGraph } from "@/lib/graph";
import { localeTag, t } from "@/lib/i18n";
import { useProblemReporter } from "@/lib/problems";
import { useRepoStore } from "@/stores/repo";

const store = useRepoStore();
const message = useMessage();
const report = useProblemReporter();

const query = ref("");
const selectedOid = ref<string | null>(null);
const detail = shallowRef<CommitDetail | null>(null);
const detailLoading = ref(false);

const filtered = computed(() => {
  const q = query.value.trim().toLowerCase();
  if (!q) return store.log;
  return store.log.filter(
    (commit) =>
      commit.summary.toLowerCase().includes(q) ||
      commit.shortOid.includes(q) ||
      commit.oid.includes(q) ||
      commit.authorName.toLowerCase().includes(q),
  );
});

const graph = computed(() => buildGraph(filtered.value));

watch(
  filtered,
  (list) => {
    if (!selectedOid.value || !list.some((commit) => commit.oid === selectedOid.value)) {
      selectedOid.value = list[0]?.oid ?? null;
      if (selectedOid.value) void loadDetail();
      else detail.value = null;
    }
  },
  { immediate: true },
);

async function loadDetail(): Promise<void> {
  if (!selectedOid.value || !store.activeId) return;
  detailLoading.value = true;
  try {
    detail.value = await api.detail(store.activeId, selectedOid.value);
  } catch (error) {
    detail.value = null;
    report(error);
  } finally {
    detailLoading.value = false;
  }
}

function select(commit: CommitSummary): void {
  if (selectedOid.value === commit.oid) return;
  selectedOid.value = commit.oid;
  void loadDetail();
}

const AVATAR_COLORS = ["#7c5cd6", "#3d8f5f", "#b0623d", "#4a6fd6", "#8f5c7d", "#3d7c8f"];

function avatarColor(name: string): string {
  let hash = 0;
  for (let i = 0; i < name.length; i++) hash = (hash * 31 + name.charCodeAt(i)) >>> 0;
  return AVATAR_COLORS[hash % AVATAR_COLORS.length];
}

function absoluteTime(ts: number): string {
  return new Date(ts * 1000).toLocaleString(localeTag(), { hour12: false });
}

function isTag(ref: string): boolean {
  return ref.startsWith("tag: ");
}

function refText(ref: string): string {
  if (ref.startsWith("HEAD -> ")) return `HEAD → ${ref.slice(8)}`;
  if (ref.startsWith("tag: ")) return `🏷 ${ref.slice(5)}`;
  return ref;
}

function refClass(ref: string): string {
  if (ref.startsWith("HEAD -> ")) return "b-blue";
  if (ref.includes("/")) return "b-purple";
  return "b-green";
}

function fileLetter(change: string): string {
  if (change === "added") return "A";
  if (change === "deleted") return "D";
  if (change === "renamed") return "R";
  if (change === "copied") return "C";
  return "M";
}

async function copy(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    message.success(t("common.copied"));
  } catch {
    message.warning(t("common.copyFailed"));
  }
}
</script>

<template>
  <div class="history">
    <div class="filters">
      <span class="badge plain">{{ store.currentBranch }}</span>
      <div style="width: 260px">
        <n-input v-model:value="query" size="small" clearable :placeholder="t('history.search')" />
      </div>
      <span class="faint small">{{ t("history.count", { shown: filtered.length, total: store.log.length }) }}</span>
      <span class="grow"></span>
      <n-button size="small" quaternary @click="store.refreshAll(true)">⟳ {{ t("common.refresh") }}</n-button>
    </div>

    <div class="logwrap">
      <div class="loglist">
        <div v-if="!filtered.length" class="empty">
          <div class="big">◌</div>
          <p>{{ store.log.length ? t("history.emptyFiltered") : t("history.empty") }}</p>
        </div>
        <div
          v-for="(commit, index) in filtered"
          :key="commit.oid"
          class="commit-row"
          :class="{ selected: commit.oid === selectedOid }"
          @click="select(commit)"
        >
          <svg
            class="graph"
            :width="graph.width"
            :height="graph.height"
            aria-hidden="true"
          >
            <line
              v-for="(line, lineIndex) in graph.rows[index].lines"
              :key="lineIndex"
              :x1="line.x1"
              :y1="line.y1"
              :x2="line.x2"
              :y2="line.y2"
              :stroke="line.color"
              stroke-width="2"
            />
            <circle
              :cx="graph.rows[index].lane * 18 + 13"
              cy="14"
              r="4.5"
              :fill="graph.rows[index].color"
            />
          </svg>
          <span class="chash" :title="commit.oid">{{ commit.shortOid }}</span>
          <span class="grow ellipsis" :title="commit.summary">{{ commit.summary }}</span>
          <template v-for="ref in commit.refs" :key="ref">
            <span v-if="isTag(ref)" class="tagchip">{{ refText(ref) }}</span>
            <span v-else class="badge" :class="refClass(ref)">{{ refText(ref) }}</span>
          </template>
          <span class="avatar" :style="{ background: avatarColor(commit.authorName) }">
            {{ commit.authorName.slice(0, 1) }}
          </span>
          <span class="time" :title="absoluteTime(commit.committedAt)">
            {{ relativeTime(commit.committedAt) }}
          </span>
        </div>
      </div>

      <aside class="detail">
        <div v-if="detailLoading" class="faint small">{{ t("history.loadingDetail") }}</div>
        <template v-else-if="detail">
          <div class="row mb8">
            <span class="chash">{{ detail.commit.shortOid }}</span>
            <span class="right time">{{ absoluteTime(detail.commit.committedAt) }}</span>
          </div>
          <h3 style="margin-top: 0">{{ detail.commit.summary }}</h3>
          <p v-if="detail.body" class="muted small" style="white-space: pre-wrap">
            {{ detail.body }}
          </p>
          <div class="row mt8 mb8">
            <span v-if="store.currentBranch" class="badge b-green">{{ store.currentBranch }}</span>
            <span class="badge plain">{{ t("history.parents", { n: detail.commit.parents.length }) }}</span>
          </div>
          <dl class="kv">
            <dt>{{ t("history.author") }}</dt>
            <dd>{{ detail.commit.authorName }} &lt;{{ detail.commit.authorEmail }}&gt;</dd>
            <dt>{{ t("history.commit") }}</dt>
            <dd class="mono" style="font-size: 11.5px">{{ detail.commit.oid }}</dd>
            <dt>{{ t("history.changes") }}</dt>
            <dd>
              {{ t("history.files", { n: detail.files.length }) }}
              <span class="fstat">
                <span class="add">+{{ detail.files.reduce((s, f) => s + f.additions, 0) }}</span>
                <span class="del">−{{ detail.files.reduce((s, f) => s + f.deletions, 0) }}</span>
              </span>
            </dd>
          </dl>
          <h3>{{ t("history.changedFiles") }}</h3>
          <div class="detail-files">
            <div v-for="file in detail.files" :key="file.path" class="fitem">
              <span class="ficon" :class="fileLetter(file.change) === 'A' ? 'a' : fileLetter(file.change) === 'D' ? 'd' : 'm'">
                {{ fileLetter(file.change) }}
              </span>
              <span class="fname grow ellipsis" :title="file.path">{{ file.path }}</span>
              <span class="fstat">
                <span v-if="file.additions" class="add">+{{ file.additions }}</span>
                <span v-if="file.deletions" class="del">−{{ file.deletions }}</span>
              </span>
            </div>
            <div v-if="!detail.files.length" class="faint small" style="padding: 8px 10px">
              {{ t("history.noFiles") }}
            </div>
          </div>
          <div class="detail-actions">
            <n-button size="small" @click="copy(detail.commit.oid)">{{ t("history.copyHash") }}</n-button>
            <n-button size="small" quaternary @click="copy(`git show ${detail.commit.shortOid}`)">
              {{ t("history.copyCommand") }}
            </n-button>
          </div>
          <div class="cmdbox mt12">
            <span class="text">git show {{ detail.commit.shortOid }}</span>
            <button class="copy" @click="copy(`git show ${detail.commit.shortOid}`)">{{ t("common.copy") }}</button>
          </div>
        </template>
        <div v-else class="faint small">{{ t("history.pickCommit") }}</div>
      </aside>
    </div>
  </div>
</template>
