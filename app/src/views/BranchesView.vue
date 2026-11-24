<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { NButton, NCheckbox, NInput, useMessage } from "naive-ui";
import type { BranchInfo } from "@/ipc/types";
import { localeTag, t } from "@/lib/i18n";
import { useProblemReporter } from "@/lib/problems";
import { useRepoStore } from "@/stores/repo";

const store = useRepoStore();
const message = useMessage();
const report = useProblemReporter();

const selectedName = ref<string>(store.currentBranch);
const showCreate = ref(false);
const newName = ref("");
const checkoutAfter = ref(true);

const selectedBranch = computed<BranchInfo | null>(
  () => store.branches.find((b) => b.name === selectedName.value) ?? null,
);
const localBranches = computed(() => store.branches.filter((b) => !b.isRemote));
const remoteGroups = computed(() => {
  const map = new Map<string, BranchInfo[]>();
  for (const branch of store.branches.filter((b) => b.isRemote)) {
    const key = branch.remote ?? "remote";
    const list = map.get(key) ?? [];
    list.push(branch);
    map.set(key, list);
  }
  return [...map.entries()];
});

watch(
  () => store.branches,
  (list) => {
    if (!selectedName.value || !list.some((b) => b.name === selectedName.value)) {
      selectedName.value = store.currentBranch || list[0]?.name || "";
    }
  },
  { immediate: true },
);

async function checkout(branch: BranchInfo): Promise<void> {
  if (branch.isRemote || branch.isCurrent) return;
  try {
    await store.checkout(branch.name);
    selectedName.value = branch.name;
    message.success(t("branches.checkedOut", { name: branch.name }));
  } catch (error) {
    report(error);
  }
}

async function create(): Promise<void> {
  const name = newName.value.trim();
  if (!name) return;
  try {
    await store.createBranch(name, null, checkoutAfter.value);
    selectedName.value = name;
    newName.value = "";
    showCreate.value = false;
    message.success(t("branches.created", { name }));
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

function updateTime(ts: number): string {
  if (!ts) return "—";
  return new Date(ts * 1000).toLocaleString(localeTag(), { hour12: false });
}
</script>

<template>
  <div class="branches">
    <div class="bleft">
      <div class="bhead">
        <n-button size="small" type="primary" @click="showCreate = !showCreate">
          {{ t("branches.new") }}
        </n-button>
        <span class="right faint small">
          {{ t("branches.counts", { local: localBranches.length, remote: store.branches.length - localBranches.length }) }}
        </span>
      </div>

      <div v-if="showCreate" class="create-form">
        <n-input
          v-model:value="newName"
          size="small"
          :placeholder="t('branches.namePlaceholder')"
          @keyup.enter="create"
        />
        <n-checkbox v-model:checked="checkoutAfter">{{ t("branches.checkoutAfter") }}</n-checkbox>
        <div class="row">
          <n-button size="small" type="primary" :disabled="!newName.trim()" @click="create">
            {{ t("branches.create") }}
          </n-button>
          <n-button size="small" quaternary @click="showCreate = false">{{ t("common.cancel") }}</n-button>
          <span class="faint small">{{ t("branches.baseHead") }}</span>
        </div>
      </div>

      <div class="tree">
        <div class="tgroup">{{ t("branches.localGroup", { n: localBranches.length }) }}</div>
        <div
          v-for="branch in localBranches"
          :key="branch.fullName"
          class="tnode"
          :class="{ active: selectedName === branch.name, current: branch.isCurrent }"
          @click="selectedName = branch.name"
          @dblclick="checkout(branch)"
        >
          <span class="gicon">
            <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true">
              <circle cx="3" cy="3" r="1.5" fill="none" stroke="currentColor" />
              <circle cx="3" cy="9" r="1.5" fill="none" stroke="currentColor" />
              <circle cx="9" cy="4" r="1.5" fill="none" stroke="currentColor" />
              <path d="M3 4.5v3M9 5.5c0 2-2.2 2.2-4.3 2.8" fill="none" stroke="currentColor" />
            </svg>
          </span>
          <span class="bname grow ellipsis" :title="branch.name">{{ branch.name }}</span>
          <span v-if="branch.isCurrent" class="badge b-green">HEAD</span>
          <span v-if="branch.ahead" class="badge b-blue">↑ {{ branch.ahead }}</span>
          <span v-if="branch.behind" class="badge b-purple">↓ {{ branch.behind }}</span>
          <span v-if="branch.gone" class="badge b-red">gone</span>
          <button
            v-if="!branch.isCurrent"
            class="row-action"
            :title="t('branches.checkoutTooltip')"
            @click.stop="checkout(branch)"
          >
            {{ t("branches.checkout") }}
          </button>
        </div>

        <template v-for="group in remoteGroups" :key="group[0]">
          <div class="tgroup">{{ t("branches.remoteGroup", { remote: group[0], n: group[1].length }) }}</div>
          <div
            v-for="branch in group[1]"
            :key="branch.fullName"
            class="tnode"
            :class="{ active: selectedName === branch.name }"
            @click="selectedName = branch.name"
          >
            <span class="gicon">
              <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true">
                <circle cx="3" cy="3" r="1.5" fill="none" stroke="currentColor" />
                <circle cx="3" cy="9" r="1.5" fill="none" stroke="currentColor" />
                <circle cx="9" cy="4" r="1.5" fill="none" stroke="currentColor" />
                <path d="M3 4.5v3M9 5.5c0 2-2.2 2.2-4.3 2.8" fill="none" stroke="currentColor" />
              </svg>
            </span>
            <span class="bname grow ellipsis" :title="branch.name">{{ branch.name }}</span>
          </div>
        </template>

        <div class="tgroup">{{ t("branches.tagsGroup", { n: store.tags.length }) }}</div>
        <div
          v-for="tag in store.tags"
          :key="tag.name"
          class="tnode"
          :class="{ active: selectedName === `tag:${tag.name}` }"
          @click="selectedName = `tag:${tag.name}`"
        >
          <span class="gicon">🏷</span>
          <span class="bname grow ellipsis">{{ tag.name }}</span>
          <span class="chash">{{ tag.oid.slice(0, 7) }}</span>
        </div>
      </div>
    </div>

    <div class="bright">
      <div class="panel">
        <div class="panel-head">
          <b style="color: var(--text)">
            {{ selectedName.startsWith("tag:") ? t("branches.tag") : t("branches.detail") }}
          </b>
          <span v-if="selectedBranch" class="badge b-green">{{ selectedBranch.name }}</span>
          <span v-else-if="selectedName.startsWith('tag:')" class="badge b-yellow">
            {{ selectedName.slice(4) }}
          </span>
          <span class="grow"></span>
          <n-button
            v-if="selectedBranch && !selectedBranch.isCurrent && !selectedBranch.isRemote"
            size="small"
            type="primary"
            @click="checkout(selectedBranch)"
          >
            {{ t("branches.checkout") }}
          </n-button>
          <n-button
            v-if="selectedBranch"
            size="small"
            quaternary
            @click="copy(selectedBranch.name)"
          >
            {{ t("branches.copyName") }}
          </n-button>
        </div>
        <div class="panel-body">
          <template v-if="selectedBranch">
            <dl class="branch-facts">
              <dt>{{ t("branches.name") }}</dt>
              <dd class="mono">{{ selectedBranch.name }}</dd>
              <dt>{{ t("branches.type") }}</dt>
              <dd>{{ selectedBranch.isRemote ? t("branches.remoteTracking") : t("branches.local") }}</dd>
              <dt>{{ t("branches.current") }}</dt>
              <dd>{{ selectedBranch.isCurrent ? t("branches.currentYes") : t("branches.currentNo") }}</dd>
              <dt>{{ t("branches.commit") }}</dt>
              <dd class="mono">{{ selectedBranch.oid }}</dd>
              <dt>{{ t("branches.upstream") }}</dt>
              <dd class="mono">{{ selectedBranch.upstream ?? t("branches.notSet") }}</dd>
              <dt>{{ t("branches.tracking") }}</dt>
              <dd>{{ selectedBranch.track ?? "—" }}</dd>
              <dt>{{ t("branches.lastCommit") }}</dt>
              <dd>{{ updateTime(selectedBranch.updatedAt) }}</dd>
            </dl>
            <div class="note mt12">
              {{ t("branches.m0Note") }}
            </div>
            <div class="cmdbox mt12">
              <span class="text">git branch -vv --all</span>
              <button class="copy" @click="copy('git branch -vv --all')">{{ t("common.copy") }}</button>
            </div>
          </template>
          <template v-else-if="selectedName.startsWith('tag:')">
            <div class="faint small">
              {{ t("branches.tagPoints", { name: selectedName.slice(4) }) }}
              <span class="mono">
                {{ store.tags.find((t) => t.name === selectedName.slice(4))?.oid ?? "—" }}
              </span>
            </div>
            <div class="note mt12">{{ t("branches.tagLater") }}</div>
          </template>
          <div v-else class="faint small">{{ t("branches.pick") }}</div>
        </div>
      </div>

      <div class="panel">
        <div class="panel-head">
          <b style="color: var(--text)">{{ t("branches.whyTitle") }}</b>
        </div>
        <div class="panel-body faint small" style="line-height: 1.8">
          <p>
            {{ t("branches.whyBody1") }}
          </p>
          <p class="mt8">
            {{ t("branches.whyBody2") }}
          </p>
        </div>
      </div>
    </div>
  </div>
</template>
