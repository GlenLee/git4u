import type { CommitSummary } from "@/ipc/types";

const LANE_WIDTH = 18;
const PAD = 13;
const ROW_HEIGHT = 28;
const COLORS = [
  "#63e2b7",
  "#b79aef",
  "#f2c97d",
  "#70c0e8",
  "#f0a06a",
  "#e88080",
  "#7bd88f",
  "#c792ea",
];

export interface GraphLine {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
  color: string;
}

export interface GraphRow {
  lane: number;
  color: string;
  lines: GraphLine[];
}

export interface GraphLayout {
  rows: GraphRow[];
  lanes: number;
  width: number;
  height: number;
}

const x = (lane: number): number => PAD + lane * LANE_WIDTH;

function colorFor(oid: string): string {
  let hash = 0xcbf29ce484222325n;
  for (let i = 0; i < oid.length; i++) {
    hash ^= BigInt(oid.charCodeAt(i));
    hash = (hash * 0x100000001b3n) & 0xffffffffffffffffn;
  }
  return COLORS[Number(hash % BigInt(COLORS.length))];
}

function vertical(lane: number, y1: number, y2: number, color: string): GraphLine {
  return { x1: x(lane), y1, x2: x(lane), y2, color };
}

/**
 * 与 `git log --graph` 等价的 lane 分配（单次全量版）。
 * 每个 lane 代表一条尚未落地的父边；提交占用其子提交预留的 lane。
 */
export function buildGraph(commits: CommitSummary[]): GraphLayout {
  type Active = { oid: string; color: string } | null;
  const active: Active[] = [];
  const rows: GraphRow[] = [];
  let maxLanes = 1;

  for (const commit of commits) {
    let lane = active.findIndex((a) => a?.oid === commit.oid);
    if (lane < 0) {
      lane = active.findIndex((a) => a === null);
      if (lane < 0) lane = active.length;
      active[lane] = { oid: commit.oid, color: colorFor(commit.oid) };
    }

    const color = active[lane]!.color;
    const before = active.slice();
    active[lane] = null;

    const parentLanes: number[] = [];
    for (let idx = 0; idx < commit.parents.length; idx++) {
      const parent = commit.parents[idx];
      let parentLane = active.findIndex((a) => a?.oid === parent);
      if (parentLane < 0) {
        if (idx === 0 && active[lane] === null) {
          parentLane = lane;
        } else {
          parentLane = active.findIndex((a) => a === null);
          if (parentLane < 0) parentLane = active.length;
        }
        active[parentLane] = { oid: parent, color: colorFor(parent) };
      }
      parentLanes.push(parentLane);
    }

    while (active.length > 0 && active[active.length - 1] === null) active.pop();

    const lanes = Math.max(active.length, lane + 1);
    maxLanes = Math.max(maxLanes, lanes);
    const lines: GraphLine[] = [];

    for (let i = 0; i < lanes; i++) {
      if (i === lane) continue;
      if (before[i] && active[i]) {
        lines.push(vertical(i, 0, ROW_HEIGHT, before[i]!.color));
      }
    }
    lines.push(vertical(lane, 0, ROW_HEIGHT / 2, color));
    parentLanes.forEach((parentLane, idx) => {
      lines.push({
        x1: x(lane),
        y1: ROW_HEIGHT / 2,
        x2: x(parentLane),
        y2: ROW_HEIGHT,
        color: idx === 0 ? color : colorFor(commit.parents[idx]),
      });
    });

    rows.push({ lane, color, lines });
  }

  return {
    rows,
    lanes: maxLanes,
    width: PAD * 2 + (maxLanes - 1) * LANE_WIDTH + 10,
    height: ROW_HEIGHT,
  };
}
