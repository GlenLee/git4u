import { computed, h, ref } from "vue";
import { useMessage } from "naive-ui";
import { useRouter } from "vue-router";
import { openSettingsWindow } from "@/ipc";
import type { G4uError, Suggestion } from "@/ipc/types";
import { midStateLabel, t } from "@/lib/i18n";
import { useRepoStore } from "@/stores/repo";

export type ProblemRisk = "safe" | "caution" | "dangerous";

export interface ProblemAction {
  key: string;
  label: string;
  risk: ProblemRisk;
  /** 后端建议的目标，例如凭证 id。 */
  target: string | null;
  /** 动作成功后是否从列表移除；导航类动作保留问题，等用户处理完再刷新。 */
  dismissOnSuccess?: boolean;
  run: () => void | Promise<unknown>;
}

export interface Problem {
  id: number;
  kind: string | null;
  title: string;
  causes: string[];
  cli: string | null;
  actions: ProblemAction[];
  /** 没有前端动作的原始建议，按提示文案展示。 */
  hints: string[];
  at: number;
}

interface ErrorDescription {
  kind: string | null;
  title: string;
  causes: string[];
  cli: string | null;
  suggestions: Suggestion[];
}

const MAX_PROBLEMS = 20;

const problems = ref<Problem[]>([]);
const drawerOpen = ref(false);
let nextId = 1;

export { drawerOpen, problems };

export const problemCount = computed(() => problems.value.length);

export function openProblems(): void {
  drawerOpen.value = true;
}

export function closeProblems(): void {
  drawerOpen.value = false;
}

export function dismissProblem(id: number): void {
  problems.value = problems.value.filter((problem) => problem.id !== id);
}

export function clearProblems(): void {
  problems.value = [];
}

/** 把任意错误收敛成可展示、可执行的问题。 */
export function describeError(error: unknown): ErrorDescription {
  if (error && typeof error === "object" && "human" in error) {
    const shaped = error as G4uError;
    return {
      kind: shaped.kind ?? null,
      title: shaped.human || t("common.operationFailed"),
      causes: (shaped.causeChain ?? []).filter(Boolean),
      cli: shaped.cliEquivalent ?? null,
      suggestions: shaped.suggestions ?? [],
    };
  }
  const title =
    error instanceof Error
      ? error.message
      : typeof error === "string"
        ? error
        : t("common.operationFailedRetry");
  return { kind: null, title, causes: [], cli: null, suggestions: [] };
}

interface ActionContext {
  repo: ReturnType<typeof useRepoStore>;
  router: ReturnType<typeof useRouter>;
}

const actions: Record<
  string,
  (
    context: ActionContext,
    target: string | null,
  ) => {
    label: string;
    /** 导航类动作不把问题标记为已解决。 */
    keepProblem?: boolean;
    run: () => void | Promise<unknown>;
  }
> = {
  "repo.refresh": ({ repo }) => ({
    label: t("problem.action.refresh"),
    run: () => repo.refreshAll(true),
  }),
  "repo.pick": ({ repo }) => ({
    label: t("problem.action.pick"),
    run: () => repo.openRepo(),
  }),
  "app.retry": () => ({
    label: t("problem.action.retry"),
    run: () => window.location.reload(),
  }),
  "credentials.update": ({ repo, router }, target) => ({
    label: t("problem.action.updateCredential"),
    keepProblem: true,
    run: async () => {
      if (await openSettingsWindow(repo.activeId, "credentials", target)) return;
      await router.push({
        path: "/settings",
        query: {
          section: "credentials",
          ...(target ? { credential: target } : {}),
          focus: String(Date.now()),
        },
      });
    },
  }),
};

/** 视图里统一用它报错：toast 只留一行，完整内容进问题抽屉。 */
export function useProblemReporter() {
  const message = useMessage();
  const repo = useRepoStore();
  const router = useRouter();

  return function report(error: unknown): Problem {
    const description = describeError(error);
    const resolved: ProblemAction[] = [];
    const hints: string[] = [];
    for (const suggestion of description.suggestions) {
      const target = suggestion.target ?? null;
      const handler = actions[suggestion.action]?.({ repo, router }, target);
      if (handler) {
        resolved.push({
          key: suggestion.action,
          label: handler.label,
          risk: suggestion.risk,
          target,
          dismissOnSuccess: !handler.keepProblem,
          run: handler.run,
        });
      } else {
        hints.push(suggestion.label);
      }
    }

    const problem: Problem = {
      id: nextId++,
      kind: description.kind,
      title: description.title,
      causes: description.causes,
      cli: description.cli,
      actions: resolved,
      hints,
      at: Date.now(),
    };
    problems.value = [problem, ...problems.value].slice(0, MAX_PROBLEMS);

    message.error(
      () =>
        h("span", [
          problem.title,
          h("button", { class: "problem-toast-action", onClick: openProblems }, t("problem.open")),
        ]),
      { duration: 8000, closable: true },
    );
    return problem;
  };
}

export interface MidStateGuide {
  label: string;
  continueCmd: string | null;
  abortCmd: string | null;
  statusCmd: string;
}

// shortcut: midState 目前是 g4u-core 返回的中文文案，按值映射；后端改成 code 后删掉这里的分支。
export function midStateGuide(state: string): MidStateGuide {
  const label = midStateLabel(state);
  if (state === "合并中") {
    return {
      label,
      continueCmd: "git commit --no-edit",
      abortCmd: "git merge --abort",
      statusCmd: "git status",
    };
  }
  if (state === "rebase 中") {
    return {
      label,
      continueCmd: "git rebase --continue",
      abortCmd: "git rebase --abort",
      statusCmd: "git status",
    };
  }
  if (state === "cherry-pick 中") {
    return {
      label,
      continueCmd: "git cherry-pick --continue",
      abortCmd: "git cherry-pick --abort",
      statusCmd: "git status",
    };
  }
  if (state === "revert 中") {
    return {
      label,
      continueCmd: "git revert --continue",
      abortCmd: "git revert --abort",
      statusCmd: "git status",
    };
  }
  if (state === "bisect 中") {
    return { label, continueCmd: null, abortCmd: "git bisect reset", statusCmd: "git bisect log" };
  }
  return { label, continueCmd: null, abortCmd: null, statusCmd: "git status" };
}
