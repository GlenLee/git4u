<script setup lang="ts">
import { computed, defineAsyncComponent } from "vue";
import { NConfigProvider, NDialogProvider, NMessageProvider, darkTheme } from "naive-ui";
import { useRoute } from "vue-router";
import AppShell from "@/components/AppShell.vue";
import ProblemDrawer from "@/components/ProblemDrawer.vue";
import { useUiStore } from "@/stores/repo";

const SettingsView = defineAsyncComponent(() => import("@/views/SettingsView.vue"));
const ui = useUiStore();
const route = useRoute();
const standaloneSettings = computed(() => route.path === "/settings-window");
</script>

<template>
  <n-config-provider
    :theme="ui.isDark ? darkTheme : null"
    :theme-overrides="ui.overrides"
    style="height: 100%"
  >
    <n-message-provider>
      <n-dialog-provider>
        <SettingsView v-if="standaloneSettings" />
        <AppShell v-else />
        <ProblemDrawer />
      </n-dialog-provider>
    </n-message-provider>
  </n-config-provider>
</template>
