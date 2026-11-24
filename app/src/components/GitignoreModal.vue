<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { NButton, NCheckbox, NModal, NPopconfirm, useMessage } from "naive-ui";
import { api } from "@/ipc";
import { GITIGNORE_TEMPLATES, gitignoreBlock } from "@/lib/gitignoreTemplates";
import { t, type MessageKey } from "@/lib/i18n";
import { useProblemReporter } from "@/lib/problems";
import { useRepoStore } from "@/stores/repo";

const show = defineModel<boolean>("show", { required: true });

const store = useRepoStore();
const message = useMessage();
const report = useProblemReporter();

const loading = ref(false);
const exists = ref(false);
const base = ref("");
const append = ref(false);
const selected = ref<string[]>([]);

const newContent = computed(() =>
  GITIGNORE_TEMPLATES.filter((template) => selected.value.includes(template.id))
    .map(gitignoreBlock)
    .join("\n"),
);
const preview = computed(() => {
  const block = newContent.value;
  if (!append.value || !base.value.trim()) return block;
  return `${base.value.replace(/\s*$/, "")}\n\n${block}`;
});
const canSave = computed(() => !loading.value && newContent.value.trim().length > 0);
const actionLabel = computed(() => {
  if (!exists.value) return t("workspace.ignoreCreate");
  return append.value ? t("workspace.ignoreAppendAction") : t("workspace.ignoreOverwrite");
});

function toggleTemplate(id: string, checked: boolean): void {
  selected.value = checked
    ? [...selected.value, id]
    : selected.value.filter((item) => item !== id);
}

async function load(): Promise<void> {
  if (!store.activeId) return;
  loading.value = true;
  selected.value = [];
  try {
    const file = await api.gitignore(store.activeId);
    exists.value = file.exists;
    base.value = file.content;
    append.value = file.exists;
  } catch (error) {
    exists.value = false;
    base.value = "";
    append.value = false;
    report(error);
  } finally {
    loading.value = false;
  }
}

async function save(): Promise<void> {
  if (!store.activeId || !canSave.value) return;
  try {
    await api.writeGitignore(store.activeId, newContent.value, append.value);
    message.success(
      exists.value && append.value
        ? t("workspace.gitignoreAppended")
        : t("workspace.gitignoreCreated"),
    );
    show.value = false;
    await store.refreshStatus();
  } catch (error) {
    report(error);
  }
}

watch(show, (open) => {
  if (open) void load();
});
</script>

<template>
  <n-modal v-model:show="show" preset="card" :title="t('workspace.ignoreTitle')" style="width: min(680px, 92vw)">
    <div class="ignore-body">
      <p class="faint small" style="margin: 0">
        {{ t("workspace.ignoreIntro") }}
      </p>
      <div class="ignore-templates">
        <n-checkbox
          v-for="template in GITIGNORE_TEMPLATES"
          :key="template.id"
          :checked="selected.includes(template.id)"
          @update:checked="(checked) => toggleTemplate(template.id, checked)"
        >
          <b>{{ template.name }}</b>
          <span class="ignore-hint">{{ t(template.hint as MessageKey) }}</span>
        </n-checkbox>
      </div>

      <n-checkbox v-if="exists" v-model:checked="append">
        {{ t("workspace.ignoreAppend") }}
      </n-checkbox>
      <div v-if="exists && !append" class="ignore-warn">
        {{ t("workspace.ignoreOverwriteWarn") }}
      </div>

      <label class="small muted">{{ t("workspace.ignoreContent") }}</label>
      <pre class="ignore-preview">{{ loading ? t("common.loading") : preview || t("workspace.ignoreEmpty") }}</pre>
    </div>
    <template #footer>
      <div class="ignore-foot">
        <span class="faint small">{{ t("workspace.ignoreFoot") }}</span>
        <span class="grow"></span>
        <n-button size="small" @click="show = false">{{ t("common.cancel") }}</n-button>
        <n-popconfirm
          v-if="exists && !append"
          :positive-text="t('workspace.ignoreOverwrite')"
          :negative-text="t('common.cancel')"
          @positive-click="save"
        >
          <template #trigger>
            <n-button size="small" type="primary" :disabled="!canSave" :loading="loading">
              {{ t("workspace.ignoreOverwrite") }}
            </n-button>
          </template>
          {{ t("workspace.ignoreOverwriteConfirm") }}
        </n-popconfirm>
        <n-button
          v-else
          size="small"
          type="primary"
          :disabled="!canSave"
          :loading="loading"
          @click="save"
        >
          {{ actionLabel }}
        </n-button>
      </div>
    </template>
  </n-modal>
</template>

<style scoped>
.ignore-body {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.ignore-templates {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(190px, 1fr));
  gap: 8px 14px;
}
.ignore-hint {
  margin-left: 6px;
  font-size: 11.5px;
  color: var(--text-faint);
}
.ignore-warn {
  font-size: 12px;
  color: var(--warning);
}
.ignore-preview {
  margin: 0;
  max-height: 220px;
  overflow: auto;
  padding: 10px 12px;
  border: 1px solid var(--border);
  border-radius: 3px;
  background: var(--bg-sunken);
  color: var(--text-muted);
  font-family: var(--mono);
  font-size: 12px;
  line-height: 1.6;
  white-space: pre-wrap;
}
.ignore-foot {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
}
</style>
