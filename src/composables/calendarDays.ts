// 日历视图的数据层：把便签摊到某个月的格子上，并算出哪些能连成横贯条。
//
// 便签没有「日程日期」这个字段，所以归属只有两条规则：有提醒看提醒，没提醒看创建。
// 重复规则要投影成多天，锚定和月末夹取跟后端 db.rs 的 next_remind_at 同口径，
// 否则日历上标出来的重复日和真正会响的那天不是同一天。
import type { Memo } from "./useMemos";
import { contentPreview } from "../utils";

/** 一个格子纵向最多能放几条（横贯条占的道也算在内），与 style.css 的格子高度对应 */
export const CELL_LANES = 3;

export interface DayCell {
  /** YYYY-MM-DD */
  key: string;
  day: number;
  /** 上/下旬补位用的邻月格子 */
  inMonth: boolean;
  weekend: boolean;
  /** 本格自己显示的单日便签（已按时间排好、已裁掉放不下的） */
  bars: Memo[];
  /** 被格子高度挤掉、没显示出来的条数 */
  more: number;
  /** 有几条横贯条压在上方，本格的内容要让出这么多道 */
  laneOffset: number;
}

/** 连续重复项在一周内连成的一条横贯带 */
export interface Band {
  memo: Memo;
  row: number;
  /** 0..6，周一为 0 */
  col: number;
  span: number;
  /** 第几道，从 0 开始 */
  lane: number;
  label: string;
}

export interface MonthLayout {
  rows: number;
  cells: DayCell[];
  bands: Band[];
  /** 本月出现过的便签条数（重复项只算一条） */
  total: number;
  /** 其中未完成的条数 */
  open: number;
}

const pad = (n: number) => String(n).padStart(2, "0");

export function dateKey(y: number, m: number, d: number): string {
  return `${y}-${pad(m + 1)}-${pad(d)}`;
}

/** 这条便签挂在哪个日期上：提醒优先，其次创建 */
export function baseKey(m: Memo): string {
  return (m.remind_at || m.created_at || "").slice(0, 10);
}

function weekdayOf(key: string): number {
  return new Date(+key.slice(0, 4), +key.slice(5, 7) - 1, +key.slice(8, 10)).getDay();
}

/** 该便签是否出现在这一天。日期串定宽，字典序就是时间序，不必到处 new Date */
export function hitsDate(m: Memo, key: string): boolean {
  const base = baseKey(m);
  if (!base || !key || key < base) return false;
  const rule = m.remind_repeat || "";
  if (!rule) return key === base;
  if (rule === "daily") return true;
  if (rule === "weekly") return weekdayOf(key) === weekdayOf(base);
  if (rule.startsWith("monthly:")) {
    const d = +key.slice(8, 10);
    const anchor = +rule.slice(8) || +base.slice(8, 10);
    const last = new Date(+key.slice(0, 4), +key.slice(5, 7), 0).getDate();
    // 月末夹取与后端 days_in_month 一致：锚定 31 号在 2 月落在当月最后一天
    return d === Math.min(anchor, last);
  }
  return false;
}

/** 有提醒的按提醒时刻排，没提醒的沉到当天末尾 */
function compareAt(a: Memo, b: Memo): number {
  const ta = a.remind_at || "9999-12-31 99:99";
  const tb = b.remind_at || "9999-12-31 99:99";
  if (ta !== tb) return ta < tb ? -1 : 1;
  return a.created_at < b.created_at ? -1 : 1;
}

/** 某一天（含重复投影）的全部便签，供当日清单用 */
export function memosOn(memos: Memo[], key: string): Memo[] {
  return memos.filter((m) => hitsDate(m, key)).sort(compareAt);
}

export function barLabel(m: Memo): string {
  return contentPreview(m.content, 12);
}

/** 整月空的时候给一个出口：跳到离当前月最近的有便签的那个月 */
export function nearestBusyMonth(memos: Memo[], y: number, monthIndex: number): { y: number; m: number } | null {
  let best: { y: number; m: number } | null = null;
  let bestGap = Infinity;
  for (const m of memos) {
    const base = baseKey(m);
    if (base.length < 7) continue;
    const cand = { y: +base.slice(0, 4), m: +base.slice(5, 7) - 1 };
    const gap = Math.abs((cand.y - y) * 12 + (cand.m - monthIndex));
    if (gap < bestGap) { bestGap = gap; best = cand; }
  }
  return best;
}

/** 一行里给带子分道：长的先占上面的道，互相重叠的往后挪 */
function assignLanes(rowBands: Band[]): void {
  rowBands.sort((a, b) => b.span - a.span || a.col - b.col);
  const lanes: Array<Array<[number, number]>> = [];
  for (const b of rowBands) {
    let lane = lanes.findIndex((l) => !l.some(([s, e]) => b.col < e && s < b.col + b.span));
    if (lane < 0) { lane = lanes.length; lanes.push([]); }
    lanes[lane].push([b.col, b.col + b.span]);
    b.lane = lane;
  }
}

/**
 * 排一个月：5~6 行 × 7 列，周一为第一列。
 * 重复项拆成横贯带（连续几天连一条，不每天重复画），单日项留在格子里；
 * 两者共用 CELL_LANES 道，放不下的记进格子的 +n。
 */
export function buildMonth(y: number, monthIndex: number, memos: Memo[]): MonthLayout {
  const offset = (new Date(y, monthIndex, 1).getDay() + 6) % 7;
  const days = new Date(y, monthIndex + 1, 0).getDate();
  const rows = Math.ceil((offset + days) / 7);
  const keys: string[] = [];
  for (let i = 0; i < rows * 7; i++) {
    const d = new Date(y, monthIndex, 1 - offset + i);
    keys.push(dateKey(d.getFullYear(), d.getMonth(), d.getDate()));
  }

  const repeats = memos.filter((m) => m.remind_repeat);
  const byDay = new Map<string, Memo[]>();
  for (const m of memos) {
    if (m.remind_repeat) continue;
    const k = baseKey(m);
    if (!k) continue;
    if (!byDay.has(k)) byDay.set(k, []);
    byDay.get(k)!.push(m);
  }
  byDay.forEach((list) => list.sort(compareAt));

  const bands: Band[] = [];
  const dropped = new Map<string, number>();
  for (let row = 0; row < rows; row++) {
    const rowKeys = keys.slice(row * 7, row * 7 + 7);
    const rowBands: Band[] = [];
    for (const m of repeats) {
      let col = 0;
      while (col < 7) {
        if (!hitsDate(m, rowKeys[col])) { col++; continue; }
        let end = col;
        while (end + 1 < 7 && hitsDate(m, rowKeys[end + 1])) end++;
        rowBands.push({ memo: m, row, col, span: end - col + 1, lane: 0, label: barLabel(m) });
        col = end + 1;
      }
    }
    assignLanes(rowBands);
    for (const b of rowBands) {
      if (b.lane >= CELL_LANES) {
        // 道数满了：不画带，改成在起始格子上记一个 +n，免得叠在一起糊成一片
        const k = rowKeys[b.col];
        dropped.set(k, (dropped.get(k) || 0) + 1);
        continue;
      }
      bands.push(b);
    }
  }

  const covered = new Map<string, number>();
  for (const b of bands) {
    for (let c = b.col; c < b.col + b.span; c++) {
      const k = keys[b.row * 7 + c];
      covered.set(k, Math.max(covered.get(k) || 0, b.lane + 1));
    }
  }

  const cells: DayCell[] = keys.map((key, i) => {
    const d = new Date(y, monthIndex, 1 - offset + i);
    const laneOffset = covered.get(key) || 0;
    const all = byDay.get(key) || [];
    const room = Math.max(0, CELL_LANES - laneOffset);
    return {
      key,
      day: d.getDate(),
      inMonth: d.getMonth() === monthIndex,
      weekend: d.getDay() === 0 || d.getDay() === 6,
      bars: all.slice(0, room),
      more: Math.max(0, all.length - room) + (dropped.get(key) || 0),
      laneOffset,
    };
  });

  const present = new Set<string>();
  for (const key of keys) {
    for (const m of byDay.get(key) || []) present.add(m.id);
    for (const m of repeats) if (hitsDate(m, key)) present.add(m.id);
  }
  let open = 0;
  for (const m of memos) if (present.has(m.id) && !m.is_done) open++;

  return { rows, cells, bands, total: present.size, open };
}
