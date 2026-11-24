import { createRouter, createWebHashHistory } from "vue-router";
import DashboardView from "@/views/DashboardView.vue";
import WorkspaceView from "@/views/WorkspaceView.vue";
import ToolsView from "@/views/ToolsView.vue";
import HistoryView from "@/views/HistoryView.vue";
import BranchesView from "@/views/BranchesView.vue";

const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: "/", redirect: "/workspace" },
    { path: "/dashboard", name: "dashboard", component: DashboardView },
    { path: "/workspace", name: "workspace", component: WorkspaceView },
    { path: "/tools", name: "tools", component: ToolsView },
    { path: "/history", name: "history", component: HistoryView },
    { path: "/branches", name: "branches", component: BranchesView },
    { path: "/settings", name: "settings", component: () => import("@/views/SettingsView.vue") },
    {
      path: "/settings-window",
      name: "settings-window",
      component: () => import("@/views/SettingsView.vue"),
    },
    { path: "/:pathMatch(.*)*", redirect: "/workspace" },
  ],
});

export default router;
