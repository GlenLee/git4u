import { describe, expect, it } from "vitest";
import { buildGraph } from "./graph";
import type { CommitSummary } from "@/ipc/types";

function commit(oid: string, parents: string[]): CommitSummary {
  return {
    oid,
    shortOid: oid.slice(0, 7),
    parents,
    summary: oid,
    authorName: "test",
    authorEmail: "test@example.com",
    committedAt: 0,
    refs: [],
  };
}

const linear = [
  commit("c5", ["c4"]),
  commit("c4", ["c3"]),
  commit("c3", ["c2"]),
  commit("c2", ["c1"]),
  commit("c1", []),
];

const withMerge = [
  commit("m3", ["m2"]),
  commit("m2", ["m1", "b1"]),
  commit("b1", ["m1"]),
  commit("m1", ["base"]),
  commit("base", []),
];

describe("buildGraph", () => {
  it("keeps a linear history on one lane", () => {
    const layout = buildGraph(linear);
    expect(layout.rows).toHaveLength(linear.length);
    expect(layout.lanes).toBe(1);
    expect(layout.rows.every((row) => row.lane === 0)).toBe(true);
    expect(layout.rows[4].lines).toHaveLength(1); // 根提交只有上半段
  });

  it("opens a second lane for a merge and closes it after the branch tip", () => {
    const layout = buildGraph(withMerge);
    expect(layout.lanes).toBeGreaterThanOrEqual(2);
    expect(layout.rows[1].lines.some((line) => line.x2 !== line.x1)).toBe(true);
    expect(layout.rows.every((row) => row.lane < layout.lanes)).toBe(true);
  });

  it("never emits NaN coordinates", () => {
    const layout = buildGraph(withMerge);
    for (const row of layout.rows) {
      for (const line of row.lines) {
        expect(Number.isFinite(line.x1 + line.x2 + line.y1 + line.y2)).toBe(true);
      }
    }
  });
});
