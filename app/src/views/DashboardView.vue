<script setup lang="ts">
import { computed } from "vue";
import { NButton, useMessage } from "naive-ui";
import { useRouter } from "vue-router";
import { formatBytes, relativeTime } from "@/lib/format";
import { localeTag, t } from "@/lib/i18n";
import { useProblemReporter } from "@/lib/problems";
import { useRepoStore } from "@/stores/repo";

const repo = useRepoStore();
const router = useRouter();
const message = useMessage();
const report = useProblemReporter();

const totals = computed(() =>
  repo.repos.reduce(
    (sum, item) => {
      if (!item.valid) return sum;
      sum.size += item.stats?.sizeBytes ?? 0;
      sum.dirty += item.staged + item.unstaged;
      return sum;
    },
    { size: 0, dirty: 0 },
  ),
);

async function enter(id: string): Promise<void> {
  await repo.activate(id);
  void router.push("/workspace");
}

async function open(): Promise<void> {
  try {
    const result = await repo.openRepo();
    if (result === "preview") {
      message.info(t("app.previewOnlyShort"), { duration: 5000 });
    } else if (result === "opened") {
      void router.push("/workspace");
    }
  } catch (error) {
    report(error);
  }
}
</script>

<template>
  <div class="dashboard">
    <template v-if="repo.repos.length">
      <div class="head">
        <b>{{ t("dashboard.repos") }}</b>
        <span class="badge plain">{{ repo.repos.length }}</span>
        <span class="faint small">
          {{ t("dashboard.totals", { size: formatBytes(totals.size), n: totals.dirty }) }}
        </span>
        <span class="grow"></span>
        <n-button size="small" @click="open">{{ t("app.openLocalRepo") }}</n-button>
      </div>
      <div class="cardgrid">
        <div
          v-for="item in repo.repos"
          :key="item.id"
          class="repo-card"
          :style="!item.valid ? 'opacity:.65' : ''"
          @click="item.valid && enter(item.id)"
        >
          <div class="row">
            <span class="rname">{{ item.name }}</span>
            <span class="badge" :class="item.valid ? 'b-green' : 'plain'">
              {{ item.valid ? (item.head?.branch ?? "detached") : t("dashboard.invalidPath") }}
            </span>
            <span class="right time">
              {{
                item.stats?.lastCommitAt
                  ? relativeTime(item.stats.lastCommitAt)
                  : item.valid
                    ? t("dashboard.opened")
                    : "—"
              }}
            </span>
          </div>
          <div class="rpath" :title="item.path">{{ item.path }}</div>
          <div v-if="item.valid" class="badgerow">
            <span v-if="item.staged + item.unstaged" class="badge b-yellow">
              {{ t("dashboard.uncommitted", { n: item.staged + item.unstaged }) }}
            </span>
            <span v-if="item.ahead" class="badge b-blue">↑ {{ item.ahead }}</span>
            <span v-if="item.behind" class="badge b-purple">↓ {{ item.behind }}</span>
            <span v-if="item.conflicts" class="badge b-red">{{ t("dashboard.conflicts", { n: item.conflicts }) }}</span>
            <span v-if="item.midState" class="badge b-yellow">⟳ {{ item.midState }}</span>
            <span v-if="!item.staged && !item.unstaged && !item.conflicts && !item.midState" class="badge plain">
              {{ t("dashboard.clean") }}
            </span>
          </div>
          <div v-else class="badgerow">
            <span class="badge plain">⚠ {{ item.error || t("dashboard.relocate") }}</span>
          </div>

          <template v-if="item.stats">
            <div class="repo-stats">
              <div class="stat" :title="t('dashboard.sizeHint')">
                <span class="k">{{ t("dashboard.size") }}</span>
                <span class="v">{{ formatBytes(item.stats.sizeBytes) }}</span>
              </div>
              <div class="stat">
                <span class="k">{{ t("dashboard.commits") }}</span>
                <span class="v">{{ item.stats.commitCount.toLocaleString(localeTag()) }}</span>
              </div>
              <div class="stat">
                <span class="k">{{ t("dashboard.branches") }}</span>
                <span class="v">{{ item.stats.branchCount }}</span>
              </div>
              <div class="stat">
                <span class="k">{{ t("dashboard.tags") }}</span>
                <span class="v">{{ item.stats.tagCount }}</span>
              </div>
            </div>
            <div v-if="item.stats.lastCommitSummary" class="lastcommit" :title="item.stats.lastCommitSummary">
              <span class="k">{{ t("dashboard.lastCommit") }}</span>
              <span class="ellipsis">{{ item.stats.lastCommitSummary }}</span>
            </div>
          </template>
        </div>
      </div>
    </template>

    <div v-else class="empty">
      <div class="big">⎇</div>
      <h2>{{ t("dashboard.emptyTitle") }}</h2>
      <p>
        {{ t("dashboard.emptyBody") }}
      </p>
      <n-button type="primary" @click="open">{{ t("dashboard.openCta") }}</n-button>
      <p class="small">
        {{ t("dashboard.emptyNote") }}
      </p>
    </div>
  </div>
</template>
