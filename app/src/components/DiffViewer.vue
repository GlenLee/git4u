<script setup lang="ts">
import { computed } from "vue";
import type { DiffLine, FileDiff } from "@/ipc/types";
import { t } from "@/lib/i18n";

const props = defineProps<{
  diff: FileDiff | null;
  loading: boolean;
  mode: "side" | "unified";
}>();

interface Pair {
  left: DiffLine | null;
  right: DiffLine | null;
}

interface Block {
  header: string;
  pairs: Pair[];
  lines: DiffLine[];
}

const blocks = computed<Block[]>(() => {
  const diff = props.diff;
  if (!diff) return [];
  return diff.hunks.map((hunk) => {
    const pairs: Pair[] = [];
    let dels: DiffLine[] = [];
    let adds: DiffLine[] = [];

    const flush = () => {
      const count = Math.max(dels.length, adds.length);
      for (let i = 0; i < count; i++) {
        pairs.push({ left: dels[i] ?? null, right: adds[i] ?? null });
      }
      dels = [];
      adds = [];
    };

    for (const line of hunk.lines) {
      if (line.kind === "del") {
        dels.push(line);
      } else if (line.kind === "add") {
        adds.push(line);
      } else {
        flush();
        pairs.push({ left: line, right: line });
      }
    }
    flush();

    return { header: hunk.header, pairs, lines: hunk.lines };
  });
});

function sideClass(line: DiffLine | null): string {
  if (!line) return "blank";
  return line.kind === "add" ? "add" : line.kind === "del" ? "del" : "";
}

function prefix(kind: DiffLine["kind"]): string {
  return kind === "add" ? "+" : kind === "del" ? "-" : " ";
}
</script>

<template>
  <div v-if="loading" class="diff-empty">{{ t("diff.loading") }}</div>
  <div v-else-if="!diff" class="diff-empty">{{ t("diff.pick") }}</div>
  <div v-else-if="diff.binary" class="diff">
    <div class="dhead"><span class="mono">{{ diff.path }}</span><span class="faint small">{{ t("diff.binary") }}</span></div>
    <div class="binary-tip">{{ t("diff.binaryTip") }}</div>
  </div>
  <div v-else-if="!blocks.length" class="diff-empty">
    {{ t("diff.noText") }}
  </div>
  <div v-else class="diff">
    <div class="dhead">
      <span class="mono">{{ diff.path }}</span>
      <span class="fstat">
        <span class="add">+{{ diff.additions }}</span>
        <span class="del">−{{ diff.deletions }}</span>
      </span>
    </div>

    <template v-if="mode === 'side'">
      <div class="d2head">
        <span>{{ t("diff.before") }}</span>
        <span>{{ t("diff.after") }}</span>
      </div>
      <template v-for="(block, blockIndex) in blocks" :key="blockIndex">
        <div class="dhunk">{{ block.header }}</div>
        <div v-for="(pair, pairIndex) in block.pairs" :key="pairIndex" class="d2row">
          <div class="d2side" :class="sideClass(pair.left)">
            <span class="ln">{{ pair.left?.oldLineno ?? "" }}</span>
            <span class="code">{{ pair.left ? pair.left.content || " " : " " }}</span>
          </div>
          <div class="d2side" :class="sideClass(pair.right)">
            <span class="ln">{{ pair.right?.newLineno ?? "" }}</span>
            <span class="code">{{ pair.right ? pair.right.content || " " : " " }}</span>
          </div>
        </div>
      </template>
    </template>

    <template v-else>
      <template v-for="(block, blockIndex) in blocks" :key="blockIndex">
        <div class="dhunk">{{ block.header }}</div>
        <div v-for="(line, lineIndex) in block.lines" :key="lineIndex" class="dline" :class="line.kind">
          <span class="lnum">{{ line.kind === "del" ? line.oldLineno : line.newLineno }}</span>
          <span>{{ prefix(line.kind) }}{{ line.content || " " }}</span>
        </div>
      </template>
    </template>
  </div>
</template>
