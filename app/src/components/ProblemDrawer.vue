<script setup lang="ts">
import { computed, ref } from "vue";
import { NButton, NDrawer, NDrawerContent, useMessage } from "naive-ui";
import { t } from "@/lib/i18n";
import {
  clearProblems,
  closeProblems,
  dismissProblem,
  drawerOpen,
  midStateGuide,
  problems,
  useProblemReporter,
  type ProblemAction,
} from "@/lib/problems";
import { useRepoStore } from "@/stores/repo";

const repo = useRepoStore();
const message = useMessage();
const report = useProblemReporter();
const busy = ref("");

const guide = computed(() =>
  repo.status?.midState ? midStateGuide(repo.status.midState) : null,
);

async function copy(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    message.success(t("common.copied"));
  } catch {
    message.warning(t("common.copyFailed"));
  }
}

async function run(problemId: number, action: ProblemAction): Promise<void> {
  busy.value = `${problemId}:${action.key}`;
  try {
    await action.run();
    if (action.dismissOnSuccess !== false) dismissProblem(problemId);
  } catch (error) {
    report(error);
  } finally {
    busy.value = "";
  }
}

async function refresh(): Promise<void> {
  try {
    await repo.refreshAll(true);
  } catch (error) {
    report(error);
  }
}
</script>

<template>
  <n-drawer
    :show="drawerOpen"
    :width="440"
    placement="right"
    @update:show="(value: boolean) => !value && closeProblems()"
  >
    <n-drawer-content :title="t('problem.title')" closable @close="closeProblems">
      <article v-if="guide" class="problem-card block">
        <div class="problem-head">
          <span class="dot"></span>
          <b>{{ guide.label }}</b>
        </div>
        <p class="hint">{{ t("problem.midState.detail") }}</p>
        <div v-if="guide.continueCmd" class="cmdrow">
          <span class="hint">{{ t("problem.midState.continue") }}</span>
          <code>{{ guide.continueCmd }}</code>
          <button class="link" @click="copy(guide.continueCmd)">{{ t("common.copy") }}</button>
        </div>
        <div v-if="guide.abortCmd" class="cmdrow">
          <span class="hint">{{ t("problem.midState.abort") }}</span>
          <code>{{ guide.abortCmd }}</code>
          <button class="link" @click="copy(guide.abortCmd)">{{ t("common.copy") }}</button>
        </div>
        <div class="cmdrow">
          <span class="hint">{{ t("problem.midState.status") }}</span>
          <code>{{ guide.statusCmd }}</code>
          <button class="link" @click="copy(guide.statusCmd)">{{ t("common.copy") }}</button>
        </div>
        <div>
          <n-button size="small" @click="refresh">{{ t("problem.refresh") }}</n-button>
        </div>
      </article>

      <div v-if="!problems.length && !guide" class="empty">{{ t("problem.empty") }}</div>

      <article v-for="problem in problems" :key="problem.id" class="problem-card">
        <div class="problem-head">
          <b>{{ problem.title }}</b>
          <span class="grow"></span>
          <button class="link" @click="dismissProblem(problem.id)">
            {{ t("problem.dismiss") }}
          </button>
        </div>

        <div v-if="problem.hints.length" class="hint">
          <div v-for="hint in problem.hints" :key="hint">· {{ hint }}</div>
        </div>

        <div v-if="problem.actions.length" class="actions">
          <n-button
            v-for="action in problem.actions"
            :key="action.key"
            size="small"
            :type="action.risk === 'dangerous' ? 'error' : action.risk === 'safe' ? 'primary' : 'default'"
            :loading="busy === `${problem.id}:${action.key}`"
            @click="run(problem.id, action)"
          >
            {{ action.label }}
          </n-button>
        </div>

        <details v-if="problem.causes.length || problem.kind">
          <summary class="hint">{{ t("problem.details") }}</summary>
          <div v-if="problem.kind" class="hint">kind: {{ problem.kind }}</div>
          <pre v-if="problem.causes.length">{{ problem.causes.join("\n") }}</pre>
        </details>

        <div v-if="problem.cli" class="cmdrow">
          <code>{{ problem.cli }}</code>
          <button class="link" @click="copy(problem.cli)">{{ t("common.copy") }}</button>
        </div>
      </article>

      <template #footer>
        <div class="foot">
          <n-button v-if="problems.length" size="small" quaternary @click="clearProblems">
            {{ t("problem.clear") }}
          </n-button>
          <span class="grow"></span>
          <n-button size="small" @click="closeProblems">{{ t("common.done") }}</n-button>
        </div>
      </template>
    </n-drawer-content>
  </n-drawer>
</template>

<style scoped>
.problem-card {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 12px;
  margin-bottom: 10px;
  border: 1px solid var(--border);
  border-radius: 4px;
  background: var(--bg-panel);
}
.problem-card.block {
  border-color: var(--warning);
}
.problem-head {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12.5px;
}
.problem-head b {
  overflow-wrap: anywhere;
}
.dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  flex: none;
  background: var(--warning);
}
.hint {
  margin: 0;
  font-size: 11.5px;
  line-height: 1.6;
  color: var(--text-faint);
}
.actions {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}
.cmdrow {
  display: flex;
  align-items: center;
  gap: 8px;
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
pre {
  margin: 4px 0 0;
  padding: 8px;
  max-height: 180px;
  overflow: auto;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  border-radius: 3px;
  background: var(--bg-sunken);
  color: var(--text-muted);
  font-family: var(--mono);
  font-size: 11px;
  line-height: 1.6;
}
.empty {
  padding: 24px 4px;
  color: var(--text-faint);
  font-size: 12px;
}
.foot {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
}
</style>
