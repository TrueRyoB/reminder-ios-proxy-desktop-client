// Dashboard v2 frontend. レイアウト/見た目は design/draft/dashboard-prototype.html
// (Gate 2 承認)が正 -- Framework7 は不使用(固定配置ナビバーがカスタム要素を
// 隠す等、構造的に衝突するため全廃した。ユーザー裁定 2026-08-01)。
import "./styles.css";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

// Windows のライト/ダーク設定に追随(CSS 側は :root.dark 変数で対応)
function syncDarkMode(query: MediaQueryList | MediaQueryListEvent) {
  document.documentElement.classList.toggle("dark", query.matches);
}
const darkMediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
syncDarkMode(darkMediaQuery);
darkMediaQuery.addEventListener("change", syncDarkMode);

type LoginResult = { status: "complete" | "two_factor_required" };

type RemindersList = {
  id: string;
  title: string;
  reminderIds: string[];
  recordChangeTag: string | null;
  colorHex: string | null;
  badgeEmblem: string | null;
};

type Reminder = {
  id: string;
  listId: string;
  title: string;
  desc: string;
  completed: boolean;
  dueDate: string | null;
  created: string | null;
  priority: number;
  flagged: boolean;
  allDay: boolean;
  deleted: boolean;
  recordChangeTag: string | null;
};

type AggregatedReminder = Reminder & { listTitle: string };

// Proxy-local vocabulary (expression §1): CloudKit に往復しない意味。
type ProxyMeta = {
  cls?: string | null;
  group?: string | null;
  purpose?: string | null;
  parent?: string | null;
  env?: string | null;
  // upcoming 専用(U1/U2)
  targetList?: string | null;
  repeatDays?: number | null;
  dueOffsetDays?: number | null;
};
type ProxyStore = {
  meta: Record<string, ProxyMeta>;
  notified: string[];
  lastMetaReminder?: string | null;
  envKeys?: string[];
  excludedLists?: string[];
};

// 属性キーの既定語彙。登録済みキー(U4)があればそちらが正。
const DEFAULT_KEYS = ["家", "PC", "外出", "スーツ"];
let envKeys: string[] = [];
function keyList(): string[] {
  return envKeys.length > 0 ? envKeys : DEFAULT_KEYS;
}
// 推薦計算の内部重みのみ -- 表示しない(偽精度の禁止、Gate 2 差し戻し#2)
const SIZE_WEIGHT: Record<string, number> = { 大: 60, 中: 25, 小: 10 };
const LADDER = [
  { min: 180, label: "3時間前" },
  { min: 60, label: "1時間前" },
  { min: 10, label: "10分前" },
];

/* ---------------- tiny UI plumbing (Framework7 の代替) ---------------- */

const $ = <T extends HTMLElement = HTMLElement>(sel: string) => document.querySelector<T>(sel);

const overlayEl = $("#overlay");
let dismissableSheet: HTMLElement | null = null;

function openSheet(id: string, dismissable: boolean) {
  const sheet = $(id);
  if (!sheet) return;
  sheet.classList.add("open");
  overlayEl?.classList.add("open");
  dismissableSheet = dismissable ? sheet : null;
}
function closeSheet(id: string) {
  $(id)?.classList.remove("open");
  if (!document.querySelector(".sheet.open")) overlayEl?.classList.remove("open");
}
overlayEl?.addEventListener("click", () => {
  if (dismissableSheet) {
    dismissableSheet.classList.remove("open");
    dismissableSheet = null;
    if (!document.querySelector(".sheet.open")) overlayEl.classList.remove("open");
  }
});

let toastTimer: number | undefined;
function toast(text: string) {
  const el = $("#toast");
  if (!el) return;
  el.textContent = text;
  el.style.display = "block";
  window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => {
    el.style.display = "none";
  }, 2200);
}

function showPreloader() {
  $("#preloader")?.classList.add("open");
}
function hidePreloader() {
  $("#preloader")?.classList.remove("open");
}
async function withLoading<T>(fn: () => Promise<T>): Promise<T> {
  showPreloader();
  try {
    return await fn();
  } finally {
    hidePreloader();
  }
}

const statusBlock = $("#status-block");
function setStatus(html: string) {
  if (statusBlock) statusBlock.innerHTML = html;
}

function escapeHtml(s: string): string {
  const div = document.createElement("div");
  div.textContent = s;
  return div.innerHTML;
}

function friendlyError(err: unknown): string {
  const msg = String(err);
  if (msg.includes("ログインしていません")) {
    return "ログインが必要です。アプリを再起動してください。";
  }
  if (msg.includes("AUTHENTICATION_FAILED") || msg.includes("(401)") || msg.includes("(421)")) {
    return "セッションの有効期限が切れました。アプリを再起動して再ログインしてください。";
  }
  if (msg.includes("CONFLICT")) {
    return "この項目は他の場所(iPhoneなど)で更新されていました。表示を最新の状態に更新しました。";
  }
  if (msg.toLowerCase().includes("timed out") || msg.includes("request failed") || msg.toLowerCase().includes("connection")) {
    return "ネットワークに接続できませんでした。しばらくしてからもう一度お試しください。";
  }
  return msg;
}

function isConflictError(err: unknown): boolean {
  return String(err).includes("CONFLICT");
}

async function reportMutationError(err: unknown, errEl: HTMLElement | null) {
  if (errEl) errEl.textContent = friendlyError(err);
  if (isConflictError(err)) {
    await refreshCurrentView();
  }
}

const EMBLEM_GLYPHS: Array<[string, string]> = [
  ["people", "👥"],
  ["sport", "🏃"],
  ["lifestyle", "🛍️"],
  ["nature", "🌿"],
  ["food", "🍽️"],
  ["home", "🏠"],
  ["travel", "✈️"],
  ["work", "💼"],
  ["symbol", "📋"],
  ["default", "📋"],
];

function glyphForList(list: RemindersList): string {
  const emblem = list.badgeEmblem?.toLowerCase() ?? "";
  const match = EMBLEM_GLYPHS.find(([prefix]) => emblem.startsWith(prefix));
  if (match) return match[1];
  return list.title.trim().charAt(0).toUpperCase() || "?";
}

/* ---------------- state ---------------- */

let cachedLists: RemindersList[] = [];
let metaMap: Record<string, ProxyMeta> = {};
let allCache: AggregatedReminder[] = [];

type ViewState =
  | { kind: "dash" }
  | { kind: "list"; id: string; title: string }
  | { kind: "search"; q: string };
let currentView: ViewState = { kind: "dash" };

/// ダッシュボード集計から除外するリスト(メモ系)。アイコンクリックでトグル。
let excludedLists = new Set<string>();

type DashMode = "idle" | "compose" | "run" | "finish";
let dashMode: DashMode = "idle";
let plateIds: string[] = [];
let sessionDecl: { min: number; env: string | null } | null = null;
let runDoneCount = 0;
let editModeActive = false;

let currentReminders: AggregatedReminder[] = [];

const dashContentEl = $("#dash-content");
const listToolbarEl = $("#list-toolbar");
const rowsEl = $("#rows");
const viewErrorEl = $("#view-error");
const mainTitleEl = $("#main-title");
const listsListEl = $("#lists-list");
const listsErrorEl = $("#lists-error");
const editModeBtn = $("#edit-mode-btn");
const searchInputEl = $<HTMLInputElement>("#search-input");
const quickrowEl = $("#quickrow");

/* ---------------- derivations (写像: expression §1) ---------------- */

function metaOf(r: Reminder): ProxyMeta {
  return metaMap[r.id] ?? {};
}
function clsOf(r: Reminder): string {
  return metaOf(r).cls ?? "task";
}
/// 参照面の資格規則: 課題ありのみ。時点(signal)・習慣(habit)に加え、
/// upcoming(開始可能時間が未到達 — U1)と集計除外リスト(メモ系)も外す。
function isTaskCard(r: Reminder): boolean {
  return (
    clsOf(r) === "task" &&
    !r.completed &&
    r.listId !== upcomingListId &&
    !excludedLists.has(r.listId)
  );
}
function sizeOf(r: Reminder): "大" | "中" | "小" | null {
  if (r.priority === 1) return "大";
  if (r.priority === 5) return "中";
  if (r.priority === 9) return "小";
  return null;
}
function sizeWeight(r: Reminder): number {
  const s = sizeOf(r);
  return s ? SIZE_WEIGHT[s] : SIZE_WEIGHT["中"];
}
function dueMs(r: Reminder): number | null {
  return r.dueDate ? new Date(r.dueDate).getTime() : null;
}
function endOfToday(): Date {
  const d = new Date();
  d.setHours(23, 59, 59, 999);
  return d;
}
function isOverdue(r: Reminder): boolean {
  const m = dueMs(r);
  return m !== null && m < Date.now();
}
function isDueToday(r: Reminder): boolean {
  const m = dueMs(r);
  return m !== null && !isOverdue(r) && m <= endOfToday().getTime();
}
function slackDays(r: Reminder): number {
  const m = dueMs(r);
  return m === null ? Infinity : Math.max(0, (m - Date.now()) / 86400000);
}
function ageDays(r: Reminder): number {
  if (!r.created) return 0;
  return Math.max(0, (Date.now() - new Date(r.created).getTime()) / 86400000);
}
function fmtDue(r: Reminder): string {
  if (!r.dueDate) return "締切不明";
  const d = new Date(r.dueDate);
  const sameDay = d.toDateString() === new Date().toDateString();
  const dd = `${d.getMonth() + 1}/${d.getDate()}`;
  const hm = `${d.getHours()}:${String(d.getMinutes()).padStart(2, "0")}`;
  // 終日=締切(鳴らない)/時刻つき=発火(鳴る)
  if (r.allDay) return `締切 ${sameDay ? "今日" : dd}`;
  return `🔔 ${sameDay ? hm : `${dd} ${hm}`}`;
}
function byId(id: string): AggregatedReminder | undefined {
  return allCache.find((r) => r.id === id);
}

/* メタタグ(U3/U4): 属性はタイトル内の [キー] として持つ。
   iOS からも見え、ローカルストアが消えても属性は生き残る。 */
function parseTags(title: string): { tags: string[]; clean: string } {
  const tags: string[] = [];
  const clean = title
    .replace(/\[([^\[\]]+)\]/g, (_m, k: string) => {
      const key = k.trim();
      if (key && !tags.includes(key)) tags.push(key);
      return "";
    })
    .replace(/\s+/g, " ")
    .trim();
  return { tags, clean };
}
function tagsOf(r: Reminder): string[] {
  return parseTags(r.title).tags;
}
function cleanTitle(r: Reminder): string {
  const c = parseTags(r.title).clean;
  return c || r.title;
}
function tagBadges(r: Reminder): string {
  return tagsOf(r)
    .map((t) => `<span class="qbadge purple">${escapeHtml(t)}</span>`)
    .join("");
}

/* ---------------- data loading ---------------- */

// U1/U2: "upcoming" という名のリストは待機庫。ここのカードは参照面に出ない。
let upcomingListId: string | null = null;

async function loadLists(): Promise<void> {
  cachedLists = await invoke<RemindersList[]>("list_lists");
  upcomingListId = cachedLists.find((l) => l.title.trim().toLowerCase() === "upcoming")?.id ?? null;
  const laterBtn = $("#quick-later-btn");
  if (laterBtn) laterBtn.style.display = upcomingListId ? "" : "none";
  renderListsNav();
}

async function loadProxyStore(): Promise<void> {
  const store = await invoke<ProxyStore>("get_proxy_store");
  metaMap = store.meta ?? {};
  envKeys = store.envKeys ?? [];
  excludedLists = new Set(store.excludedLists ?? []);
  // 初回だけ既定語彙をストアへ書き込み、以後はストア(proxy_store.json)が
  // 唯一の正になる — ハードコードの解消。ファイルの手編集も有効。
  if (envKeys.length === 0) {
    envKeys = [...DEFAULT_KEYS];
    void invoke("set_env_keys", { keys: envKeys });
  }
}

async function fetchAll(): Promise<AggregatedReminder[]> {
  const perList = await Promise.all(
    cachedLists.map(async (list) => {
      const items = await invoke<Reminder[]>("list_reminders", {
        listId: list.id,
        includeCompleted: false,
      });
      return items.map((r) => ({ ...r, listTitle: list.title }));
    }),
  );
  allCache = perList.flat();
  return allCache;
}

function renderListsNav() {
  if (!listsListEl) return;
  listsListEl.innerHTML = cachedLists
    .map((l) => {
      const on = currentView.kind === "list" && currentView.id === l.id;
      const excluded = excludedLists.has(l.id);
      const color = l.colorHex ?? "#8E8E93";
      return `
      <button class="nav list-item ${on ? "on" : ""} ${excluded ? "excluded" : ""}"
        data-list-id="${escapeHtml(l.id)}">
        <span class="list-badge" style="background-color: ${escapeHtml(color)}"
          data-exclude-id="${escapeHtml(l.id)}"
          title="${excluded ? "集計対象外(クリックで戻す)" : "クリックで集計対象から外す(メモ系リスト用)"}">${glyphForList(l)}</span>
        ${escapeHtml(l.title)}
        <span class="count">${excluded ? "🚫" : l.reminderIds.length}</span>
      </button>`;
    })
    .join("");
  $("#dashboard-nav-item")?.classList.toggle("on", currentView.kind === "dash");
}

/* ---------------- container switching + skeleton ---------------- */

function showContainers(which: "dash" | "rows" | "rows-with-toolbar") {
  if (dashContentEl) dashContentEl.style.display = which === "dash" ? "" : "none";
  if (rowsEl) rowsEl.style.display = which === "dash" ? "none" : "";
  if (listToolbarEl) listToolbarEl.style.display = which === "rows-with-toolbar" ? "" : "none";
}

function skeletonHtml(): string {
  return `
    <div class="skel" style="width:38%"></div>
    <div class="skel"></div>
    <div class="skel" style="width:72%"></div>`;
}

/* ---------------- 締切支配ゾーン + 編成エンジン ---------------- */

function deadlineZone(all: AggregatedReminder[]) {
  const seen = new Set<string>();
  const pick = (arr: AggregatedReminder[]) =>
    arr.filter((r) => !seen.has(r.id)).map((r) => (seen.add(r.id), r));
  const tasks = all.filter(isTaskCard);
  const byDueAsc = (a: Reminder, b: Reminder) => (dueMs(a) ?? 0) - (dueMs(b) ?? 0);
  const od = pick(tasks.filter(isOverdue).sort(byDueAsc));
  const wip = pick(tasks.filter((r) => r.flagged));
  const td = pick(tasks.filter(isDueToday).sort(byDueAsc));
  return { od, wip, td, all: [...od, ...wip, ...td] };
}

type Candidate = { r: AggregatedReminder; score: number; why: Array<[string, string]> };

function candidatesFor(env: string | null, plate: string[]): Candidate[] {
  const inPlate = new Set(plate);
  const zone = new Set(deadlineZone(allCache).all.map((r) => r.id));
  const platePurposes = new Set(
    plate.map((id) => metaMap[id]?.purpose).filter((p): p is string => !!p),
  );
  return allCache
    .filter((r) => isTaskCard(r) && !inPlate.has(r.id) && !zone.has(r.id))
    .map((r) => {
      let score = 0;
      const why: Array<[string, string]> = [];
      if (slackDays(r) <= 2) {
        score += 100;
        why.push(["⏳逼迫", "red"]);
      }
      if (env && tagsOf(r).includes(env)) {
        score += 30;
        why.push(["👔同環境", "purple"]);
      }
      const p = metaOf(r).purpose;
      if (p && platePurposes.has(p)) {
        score += 20;
        why.push(["🔗同系統", "blue"]);
      }
      if (!r.dueDate) {
        score += 25;
        why.push(["🌱いつでも", "green"]);
        const a = ageDays(r);
        if (a >= 7) {
          score += Math.min(a, 30);
          why.push([`⬆浮上(${Math.floor(a)}日)`, "orange"]);
        }
      }
      if (why.length === 0) why.push(["・柔軟", ""]);
      return { r, score, why };
    })
    .sort((a, b) => b.score - a.score);
}

function plateWeight(): number {
  return plateIds.reduce((acc, id) => {
    const r = byId(id);
    return acc + (r ? sizeWeight(r) : 0);
  }, 0);
}

/// 皿の自動仮組み: 義務全部 + 締切不明を必ず1枚 + 高スコア順に容量まで。
function autoComposePlate() {
  if (!sessionDecl) return;
  plateIds = deadlineZone(allCache).all.map((r) => r.id);
  const room = () => (sessionDecl ? sessionDecl.min - plateWeight() : 0);
  const noDeadline = candidatesFor(sessionDecl.env, plateIds).find(
    (c) => !c.r.dueDate && sizeWeight(c.r) <= room(),
  );
  if (noDeadline) plateIds.push(noDeadline.r.id);
  for (const c of candidatesFor(sessionDecl.env, plateIds)) {
    if (plateIds.length >= 8) break;
    if (sizeWeight(c.r) <= room()) plateIds.push(c.r.id);
  }
}

/* ---------------- shared row rendering ---------------- */

function whyBadges(why: Array<[string, string]>): string {
  return why.map(([w, c]) => `<span class="qbadge ${c}">${w}</span>`).join("");
}
function sizeBadge(r: Reminder): string {
  const s = sizeOf(r);
  return s ? `<span class="qbadge">${s}</span>` : "";
}

function rowHtml(r: AggregatedReminder, opts: { showList?: boolean; draggable?: boolean } = {}): string {
  const cls = clsOf(r);
  const clsBadge =
    cls === "signal"
      ? `<span class="qbadge orange">時点</span>`
      : cls === "habit"
        ? `<span class="qbadge green">習慣</span>`
        : "";
  const groupBadge = metaOf(r).group ? `<span class="qbadge purple">イベント</span>` : "";
  const dueBadge = isOverdue(r) && cls === "task"
    ? `<span class="qbadge red">⚠期限切れ</span>`
    : isDueToday(r) && cls === "task"
      ? `<span class="qbadge blue">今日</span>`
      : "";
  const metaBits = [
    fmtDue(r),
    opts.showList ? r.listTitle : null,
    metaOf(r).purpose ? `目的: ${metaOf(r).purpose}` : null,
  ]
    .filter((x): x is string => !!x)
    .map(escapeHtml)
    .join(" ・ ");
  return `
    <div class="row ${cls !== "task" ? "row-ghost" : ""}" data-reminder-id="${escapeHtml(r.id)}"
      ${opts.draggable ? 'draggable="true"' : ""}>
      <input type="checkbox" class="chk" data-reminder-id="${escapeHtml(r.id)}" ${r.completed ? "checked" : ""} />
      <button class="flagbtn ${r.flagged ? "on" : ""}" data-reminder-id="${escapeHtml(r.id)}"
        title="${r.flagged ? "着手中(タップで解除)" : "タップで着手中にする"}">🚩</button>
      <div class="t" data-edit-id="${escapeHtml(r.id)}">
        <div class="name">${escapeHtml(cleanTitle(r))} ${dueBadge}${clsBadge}${groupBadge}${sizeBadge(r)}${tagBadges(r)}</div>
        <div class="meta">${metaBits}${r.desc ? ` ・ ${escapeHtml(r.desc)}` : ""}</div>
      </div>
    </div>`;
}

function renderRows(rows: AggregatedReminder[], showList: boolean, emptyMessage = "リマインダーはありません。") {
  if (!rowsEl) return;
  currentReminders = rows;
  rowsEl.innerHTML =
    rows.length === 0
      ? `<div class="empty">${escapeHtml(emptyMessage)}</div>`
      : `<div class="cardbox">${rows.map((r) => rowHtml(r, { showList, draggable: editModeActive })).join("")}</div>`;
}

/* ---------------- dashboard rendering ---------------- */

function renderDash() {
  if (!dashContentEl) return;
  showContainers("dash");
  if (mainTitleEl) mainTitleEl.textContent = "ダッシュボード";
  if (dashMode === "idle") renderIdle();
  else if (dashMode === "compose") renderCompose();
  else if (dashMode === "run") renderRun();
  else renderFinish();
}

function renderIdle() {
  if (!dashContentEl) return;
  const zone = deadlineZone(allCache);
  const tasks = allCache.filter(isTaskCard);
  const balUnknown = tasks.filter((r) => !r.dueDate).length;
  const balWeek = tasks.filter((r) => r.dueDate && !isOverdue(r) && slackDays(r) < 7).length;
  const next = zone.all[0];

  let html = `
    <div id="balance">
      <span><b>${balUnknown}</b>件 締切不明</span>
      <span><b>${balWeek}</b>件 今週締切</span>
    </div>`;

  if (next) {
    const why = isOverdue(next)
      ? "⚠ 期限切れ — 最優先です"
      : next.flagged
        ? "🚩 着手中 — まず完遂しましょう"
        : "📅 今日が期限です";
    html += `
      <div id="nexthand">
        <div class="lbl">次の一手</div>
        <div class="name">${escapeHtml(cleanTitle(next))}</div>
        <div class="why">${why} ・ ${fmtDue(next)}${sizeOf(next) ? ` ・ ${sizeOf(next)}` : ""} ・ ${escapeHtml(next.listTitle)}${tagsOf(next).length ? ` ・ ${tagsOf(next).map(escapeHtml).join(" ")}` : ""}</div>
        <div class="acts">
          <button data-action="do-next" data-id="${escapeHtml(next.id)}">これをやる</button>
          <button data-action="done" data-id="${escapeHtml(next.id)}">完了にする</button>
        </div>
      </div>`;
    const rest = zone.all.slice(1);
    if (rest.length > 0) {
      // 規律: 続きが想定以上でもここでだけスクロール(max-height は CSS)
      html += `
        <div class="cardbox">
          <div class="muted" style="margin-bottom:4px">締切支配ゾーンの続き(義務・少数)</div>
          <div class="zone-rest">${rest.map((r) => rowHtml(r, { showList: true })).join("")}</div>
        </div>`;
    }
  } else {
    html += `<div class="cardbox empty">義務(期限切れ・今日)はありません 🎉<br/>時間があるなら、下で宣言して皿を組みましょう。</div>`;
  }

  html += `
    <div class="cardbox">
      <div class="muted" style="margin-bottom:8px">セッション宣言 — この時間で何を組む?</div>
      <div class="declare">
        今から
        <select id="d-min">
          <option value="30">30分</option>
          <option value="60" selected>1時間</option>
          <option value="120">2時間</option>
          <option value="180">3時間</option>
        </select>
        <select id="d-env">
          <option value="">環境指定なし</option>
          ${keyList()
            .map((e) => `<option>${escapeHtml(e)}</option>`)
            .join("")}
        </select>
        <button class="primary" data-action="declare">皿を組む</button>
      </div>
    </div>`;
  dashContentEl.innerHTML = html;
}

function renderCompose() {
  if (!dashContentEl || !sessionDecl) return;
  const cands = candidatesFor(sessionDecl.env, plateIds);
  const used = plateWeight();
  const cap = sessionDecl.min;
  const pct = Math.min(100, Math.round((used / cap) * 100));
  const capNote =
    used > cap
      ? "溢れ気味 — 減らすか時間を伸ばしてください"
      : used / cap < 0.7
        ? "まだゆとりがあります"
        : "ちょうど良い量です";

  const plateItems = plateIds.map((id) => byId(id)).filter((r): r is AggregatedReminder => !!r);

  const plateHtml = plateItems
    .map((r, i) => {
      let breather = "";
      const prev = plateItems[i - 1];
      const nextItem = plateItems[i + 1];
      if (i >= 1 && sizeOf(r) !== "小" && prev && sizeOf(prev) !== "小" && nextItem && sizeOf(nextItem) !== "小") {
        const small = cands.find((c) => sizeOf(c.r) === "小");
        if (small) {
          breather = `
            <div class="breather">☕ ここで息抜きを挟みませんか —
              <button data-action="insert-breather" data-id="${escapeHtml(small.r.id)}" data-i="${i + 1}">
                ${escapeHtml(cleanTitle(small.r))}(小)を挿入</button>
            </div>`;
        }
      }
      return `
        <div class="plate-item" draggable="true" data-plate-i="${i}">
          <div class="t">
            <div class="name">${escapeHtml(cleanTitle(r))} ${sizeBadge(r)}${tagBadges(r)}</div>
            <div class="meta">${fmtDue(r)}</div>
          </div>
          <button data-action="remove-plate" data-i="${i}">✕</button>
        </div>${breather}`;
    })
    .join("");

  dashContentEl.innerHTML = `
    <div class="cardbox muted">
      宣言: 今から ${cap}分${sessionDecl.env ? ` ・ 環境=${escapeHtml(sessionDecl.env)}` : ""}
      — 皿は自動で仮組み済み(締切不明を必ず1枚混ぜます)。入れ替え・削除は自由。
    </div>
    <div class="compose">
      <div class="col cardbox">
        <div class="compose-head">候補(理由つき)</div>
        <div class="cand-list">
        ${
          cands.length > 0
            ? cands
                .map(
                  (c) => `
          <div class="cand" draggable="true" data-drag-id="${escapeHtml(c.r.id)}">
            <div class="t">
              <div class="name">${escapeHtml(cleanTitle(c.r))} ${sizeBadge(c.r)}${tagBadges(c.r)}</div>
              <div class="meta">${fmtDue(c.r)} ${whyBadges(c.why)}</div>
            </div>
            <button data-action="add-plate" data-id="${escapeHtml(c.r.id)}">＋皿へ</button>
          </div>`,
                )
                .join("")
            : `<div class="empty">候補はもうありません</div>`
        }
        </div>
      </div>
      <div class="col cardbox" id="plate-drop">
        <div class="compose-head">皿(このセッションの献立)</div>
        <div class="capwrap"><div class="capbar ${used > cap ? "over" : ""}" style="width:${pct}%"></div></div>
        <div class="muted">${capNote}</div>
        <div class="plate-list" style="margin-top:8px">${plateHtml || `<div class="empty">皿は空です。候補から選んでください。</div>`}</div>
        <div class="compose-actions">
          <button class="primary" data-action="start-run" ${plateIds.length ? "" : "disabled"}>この皿で開始</button>
          <button class="ghostbtn" data-action="cancel-compose">やめる</button>
        </div>
      </div>
    </div>`;
}

function renderRun() {
  if (!dashContentEl) return;
  plateIds = plateIds.filter((id) => {
    const r = byId(id);
    return !!r && !r.completed;
  });
  const current = plateIds.length > 0 ? byId(plateIds[0]) : undefined;
  if (!current) {
    dashMode = "finish";
    renderFinish();
    return;
  }
  const restHtml = plateIds
    .map((id, i) => {
      const r = byId(id);
      if (!r) return "";
      return `<div class="run-line ${i === 0 ? "now" : ""}">${i === 0 ? "▶ " : ""}${escapeHtml(cleanTitle(r))} ${sizeBadge(r)}</div>`;
    })
    .join("");
  dashContentEl.innerHTML = `
    <div id="runcard">
      <div class="muted">いまこれ(🚩 着手中は自動で点きます)</div>
      <div class="name">🚩 ${escapeHtml(cleanTitle(current))}</div>
      <div class="muted">${fmtDue(current)}${sizeOf(current) ? ` ・ ${sizeOf(current)}` : ""} ・ ${escapeHtml(current.listTitle)}${tagsOf(current).length ? ` ・ ${tagsOf(current).map(escapeHtml).join(" ")}` : ""}</div>
      <div class="acts">
        <button class="primary" data-action="run-done">完了</button>
        <button class="ghostbtn" data-action="run-skip">スキップ(合わない)</button>
        <button class="ghostbtn" data-action="run-stop">中断して待機へ</button>
      </div>
    </div>
    <div class="cardbox">
      <div class="muted" style="margin-bottom:4px">皿の進行(完了 ${runDoneCount} / 残り ${plateIds.length})</div>
      <div class="run-lines">${restHtml}</div>
    </div>`;
}

function renderFinish() {
  if (!dashContentEl) return;
  dashContentEl.innerHTML = `
    <div class="cardbox finish">
      <div class="big">🎉</div>
      <h2>皿を完走しました</h2>
      <div class="muted">今日はもう終わっていい? それとももう一皿?</div>
      <div class="acts">
        <button class="ghostbtn" data-action="finish-done">おしまい</button>
        <button class="primary" data-action="finish-again">もう一皿</button>
      </div>
    </div>`;
}

/* ---------------- dashboard actions ---------------- */

async function ensureFlagged(r: AggregatedReminder, flagged: boolean) {
  if (r.flagged === flagged) return;
  try {
    const updated = await invoke<Reminder>("update_reminder", { reminder: { ...r, flagged } });
    Object.assign(r, updated);
  } catch (err) {
    console.warn("flag update failed", err);
  }
}

async function completeReminder(id: string) {
  const r = byId(id);
  if (!r) return;
  try {
    const patch = r.flagged ? { completed: true, flagged: false } : { completed: true };
    await withLoading(() => invoke<Reminder>("update_reminder", { reminder: { ...r, ...patch } }));
    allCache = allCache.filter((x) => x.id !== id);
    plateIds = plateIds.filter((x) => x !== id);
    promptNextTask(r);
  } catch (err) {
    await reportMutationError(err, viewErrorEl);
  }
}

async function toggleFlagById(id: string) {
  const r = byId(id) ?? currentReminders.find((x) => x.id === id);
  if (!r) return;
  try {
    const updated = await invoke<Reminder>("update_reminder", { reminder: { ...r, flagged: !r.flagged } });
    Object.assign(r, updated);
    rerenderCurrentView();
  } catch (err) {
    await reportMutationError(err, viewErrorEl);
  }
}

function rerenderCurrentView() {
  if (currentView.kind === "dash") renderDash();
  else if (currentView.kind === "list") renderRows(currentReminders, false);
  else renderRows(currentReminders, true);
}

function bindDashActions() {
  dashContentEl?.addEventListener("click", (e) => {
    const target = (e.target as HTMLElement).closest<HTMLElement>("[data-action]");
    if (!target) return;
    const action = target.dataset.action;
    const id = target.dataset.id ?? "";
    e.preventDefault();

    switch (action) {
      case "declare": {
        const min = Number($<HTMLSelectElement>("#d-min")?.value ?? "60");
        const env = $<HTMLSelectElement>("#d-env")?.value || null;
        sessionDecl = { min, env };
        autoComposePlate();
        dashMode = "compose";
        renderDash();
        break;
      }
      case "do-next": {
        const r = byId(id);
        if (!r) break;
        plateIds = [id];
        sessionDecl = { min: sizeWeight(r), env: null };
        runDoneCount = 0;
        dashMode = "run";
        void ensureFlagged(r, true);
        renderDash();
        break;
      }
      case "done":
        void completeReminder(id);
        break;
      case "add-plate":
        if (!plateIds.includes(id)) plateIds.push(id);
        renderCompose();
        break;
      case "remove-plate":
        plateIds.splice(Number(target.dataset.i ?? "-1"), 1);
        renderCompose();
        break;
      case "insert-breather":
        if (!plateIds.includes(id)) plateIds.splice(Number(target.dataset.i ?? "0"), 0, id);
        renderCompose();
        break;
      case "start-run": {
        runDoneCount = 0;
        dashMode = "run";
        const first = plateIds.length > 0 ? byId(plateIds[0]) : undefined;
        if (first) void ensureFlagged(first, true);
        renderDash();
        break;
      }
      case "cancel-compose":
        dashMode = "idle";
        renderDash();
        break;
      case "run-done": {
        const currentId = plateIds[0];
        if (currentId) {
          runDoneCount += 1;
          void completeReminder(currentId).then(() => {
            const nextTask = plateIds.length > 0 ? byId(plateIds[0]) : undefined;
            if (nextTask) void ensureFlagged(nextTask, true);
            renderDash();
          });
        }
        break;
      }
      case "run-skip": {
        const skippedId = plateIds.shift();
        const skipped = skippedId ? byId(skippedId) : undefined;
        if (skipped) void ensureFlagged(skipped, false);
        toast("スキップしました(このセッションでは勧めません)");
        renderDash();
        break;
      }
      case "run-stop":
        dashMode = "idle";
        renderDash();
        break;
      case "finish-done":
        dashMode = "idle";
        plateIds = [];
        sessionDecl = null;
        renderDash();
        break;
      case "finish-again":
        dashMode = "idle";
        plateIds = [];
        renderDash();
        $<HTMLSelectElement>("#d-min")?.focus();
        break;
    }
  });

  // ゾーン行の操作(チェック・🚩・タップ編集)はダッシュボード内でも有効
  dashContentEl?.addEventListener("click", (e) => {
    const target = e.target as HTMLElement;
    if (target.closest("[data-action]")) return;
    const chk = target.closest<HTMLInputElement>(".chk");
    if (chk?.dataset.reminderId) {
      e.stopPropagation();
      void completeReminder(chk.dataset.reminderId);
      return;
    }
    const flag = target.closest<HTMLElement>(".flagbtn");
    if (flag?.dataset.reminderId) {
      e.stopPropagation();
      void toggleFlagById(flag.dataset.reminderId);
      return;
    }
    const body = target.closest<HTMLElement>("[data-edit-id]");
    if (body?.dataset.editId) openEditSheet(body.dataset.editId);
  });
}

let dragCandidateId: string | null = null;
let dragPlateIndex: number | null = null;

function bindDashDnD() {
  dashContentEl?.addEventListener("dragstart", (e) => {
    const t = e.target as HTMLElement;
    const cand = t.closest<HTMLElement>("[data-drag-id]");
    if (cand?.dataset.dragId) {
      dragCandidateId = cand.dataset.dragId;
      dragPlateIndex = null;
      return;
    }
    const plate = t.closest<HTMLElement>("[data-plate-i]");
    if (plate?.dataset.plateI !== undefined) {
      dragPlateIndex = Number(plate.dataset.plateI);
      dragCandidateId = null;
    }
  });
  dashContentEl?.addEventListener("dragover", (e) => {
    if ((e.target as HTMLElement).closest("#plate-drop")) e.preventDefault();
  });
  dashContentEl?.addEventListener("drop", (e) => {
    const target = e.target as HTMLElement;
    if (!target.closest("#plate-drop")) return;
    e.preventDefault();
    const item = target.closest<HTMLElement>("[data-plate-i]");
    const insertAt = item ? Number(item.dataset.plateI) : plateIds.length;
    if (dragCandidateId) {
      if (!plateIds.includes(dragCandidateId)) plateIds.splice(insertAt, 0, dragCandidateId);
    } else if (dragPlateIndex !== null && dragPlateIndex !== insertAt) {
      const [moved] = plateIds.splice(dragPlateIndex, 1);
      plateIds.splice(Math.min(insertAt, plateIds.length), 0, moved);
    }
    dragCandidateId = null;
    dragPlateIndex = null;
    renderCompose();
  });
}

/* ---------------- クイック投函(3入口・インライン) ---------------- */

type QuickKind = "task" | "remind" | "event" | "later";

function listOptionsHtml(preferredId?: string): string {
  return cachedLists
    .map(
      (l) =>
        `<option value="${escapeHtml(l.id)}" ${l.id === preferredId ? "selected" : ""}>${escapeHtml(l.title)}</option>`,
    )
    .join("");
}

function openQuick(kind: QuickKind, opt: { heading?: string; listId?: string } = {}) {
  if (!quickrowEl || cachedLists.length === 0) return;
  const preferred = opt.listId ?? (currentView.kind === "list" ? currentView.id : undefined);
  const head = opt.heading ? `<span class="quick-heading">${escapeHtml(opt.heading)}</span>` : "";
  const sizeSel = `<select id="q-size" title="所要時間"><option value="9">小</option><option value="5" selected>中</option><option value="1">大</option></select>`;
  const listSel = `<select id="q-list">${listOptionsHtml(preferred)}</select>`;

  let inner = "";
  if (kind === "task") {
    inner = `${head}<b>やること</b>
      <input type="text" id="q-title" placeholder="タイトル(Enterで追加)" />
      ${listSel}${sizeSel}
      <input type="date" id="q-due" title="任意: 締切日(鳴りません)" />
      <button class="primary" id="q-submit">追加</button>
      <button class="ghostbtn" id="q-close">閉じる</button>
      <span class="quick-hint">鳴りません。締切日を付けると、静かに並びの材料になるだけです。</span>`;
  } else if (kind === "remind") {
    inner = `${head}<b>リマインド</b>
      <input type="text" id="q-title" placeholder="タイトル" />
      ${listSel}${sizeSel}
      <input type="datetime-local" id="q-due" />
      <button class="primary" id="q-submit">追加</button>
      <button class="ghostbtn" id="q-close">閉じる</button>
      <span class="quick-hint">この時刻に鳴ります(iOS でも)。</span>`;
  } else if (kind === "later") {
    const targets = cachedLists.filter((l) => l.id !== upcomingListId);
    inner = `${head}<b>習慣</b>
      <input type="text" id="q-title" placeholder="タイトル([属性]込みで書けます)" />
      ${sizeSel}
      開始日 <input type="date" id="q-due" />
      行き先 <select id="q-target">${targets
        .map((l) => `<option value="${escapeHtml(l.id)}">${escapeHtml(l.title)}</option>`)
        .join("")}</select>
      <select id="q-repeat">
        <option value="0">一回きり</option>
        <option value="1">毎日</option>
        <option value="7" selected>毎週</option>
        <option value="14">隔週</option>
        <option value="30">毎月(30日)</option>
      </select>
      締切+<input type="number" id="q-offset" min="0" step="1" style="width:56px" />日
      <button class="primary" id="q-submit">習慣として置く</button>
      <button class="ghostbtn" id="q-close">閉じる</button>
      <span class="quick-hint">その時にならないと着手できないタスク。開始日が来たら行き先リストに現れます(それまで参照面には出ません)。繰り返しなら発火後に次回へ自動で再装填。締切+N日は空なら締切不明のまま産みます。</span>`;
  } else {
    inner = `${head}<b>イベント</b>
      <input type="text" id="q-title" placeholder="行事名(例: 面接(C社))" />
      ${listSel}
      <input type="datetime-local" id="q-due" title="行事時刻" />
      ${LADDER.map(
        (l) =>
          `<label class="quick-ladder"><input type="checkbox" class="q-lad" value="${l.min}" checked />${l.label}</label>`,
      ).join("")}
      <button class="primary" id="q-submit">一括作成</button>
      <button class="ghostbtn" id="q-close">閉じる</button>
      <span class="quick-hint">本番まで段階的に鳴ります。行事カード1枚+時点カードN枚を生成します(時点カードは発火後に自動完了)。</span>`;
  }
  quickrowEl.dataset.kind = kind;
  quickrowEl.innerHTML = inner;
  quickrowEl.classList.add("open");
  const titleInput = $<HTMLInputElement>("#q-title");
  titleInput?.focus();
  titleInput?.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      void submitQuick();
    }
  });
  $("#q-submit")?.addEventListener("click", () => void submitQuick());
  $("#q-close")?.addEventListener("click", closeQuick);
}

function closeQuick() {
  if (!quickrowEl) return;
  quickrowEl.classList.remove("open");
  quickrowEl.innerHTML = "";
}

async function submitQuick() {
  if (!quickrowEl) return;
  const kind = quickrowEl.dataset.kind as QuickKind | undefined;
  const title = $<HTMLInputElement>("#q-title")?.value.trim() ?? "";
  const listId = $<HTMLSelectElement>("#q-list")?.value ?? "";
  if (!kind || !title) return;
  if (kind !== "later" && !listId) return;
  const priority = Number($<HTMLSelectElement>("#q-size")?.value ?? "0");

  try {
    if (kind === "later") {
      // U1/U2: upcoming に「タスク定義」を置く。発火は watch.rs が担う。
      if (!upcomingListId) return;
      const d = $<HTMLInputElement>("#q-due")?.value ?? "";
      if (!d) {
        toast("開始日を入れてください");
        return;
      }
      const target = $<HTMLSelectElement>("#q-target")?.value ?? "";
      const repeat = Number($<HTMLSelectElement>("#q-repeat")?.value ?? "0");
      const offsetRaw = $<HTMLInputElement>("#q-offset")?.value ?? "";
      if (!target) return;
      await withLoading(async () => {
        const card = await invoke<Reminder>("create_reminder", {
          listId: upcomingListId,
          title,
          notes: "",
          priority,
          flagged: false,
          dueDate: new Date(`${d}T12:00`).toISOString(),
          allDay: true, // 開始日は鳴らない担体で持つ
        });
        const meta: ProxyMeta = { targetList: target };
        if (repeat > 0) meta.repeatDays = repeat;
        if (offsetRaw !== "") meta.dueOffsetDays = Number(offsetRaw);
        await invoke("set_proxy_meta", { id: card.id, meta });
        metaMap[card.id] = meta;
      });
      toast("習慣として置きました(開始日に行き先リストへ現れます)");
    } else if (kind === "task") {
      const d = $<HTMLInputElement>("#q-due")?.value ?? "";
      const dueDate = d ? new Date(`${d}T12:00`).toISOString() : null;
      await withLoading(() =>
        invoke("create_reminder", {
          listId,
          title,
          notes: "",
          priority,
          flagged: false,
          dueDate,
          allDay: dueDate !== null,
        }),
      );
    } else if (kind === "remind") {
      const d = $<HTMLInputElement>("#q-due")?.value ?? "";
      if (!d) {
        toast("日時を入れてください");
        return;
      }
      await withLoading(() =>
        invoke("create_reminder", {
          listId,
          title,
          notes: "",
          priority,
          flagged: false,
          dueDate: new Date(d).toISOString(),
          allDay: false,
        }),
      );
    } else {
      const d = $<HTMLInputElement>("#q-due")?.value ?? "";
      if (!d) {
        toast("行事時刻を入れてください");
        return;
      }
      const eventTime = new Date(d);
      const ladderMins = Array.from(document.querySelectorAll<HTMLInputElement>(".q-lad:checked")).map((c) =>
        Number(c.value),
      );
      const group = crypto.randomUUID();
      await withLoading(async () => {
        const main = await invoke<Reminder>("create_reminder", {
          listId,
          title,
          notes: "",
          priority: priority || 5,
          flagged: false,
          dueDate: eventTime.toISOString(),
          allDay: false,
        });
        await invoke("set_proxy_meta", { id: main.id, meta: { group } });
        metaMap[main.id] = { group };
        for (const min of ladderMins) {
          const ladderDef = LADDER.find((l) => l.min === min);
          const signal = await invoke<Reminder>("create_reminder", {
            listId,
            title: `${title}まで${ladderDef?.label.replace("前", "") ?? `${min}分`}`,
            notes: "",
            priority: 9,
            flagged: false,
            dueDate: new Date(eventTime.getTime() - min * 60000).toISOString(),
            allDay: false,
          });
          const meta: ProxyMeta = { cls: "signal", group };
          await invoke("set_proxy_meta", { id: signal.id, meta });
          metaMap[signal.id] = meta;
        }
      });
      toast("行事カード1枚+時点カードを作成しました");
    }
    closeQuick();
    await refreshCurrentView();
  } catch (err) {
    await reportMutationError(err, viewErrorEl);
  }
}

// 完遂→発生への還流(Q4)
function promptNextTask(completed: Reminder) {
  openQuick("task", {
    heading: `「${completed.title}」を完了しました。次のタスクを追加しますか?`,
    listId: completed.listId,
  });
}

/* ---------------- views: 具体リスト / 検索 ---------------- */

async function selectList(listId: string, title: string) {
  currentView = { kind: "list", id: listId, title };
  exitEditMode();
  showContainers("rows-with-toolbar");
  renderListsNav();
  if (mainTitleEl) mainTitleEl.textContent = title;
  if (viewErrorEl) viewErrorEl.textContent = "";
  if (rowsEl) rowsEl.innerHTML = skeletonHtml();
  try {
    const reminders = await invoke<Reminder[]>("list_reminders", {
      listId,
      includeCompleted: false,
    });
    const rows = reminders.map((r) => ({ ...r, listTitle: title }));
    renderRows(rows, false, "このリストにリマインダーはありません。");
  } catch (err) {
    if (viewErrorEl) viewErrorEl.textContent = friendlyError(err);
  }
}

async function selectDashboard() {
  currentView = { kind: "dash" };
  exitEditMode();
  showContainers("dash");
  renderListsNav();
  if (mainTitleEl) mainTitleEl.textContent = "ダッシュボード";
  if (viewErrorEl) viewErrorEl.textContent = "";
  if (dashContentEl) dashContentEl.innerHTML = skeletonHtml();
  try {
    await loadProxyStore();
    await fetchAll();
    renderDash();
  } catch (err) {
    if (viewErrorEl) viewErrorEl.textContent = friendlyError(err);
  }
}

async function runSearch(q: string) {
  currentView = { kind: "search", q };
  exitEditMode();
  showContainers("rows");
  renderListsNav();
  if (mainTitleEl) mainTitleEl.textContent = `検索: ${q}`;
  if (viewErrorEl) viewErrorEl.textContent = "";
  try {
    if (allCache.length === 0) {
      await loadProxyStore();
      await fetchAll();
    }
    const needle = q.toLowerCase();
    // 課題ありフィルタ(D2): 時点・習慣は具体リストでのみ見える
    const hits = allCache.filter(
      (r) =>
        isTaskCard(r) &&
        (r.title.toLowerCase().includes(needle) || r.desc.toLowerCase().includes(needle)),
    );
    renderRows(hits, true, "検索結果はありません。");
  } catch (err) {
    if (viewErrorEl) viewErrorEl.textContent = friendlyError(err);
  }
}

async function refreshCurrentView() {
  if (currentView.kind === "dash") {
    try {
      await loadProxyStore();
      await fetchAll();
      renderDash();
    } catch (err) {
      if (viewErrorEl) viewErrorEl.textContent = friendlyError(err);
    }
  } else if (currentView.kind === "list") {
    await selectList(currentView.id, currentView.title);
  } else {
    await runSearch(currentView.q);
  }
}

/* ---------------- 並び替え(具体リストのみ・HTML5 DnD) ---------------- */

function exitEditMode() {
  if (!editModeActive) return;
  editModeActive = false;
  if (editModeBtn) editModeBtn.textContent = "編集(並び替え)";
}

async function handleReorder() {
  if (currentView.kind !== "list" || !rowsEl) return;
  const listId = currentView.id;
  const ids = Array.from(rowsEl.querySelectorAll<HTMLElement>("[data-reminder-id]"))
    .map((el) => el.dataset.reminderId)
    .filter((id): id is string => Boolean(id));
  const list = cachedLists.find((l) => l.id === listId);
  if (!list) return;
  if (viewErrorEl) viewErrorEl.textContent = "";
  try {
    await withLoading(async () => {
      await invoke("reorder_list", { list, newOrder: ids });
      cachedLists = await invoke<RemindersList[]>("list_lists");
    });
  } catch (err) {
    await reportMutationError(err, viewErrorEl);
  }
}

let rowDragId: string | null = null;

function bindRowsInteractions() {
  editModeBtn?.addEventListener("click", (e) => {
    e.preventDefault();
    if (currentView.kind !== "list") return;
    editModeActive = !editModeActive;
    if (editModeBtn) editModeBtn.textContent = editModeActive ? "完了" : "編集(並び替え)";
    renderRows(currentReminders, false, "このリストにリマインダーはありません。");
  });

  rowsEl?.addEventListener("dragstart", (e) => {
    if (!editModeActive) return;
    const row = (e.target as HTMLElement).closest<HTMLElement>("[data-reminder-id]");
    rowDragId = row?.dataset.reminderId ?? null;
  });
  rowsEl?.addEventListener("dragover", (e) => {
    if (editModeActive && rowDragId) e.preventDefault();
  });
  rowsEl?.addEventListener("drop", (e) => {
    if (!editModeActive || !rowDragId || !rowsEl) return;
    e.preventDefault();
    const target = (e.target as HTMLElement).closest<HTMLElement>("[data-reminder-id]");
    const src = rowsEl.querySelector<HTMLElement>(`[data-reminder-id="${CSS.escape(rowDragId)}"]`);
    if (src && target && src !== target) {
      target.before(src);
      void handleReorder();
    }
    rowDragId = null;
  });

  rowsEl?.addEventListener("click", (e) => {
    if (editModeActive) return; // 並び替え中はタップ編集を止める
    const target = e.target as HTMLElement;

    const checkbox = target.closest<HTMLInputElement>(".chk");
    if (checkbox) {
      e.stopPropagation();
      const id = checkbox.dataset.reminderId;
      if (id) void toggleCompletedRow(id, checkbox.checked);
      return;
    }

    const flag = target.closest<HTMLElement>(".flagbtn");
    if (flag) {
      e.stopPropagation();
      const id = flag.dataset.reminderId;
      if (id) void toggleFlagById(id);
      return;
    }

    const body = target.closest<HTMLElement>("[data-edit-id]");
    if (body?.dataset.editId) openEditSheet(body.dataset.editId);
  });
}

async function toggleCompletedRow(id: string, completed: boolean) {
  const r = currentReminders.find((x) => x.id === id);
  if (!r) return;
  try {
    const patch: Partial<Reminder> = completed && r.flagged ? { completed, flagged: false } : { completed };
    await withLoading(() => invoke<Reminder>("update_reminder", { reminder: { ...r, ...patch } }));
    await refreshCurrentView();
    if (completed && clsOf(r) === "task") promptNextTask(r);
  } catch (err) {
    await reportMutationError(err, viewErrorEl);
  }
}

/* ---------------- 編集シート ---------------- */

let editingReminder: AggregatedReminder | null = null;

function toLocalDatetimeInputValue(d: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

function findReminder(id: string): AggregatedReminder | undefined {
  return byId(id) ?? currentReminders.find((x) => x.id === id);
}

function renderEditTags(r: AggregatedReminder) {
  const box = $("#edit-tags");
  if (!box) return;
  const active = new Set(tagsOf(r));
  box.innerHTML = keyList()
    .map(
      (k) =>
        `<button type="button" class="tagchip ${active.has(k) ? "on" : ""}" data-key="${escapeHtml(k)}">${escapeHtml(k)}</button>`,
    )
    .join("");
}

function openEditSheet(id: string) {
  const r = findReminder(id);
  if (!r) return;
  editingReminder = r;
  const meta = metaOf(r);

  const setVal = (sel: string, v: string) => {
    const el = $<HTMLInputElement>(sel);
    if (el) el.value = v;
  };
  setVal("#edit-title", r.title);
  setVal("#edit-notes", r.desc);
  setVal("#edit-size", String(r.priority));
  setVal("#edit-due-date", r.dueDate ? toLocalDatetimeInputValue(new Date(r.dueDate)) : "");
  setVal("#edit-cls", meta.cls ?? "");
  setVal("#edit-purpose", meta.purpose ?? "");
  renderEditTags(r);
  // upcoming のカードだけ、発火の行き先・繰り返し・締切オフセットを見せる
  const isUpcoming = r.listId === upcomingListId;
  document.querySelectorAll<HTMLElement>(".upcoming-only").forEach((el) => {
    el.style.display = isUpcoming ? "" : "none";
  });
  if (isUpcoming) {
    const targetSel = $<HTMLSelectElement>("#edit-target");
    if (targetSel) {
      targetSel.innerHTML = cachedLists
        .filter((l) => l.id !== upcomingListId)
        .map((l) => `<option value="${escapeHtml(l.id)}">${escapeHtml(l.title)}</option>`)
        .join("");
      if (meta.targetList) targetSel.value = meta.targetList;
    }
    setVal("#edit-repeat", String(meta.repeatDays ?? 0));
    setVal("#edit-offset", meta.dueOffsetDays === null || meta.dueOffsetDays === undefined ? "" : String(meta.dueOffsetDays));
  }
  const flaggedInput = $<HTMLInputElement>("#edit-flagged");
  if (flaggedInput) flaggedInput.checked = r.flagged;
  const allDayInput = $<HTMLInputElement>("#edit-all-day");
  if (allDayInput) allDayInput.checked = r.allDay;
  const listSelect = $<HTMLSelectElement>("#edit-list-id");
  if (listSelect) {
    listSelect.innerHTML = listOptionsHtml();
    listSelect.value = r.listId;
  }
  const errEl = $("#edit-error");
  if (errEl) errEl.textContent = "";

  openSheet("#edit-sheet", true);
}

async function applyEdit(patch: Partial<Reminder>) {
  if (!editingReminder) return;
  const errEl = $("#edit-error");
  if (errEl) errEl.textContent = "";
  try {
    const merged = { ...editingReminder, ...patch };
    const updated = await withLoading(() => invoke<Reminder>("update_reminder", { reminder: merged }));
    Object.assign(editingReminder, updated);
    rerenderCurrentView();
  } catch (err) {
    await reportMutationError(err, errEl);
  }
}

async function saveMeta(id: string, patch: Partial<ProxyMeta>) {
  const merged: ProxyMeta = { ...(metaMap[id] ?? {}), ...patch };
  (Object.keys(merged) as Array<keyof ProxyMeta>).forEach((k) => {
    // 0 は有効値(締切+0日=発火日当日)なので、null/undefined/空文字だけ落とす
    if (merged[k] === null || merged[k] === undefined || merged[k] === "") delete merged[k];
  });
  if (Object.keys(merged).length === 0) delete metaMap[id];
  else metaMap[id] = merged;
  try {
    await invoke("set_proxy_meta", { id, meta: merged });
  } catch (err) {
    const errEl = $("#edit-error");
    if (errEl) errEl.textContent = friendlyError(err);
  }
}

function bindEditSheet() {
  $("#edit-title")?.addEventListener("change", (e) => {
    void applyEdit({ title: (e.target as HTMLInputElement).value });
  });
  $("#edit-notes")?.addEventListener("change", (e) => {
    void applyEdit({ desc: (e.target as HTMLTextAreaElement).value });
  });
  $("#edit-size")?.addEventListener("change", (e) => {
    void applyEdit({ priority: Number((e.target as HTMLSelectElement).value) });
  });
  $("#edit-flagged")?.addEventListener("change", (e) => {
    void applyEdit({ flagged: (e.target as HTMLInputElement).checked });
  });
  $("#edit-due-date")?.addEventListener("change", (e) => {
    const v = (e.target as HTMLInputElement).value;
    void applyEdit({ dueDate: v ? new Date(v).toISOString() : null });
  });
  $("#edit-all-day")?.addEventListener("change", (e) => {
    void applyEdit({ allDay: (e.target as HTMLInputElement).checked });
  });
  $("#edit-cls")?.addEventListener("change", (e) => {
    if (!editingReminder) return;
    void saveMeta(editingReminder.id, { cls: (e.target as HTMLSelectElement).value || null });
  });
  $("#edit-purpose")?.addEventListener("change", (e) => {
    if (!editingReminder) return;
    void saveMeta(editingReminder.id, { purpose: (e.target as HTMLInputElement).value || null });
  });
  // 属性タグ(U3): チップのトグルがタイトル内の [キー] を書き換える
  $("#edit-tags")?.addEventListener("click", (e) => {
    const chip = (e.target as HTMLElement).closest<HTMLElement>(".tagchip");
    if (!chip || !editingReminder) return;
    const key = chip.dataset.key ?? "";
    if (!key) return;
    const { tags, clean } = parseTags(editingReminder.title);
    const nextTags = tags.includes(key) ? tags.filter((t) => t !== key) : [...tags, key];
    const newTitle = `${nextTags.map((t) => `[${t}]`).join("")}${nextTags.length ? " " : ""}${clean}`;
    void applyEdit({ title: newTitle }).then(() => {
      if (!editingReminder) return;
      renderEditTags(editingReminder);
      const titleInput = $<HTMLInputElement>("#edit-title");
      if (titleInput) titleInput.value = editingReminder.title;
    });
  });
  // 右クリックで語彙からキーを削除(タイトル内のタグ自体は残る)
  $("#edit-tags")?.addEventListener("contextmenu", (e) => {
    const chip = (e.target as HTMLElement).closest<HTMLElement>(".tagchip");
    if (!chip) return;
    e.preventDefault();
    const key = chip.dataset.key ?? "";
    if (!key) return;
    if (!window.confirm(`属性キー「${key}」を語彙から削除しますか?(タイトル内の [${key}] はそのまま残ります)`)) return;
    envKeys = keyList().filter((k) => k !== key);
    void invoke("set_env_keys", { keys: envKeys });
    if (editingReminder) renderEditTags(editingReminder);
  });
  // 新しいキーの登録(U4): 語彙はローカルストアに保存
  $("#edit-add-key")?.addEventListener("click", () => {
    const k = window.prompt("新しい属性キー(例: 上長 / 思考)", "")?.trim() ?? "";
    if (!k) return;
    if (k.includes("[") || k.includes("]")) {
      toast("[ ] は使えません");
      return;
    }
    const keys = keyList();
    if (keys.includes(k)) return;
    envKeys = [...keys, k];
    void invoke("set_env_keys", { keys: envKeys });
    if (editingReminder) renderEditTags(editingReminder);
  });
  // upcoming 専用メタ(U1/U2)
  $("#edit-target")?.addEventListener("change", (e) => {
    if (!editingReminder) return;
    void saveMeta(editingReminder.id, { targetList: (e.target as HTMLSelectElement).value || null });
  });
  $("#edit-repeat")?.addEventListener("change", (e) => {
    if (!editingReminder) return;
    const v = Number((e.target as HTMLSelectElement).value);
    void saveMeta(editingReminder.id, { repeatDays: v > 0 ? v : null });
  });
  $("#edit-offset")?.addEventListener("change", (e) => {
    if (!editingReminder) return;
    const raw = (e.target as HTMLInputElement).value;
    void saveMeta(editingReminder.id, { dueOffsetDays: raw === "" ? null : Number(raw) });
  });
  $("#edit-list-id")?.addEventListener("change", async (e) => {
    const newListId = (e.target as HTMLSelectElement).value;
    await applyEdit({ listId: newListId });
    if (currentView.kind === "list" && currentView.id !== newListId) {
      closeSheet("#edit-sheet");
      await refreshCurrentView();
    }
  });
  // 分解: 子カードに割る(親子ローカル1段、目的を継承)
  $("#edit-split-btn")?.addEventListener("click", () => {
    if (!editingReminder) return;
    const parent = editingReminder;
    const title = window.prompt("子カードのタイトル(親の目的を継承します)", "");
    const t = title?.trim() ?? "";
    if (!t) return;
    void (async () => {
      try {
        const child = await withLoading(() =>
          invoke<Reminder>("create_reminder", {
            listId: parent.listId,
            title: t,
            notes: "",
            priority: 9,
            flagged: false,
            dueDate: null,
            allDay: false,
          }),
        );
        const meta: ProxyMeta = { parent: parent.id };
        const parentPurpose = metaOf(parent).purpose;
        if (parentPurpose) meta.purpose = parentPurpose;
        await invoke("set_proxy_meta", { id: child.id, meta });
        metaMap[child.id] = meta;
        toast("子カードを作成しました(親子はローカルで1段)");
        await refreshCurrentView();
      } catch (err) {
        await reportMutationError(err, $("#edit-error"));
      }
    })();
  });
  $("#edit-delete-btn")?.addEventListener("click", () => {
    if (!editingReminder) return;
    const reminder = editingReminder;
    const errEl = $("#edit-error");
    if (!window.confirm(`「${reminder.title}」を削除しますか?この操作は取り消せません。`)) return;
    void (async () => {
      try {
        await withLoading(() => invoke("delete_reminder", { reminder }));
        closeSheet("#edit-sheet");
        await refreshCurrentView();
      } catch (err) {
        await reportMutationError(err, errEl);
      }
    })();
  });
  $("#edit-close-btn")?.addEventListener("click", () => closeSheet("#edit-sheet"));
}

/* ---------------- navigation bindings ---------------- */

function bindNavigation() {
  $("#dashboard-nav-item")?.addEventListener("click", (e) => {
    e.preventDefault();
    if (searchInputEl) searchInputEl.value = "";
    void selectDashboard();
  });
  listsListEl?.addEventListener("click", (e) => {
    const target = e.target as HTMLElement;
    // アイコンクリック = 集計対象のトグル(メモ系リストの逃し先)
    const badge = target.closest<HTMLElement>("[data-exclude-id]");
    if (badge?.dataset.excludeId) {
      e.preventDefault();
      e.stopPropagation();
      const id = badge.dataset.excludeId;
      const excluded = !excludedLists.has(id);
      if (excluded) excludedLists.add(id);
      else excludedLists.delete(id);
      void invoke("set_list_excluded", { listId: id, excluded });
      renderListsNav();
      toast(excluded ? "集計対象から外しました(ダッシュボードに出ません)" : "集計対象に戻しました");
      if (currentView.kind === "dash") renderDash();
      return;
    }
    const listLink = target.closest<HTMLElement>(".list-item");
    if (!listLink) return;
    e.preventDefault();
    const listId = listLink.dataset.listId;
    const title = listLink.textContent?.replace(/\d+$/, "").trim() ?? "";
    if (listId) {
      if (searchInputEl) searchInputEl.value = "";
      const list = cachedLists.find((l) => l.id === listId);
      void selectList(listId, list?.title ?? title);
    }
  });

  searchInputEl?.addEventListener("input", () => {
    const q = searchInputEl.value.trim();
    if (!q) {
      if (currentView.kind === "search") void selectDashboard();
      return;
    }
    void runSearch(q);
  });

  $("#quick-task-btn")?.addEventListener("click", (e) => {
    e.preventDefault();
    openQuick("task");
  });
  $("#quick-remind-btn")?.addEventListener("click", (e) => {
    e.preventDefault();
    openQuick("remind");
  });
  $("#quick-event-btn")?.addEventListener("click", (e) => {
    e.preventDefault();
    openQuick("event");
  });
  $("#quick-later-btn")?.addEventListener("click", (e) => {
    e.preventDefault();
    openQuick("later");
  });
}

/* ---------------- boot / auth ---------------- */

let eventListenerBound = false;

async function onReady() {
  setStatus("");
  if (!eventListenerBound) {
    eventListenerBound = true;
    bindNavigation();
    bindRowsInteractions();
    bindEditSheet();
    bindDashActions();
    bindDashDnD();
    // ポーラーの通知発火・時点カード自動完了を画面に反映する
    void listen("reminders-changed", () => {
      void refreshCurrentView();
    });
  }
  if (listsErrorEl) listsErrorEl.textContent = "";
  try {
    await loadLists();
    await selectDashboard();
  } catch (err) {
    if (listsErrorEl) listsErrorEl.textContent = friendlyError(err);
  }
}

function bindLoginForm() {
  const form = $<HTMLFormElement>("#login-form");
  const errorEl = $("#login-error");
  form?.addEventListener("submit", async (e) => {
    e.preventDefault();
    const appleId = $<HTMLInputElement>("#login-apple-id")?.value.trim() ?? "";
    const password = $<HTMLInputElement>("#login-password")?.value ?? "";
    if (errorEl) errorEl.textContent = "";
    try {
      const result = await invoke<LoginResult>("login", { appleId, password });
      if (result.status === "two_factor_required") {
        closeSheet("#login-sheet");
        openSheet("#two-factor-sheet", false);
      } else {
        closeSheet("#login-sheet");
        await onReady();
      }
    } catch (err) {
      if (errorEl) errorEl.textContent = String(err);
    }
  });
}

function bindTwoFactorForm() {
  const form = $<HTMLFormElement>("#two-factor-form");
  const errorEl = $("#two-factor-error");
  form?.addEventListener("submit", async (e) => {
    e.preventDefault();
    const code = $<HTMLInputElement>("#two-factor-code")?.value.trim() ?? "";
    if (errorEl) errorEl.textContent = "";
    try {
      await invoke("submit_two_factor_code", { code });
      closeSheet("#two-factor-sheet");
      await onReady();
    } catch (err) {
      if (errorEl) errorEl.textContent = String(err);
    }
  });
}

async function boot() {
  bindLoginForm();
  bindTwoFactorForm();
  setStatus("<p>セッションを確認しています…</p>");

  const appleId = await invoke<string | null>("get_persisted_apple_id");
  if (!appleId) {
    setStatus("<p>ログインしてください。</p>");
    openSheet("#login-sheet", false);
    return;
  }

  const resumed = await invoke<boolean>("try_resume", { appleId });
  if (resumed) {
    await onReady();
    return;
  }

  setStatus("<p>再ログインが必要です。</p>");
  const idInput = $<HTMLInputElement>("#login-apple-id");
  if (idInput) idInput.value = appleId;
  openSheet("#login-sheet", false);
}

void boot();
