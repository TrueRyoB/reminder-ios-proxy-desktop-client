import Framework7 from "framework7/lite/bundle";
import "framework7/css/bundle";
import "./styles.css";
import { invoke } from "@tauri-apps/api/core";

// iOS theme forced explicitly: Framework7's `theme: 'auto'` detects the host
// OS and would pick the Material theme on Windows, which defeats the whole
// point of choosing Framework7 (see project decision notes) for iOS-fidelity.
const app = new Framework7({
  el: "#app",
  theme: "ios",
  name: "iCloud Reminders",
});

// Framework7 has no automatic OS-dark-mode detection of its own (its ".dark"
// class is purely a manual toggle) -- this is what actually wires it up to
// Windows' own light/dark setting, live.
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
  priority: number;
  flagged: boolean;
  allDay: boolean;
  deleted: boolean;
  recordChangeTag: string | null;
};

const statusBlock = document.querySelector<HTMLElement>("#status-block");
function setStatus(html: string) {
  if (statusBlock) statusBlock.innerHTML = html;
}

function escapeHtml(s: string): string {
  const div = document.createElement("div");
  div.textContent = s;
  return div.innerHTML;
}

// Create/update/delete round-trip to the real Apple servers -- typically
// well under a second, but with no visual feedback at all a slow network
// blip just looks like the tap didn't register. Wraps any such call with
// Framework7's built-in preloader overlay.
async function withLoading<T>(fn: () => Promise<T>): Promise<T> {
  app.preloader.show();
  try {
    return await fn();
  } finally {
    app.preloader.hide();
  }
}

// Errors surfaced from Rust are raw anyhow chains (e.g. "CloudKit
// /records/modify failed (421): ..." or "modify failed for 1 record(s):
// Reminder/XXX: CONFLICT (...)") -- fine for handan/debugging, meaningless
// to the user. This maps the recognizable failure classes to a plain
// Japanese explanation and falls back to the raw message for anything else
// (better to show something than to hide a real error entirely).
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

// A CloudKit "CONFLICT" means someone/something else changed this record
// since we last fetched it (e.g. edited on another device) -- the stale
// data we're holding needs refreshing, not just an error message.
function isConflictError(err: unknown): boolean {
  return String(err).includes("CONFLICT");
}

async function reportMutationError(err: unknown, errEl: HTMLElement | null) {
  if (errEl) errEl.textContent = friendlyError(err);
  if (isConflictError(err)) {
    await refreshCurrentView();
  }
}

// Apple's `BadgeEmblem` values are internal icon-set identifiers (e.g.
// "people2", "sport6") from the Reminders app's icon picker -- not SF Symbol
// names, and SF Symbols can't be embedded here anyway (Apple-only license).
// This maps common prefixes to a plain Unicode glyph as a reasonable visual
// stand-in; anything unrecognized falls back to the list's own initial.
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

function listBadgeHtml(list: RemindersList): string {
  const color = list.colorHex ?? "#8E8E93";
  return `<div class="list-badge" style="background-color: ${escapeHtml(color)}">${glyphForList(list)}</div>`;
}

const loginSheet = app.sheet.create({
  el: document.querySelector("#login-sheet") as HTMLElement,
  backdrop: true,
  closeByBackdropClick: false,
  closeByOutsideClick: false,
});

const twoFactorSheet = app.sheet.create({
  el: document.querySelector("#two-factor-sheet") as HTMLElement,
  backdrop: true,
  closeByBackdropClick: false,
  closeByOutsideClick: false,
});

const listsListEl = document.querySelector<HTMLUListElement>("#lists-list ul");
const listsErrorEl = document.querySelector<HTMLElement>("#lists-error");
const remindersListEl = document.querySelector<HTMLUListElement>("#reminders-list ul");
const remindersContainerEl = document.querySelector<HTMLElement>("#reminders-list");
const remindersErrorEl = document.querySelector<HTMLElement>("#reminders-error");
const mainTitleEl = document.querySelector<HTMLElement>("#main-title");

function renderLists(lists: RemindersList[]) {
  if (!listsListEl) return;
  listsListEl.innerHTML = lists
    .map(
      (l) => `
        <li>
          <a href="#" class="item-link item-content list-item" data-list-id="${escapeHtml(l.id)}">
            <div class="item-media">${listBadgeHtml(l)}</div>
            <div class="item-inner">
              <div class="item-title">${escapeHtml(l.title)}</div>
              <div class="item-after">${l.reminderIds.length}</div>
            </div>
          </a>
        </li>`,
    )
    .join("");
}

// Native Reminders' overdue items render their due date in red; anything
// else (future, or completed) uses the normal text color -- this is the
// one color-coded signal in the reference screenshot that's driven by data
// rather than by a fixed per-list/per-smart-list color.
function dueDateHtml(r: Reminder): string {
  if (!r.dueDate) return "";
  const due = new Date(r.dueDate);
  const isOverdue = !r.completed && due.getTime() < Date.now();
  const cls = isOverdue ? "reminder-due reminder-due-overdue" : "reminder-due";
  return `<div class="${cls}">${escapeHtml(due.toLocaleString("ja-JP"))}</div>`;
}

// The flag is repurposed in this app as an explicit "I've started this"
// signal (a user decision -- see handan/0023): tapping it toggles flagged
// directly from the row, no need to open the edit sheet. Unflagged shows a
// faint outline glyph (an invitation to tap), flagged shows a solid one.
function flagToggleHtml(r: Reminder): string {
  const icon = r.flagged ? "🚩" : "⚑";
  const cls = r.flagged ? "reminder-flag reminder-flag-active" : "reminder-flag reminder-flag-inactive";
  const label = r.flagged ? "着手中(タップで解除)" : "タップで着手中にする";
  return `<div class="${cls}" data-reminder-id="${escapeHtml(r.id)}" title="${label}">${icon}</div>`;
}

function renderReminders(reminders: Reminder[], showListTitle = false, emptyMessage = "リマインダーはありません。") {
  if (!remindersListEl) return;
  if (reminders.length === 0) {
    remindersListEl.innerHTML = emptyMessage ? `<li class="reminder-empty-state">${escapeHtml(emptyMessage)}</li>` : "";
    return;
  }
  remindersListEl.innerHTML = reminders
    .map((r) => {
      const listBadge =
        showListTitle && "listTitle" in r ? `<div class="item-footer">${escapeHtml((r as AggregatedReminder).listTitle)}</div>` : "";
      return `
        <li data-reminder-id="${escapeHtml(r.id)}">
          <div class="item-content">
            <div class="item-media">
              <input type="checkbox" class="reminder-checkbox" data-reminder-id="${escapeHtml(r.id)}" ${r.completed ? "checked" : ""} />
            </div>
            <div class="item-inner">
              <div class="reminder-title-row">
                <div class="item-title">${escapeHtml(r.title)}</div>
                ${flagToggleHtml(r)}
              </div>
              ${dueDateHtml(r)}
              ${r.desc ? `<div class="item-text">${escapeHtml(r.desc)}</div>` : ""}
              ${listBadge}
            </div>
          </div>
        </li>`;
    })
    .join("");
}

/// Cached from the last `list_lists` fetch so smart lists (which aggregate
/// across every list) don't need to re-fetch it on every click.
let cachedLists: RemindersList[] = [];

type AggregatedReminder = Reminder & { listTitle: string };

async function fetchAllReminders(): Promise<AggregatedReminder[]> {
  const perList = await Promise.all(
    cachedLists.map(async (list) => {
      const items = await invoke<Reminder[]>("list_reminders", {
        listId: list.id,
        includeCompleted: false,
      });
      return items.map((r) => ({ ...r, listTitle: list.title }));
    }),
  );
  return perList.flat();
}

/// End of "today" in local time -- used as the Today smart list's cutoff so
/// overdue items (due before today) are included too, matching the native
/// Reminders app's own Today behavior.
function endOfToday(): Date {
  const d = new Date();
  d.setHours(23, 59, 59, 999);
  return d;
}

type ViewState =
  | { kind: "list"; id: string; title: string }
  | { kind: "smart"; smart: SmartListKind; title: string }
  | { kind: "dashboard" };
let currentView: ViewState | null = null;
let currentReminders: (Reminder | AggregatedReminder)[] = [];

const editModeBtn = document.querySelector<HTMLElement>("#edit-mode-btn");
const addReminderBtn = document.querySelector<HTMLElement>("#add-reminder-btn");
const dashboardSummaryEl = document.querySelector<HTMLElement>("#dashboard-summary");
let editModeActive = false;

// Reordering is only meaningful for a single concrete list -- a smart list
// is a client-side aggregate across many lists' own independent orders, so
// there is no single "list" to persist a drag-reorder into.
function exitEditMode() {
  if (!editModeActive) return;
  editModeActive = false;
  if (remindersContainerEl) app.sortable.disable(remindersContainerEl);
  if (editModeBtn) editModeBtn.textContent = "編集";
}

/// Both the reminder-list view and the dashboard share one page-content
/// area; whichever is being entered shows itself and hides the other,
/// along with the toolbar actions that only make sense for a concrete list
/// (create/reorder -- the dashboard is read-only, aggregate-only).
function showRemindersList() {
  if (dashboardSummaryEl) dashboardSummaryEl.style.display = "none";
  if (remindersContainerEl) remindersContainerEl.style.display = "";
  if (addReminderBtn) addReminderBtn.style.display = "";
}

async function selectList(listId: string, title: string) {
  currentView = { kind: "list", id: listId, title };
  exitEditMode();
  if (editModeBtn) editModeBtn.style.display = "";
  if (mainTitleEl) mainTitleEl.textContent = title;
  if (remindersErrorEl) remindersErrorEl.textContent = "";
  try {
    const reminders = await invoke<Reminder[]>("list_reminders", {
      listId,
      includeCompleted: false,
    });
    currentReminders = reminders;
    renderReminders(reminders, false, "このリストにリマインダーはありません。");
    showRemindersList();
  } catch (err) {
    if (remindersErrorEl) remindersErrorEl.textContent = friendlyError(err);
  }
  app.panel.close("left");
}

type SmartListKind = "today" | "scheduled" | "flagged" | "all";

// Design decision (see project plan / design-critique notes): the native
// Reminders app's Today view is ascending (oldest first) and requires
// scrolling past old overdue items to see what's actually due soon. This
// deliberately inverts that -- newest/most-recent due date first, so the
// most time-sensitive items are immediately visible without scrolling.
async function selectSmartList(kind: SmartListKind, title: string) {
  currentView = { kind: "smart", smart: kind, title };
  exitEditMode();
  if (editModeBtn) editModeBtn.style.display = "none";
  if (mainTitleEl) mainTitleEl.textContent = title;
  if (remindersErrorEl) remindersErrorEl.textContent = "";
  try {
    const all = await fetchAllReminders();
    let filtered: AggregatedReminder[];
    switch (kind) {
      case "today": {
        const cutoff = endOfToday();
        filtered = all.filter((r) => r.dueDate && new Date(r.dueDate) <= cutoff);
        filtered.sort((a, b) => new Date(b.dueDate as string).getTime() - new Date(a.dueDate as string).getTime());
        break;
      }
      case "scheduled": {
        filtered = all.filter((r) => r.dueDate);
        filtered.sort((a, b) => new Date(a.dueDate as string).getTime() - new Date(b.dueDate as string).getTime());
        break;
      }
      case "flagged":
        filtered = all.filter((r) => r.flagged);
        break;
      default:
        filtered = all;
    }
    currentReminders = filtered;
    renderReminders(filtered, true, SMART_LIST_EMPTY_MESSAGES[kind]);
    showRemindersList();
  } catch (err) {
    if (remindersErrorEl) remindersErrorEl.textContent = friendlyError(err);
  }
  app.panel.close("left");
}

/// GUI-11's "new usage experience", redefined per user feedback (handan/0023):
/// not an at-a-glance count summary, but a single prioritized, actionable
/// queue -- work from the top down and it's handled. Priority order:
/// overdue (longest-neglected first) -> in-progress/flagged -> due today
/// -> everything else marked high priority. A reminder appears exactly
/// once, in the highest-priority bucket it qualifies for.
async function selectDashboard() {
  currentView = { kind: "dashboard" };
  exitEditMode();
  if (editModeBtn) editModeBtn.style.display = "none";
  if (addReminderBtn) addReminderBtn.style.display = "";
  if (mainTitleEl) mainTitleEl.textContent = "ダッシュボード";
  if (remindersErrorEl) remindersErrorEl.textContent = "";
  try {
    const all = await fetchAllReminders();
    const cutoff = endOfToday();
    const now = Date.now();

    const seen = new Set<string>();
    const byDueAsc = (a: AggregatedReminder, b: AggregatedReminder) =>
      new Date(a.dueDate as string).getTime() - new Date(b.dueDate as string).getTime();
    const bucket = (
      pred: (r: AggregatedReminder) => boolean,
      sort: (a: AggregatedReminder, b: AggregatedReminder) => number,
    ): AggregatedReminder[] => {
      const items = all.filter((r) => pred(r) && !seen.has(r.id)).sort(sort);
      items.forEach((r) => seen.add(r.id));
      return items;
    };

    const overdue = bucket((r) => r.dueDate !== null && new Date(r.dueDate).getTime() < now, byDueAsc);
    const inProgress = bucket((r) => r.flagged, byDueAsc);
    const dueToday = bucket((r) => r.dueDate !== null && new Date(r.dueDate).getTime() <= cutoff.getTime(), byDueAsc);
    const highPriority = bucket((r) => r.priority === 1, byDueAsc);
    const queue = [...overdue, ...inProgress, ...dueToday, ...highPriority];

    if (dashboardSummaryEl) {
      dashboardSummaryEl.textContent =
        queue.length === 0
          ? "今取り組むべきことはありません。"
          : `上から順に片付けましょう — 期限切れ ${overdue.length}件 / 着手中 ${inProgress.length}件 / ` +
            `今日 ${dueToday.length}件 / 優先度高 ${highPriority.length}件`;
      dashboardSummaryEl.style.display = "";
    }

    currentReminders = queue;
    // No separate empty-state row here -- the summary line above already
    // says "nothing to work on" when the queue is empty.
    renderReminders(queue, true, "");
    if (dashboardSummaryEl) dashboardSummaryEl.style.display = "";
    if (remindersContainerEl) remindersContainerEl.style.display = "";
    if (addReminderBtn) addReminderBtn.style.display = "";
  } catch (err) {
    if (remindersErrorEl) remindersErrorEl.textContent = friendlyError(err);
  }
  app.panel.close("left");
}

async function refreshCurrentView() {
  if (!currentView) return;
  if (currentView.kind === "list") await selectList(currentView.id, currentView.title);
  else if (currentView.kind === "smart") await selectSmartList(currentView.smart, currentView.title);
  else await selectDashboard();
}

const SMART_LIST_TITLES: Record<SmartListKind, string> = {
  today: "今日",
  scheduled: "予定",
  flagged: "フラグ付き",
  all: "すべて",
};

const SMART_LIST_EMPTY_MESSAGES: Record<SmartListKind, string> = {
  today: "今日期限のリマインダーはありません。",
  scheduled: "期限が設定されたリマインダーはありません。",
  flagged: "着手中のリマインダーはありません。",
  all: "リマインダーはありません。",
};

function bindListSelection() {
  const panelContent = document.querySelector<HTMLElement>("#lists-panel-content");
  panelContent?.addEventListener("click", (e) => {
    const target = e.target as HTMLElement;

    if (target.closest("#dashboard-nav-item")) {
      e.preventDefault();
      void selectDashboard();
      return;
    }

    const smartLink = target.closest<HTMLElement>(".smart-item");
    if (smartLink) {
      e.preventDefault();
      const kind = smartLink.dataset.smartId as SmartListKind | undefined;
      if (kind) void selectSmartList(kind, SMART_LIST_TITLES[kind]);
      return;
    }

    const listLink = target.closest<HTMLElement>(".list-item");
    if (listLink) {
      e.preventDefault();
      const listId = listLink.dataset.listId;
      const title = listLink.querySelector(".item-title")?.textContent ?? "";
      if (listId) void selectList(listId, title);
    }
  });
}

// Completing a task ends its "in progress" status (see flagToggleHtml) and
// -- per the task-lifecycle design agreed with the user -- immediately
// offers to capture whatever comes next, rather than letting momentum
// drop once a task is checked off.
async function toggleCompleted(id: string, completed: boolean) {
  const r = currentReminders.find((x) => x.id === id);
  if (!r) return;
  try {
    const patch: Partial<Reminder> = completed && r.flagged ? { completed, flagged: false } : { completed };
    await withLoading(() => invoke<Reminder>("update_reminder", { reminder: { ...r, ...patch } }));
    await refreshCurrentView();
    if (completed) promptNextTask(r);
  } catch (err) {
    await reportMutationError(err, remindersErrorEl);
  }
}

// The flag is this app's explicit "I've started this" signal -- tapping it
// toggles that state directly from the row, no need to open the edit sheet.
async function toggleFlag(id: string) {
  const r = currentReminders.find((x) => x.id === id);
  if (!r) return;
  try {
    const updated = await withLoading(() => invoke<Reminder>("update_reminder", { reminder: { ...r, flagged: !r.flagged } }));
    const idx = currentReminders.findIndex((x) => x.id === id);
    if (idx >= 0) {
      currentReminders[idx] = { ...currentReminders[idx], ...updated };
      renderReminders(currentReminders, currentView?.kind !== "list");
    }
  } catch (err) {
    await reportMutationError(err, remindersErrorEl);
  }
}

const editSheet = app.sheet.create({
  el: document.querySelector("#edit-sheet") as HTMLElement,
  backdrop: true,
});

let editingReminder: Reminder | AggregatedReminder | null = null;

function toLocalDatetimeInputValue(d: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

function openEditSheet(id: string) {
  const r = currentReminders.find((x) => x.id === id);
  if (!r) return;
  editingReminder = r;

  const titleInput = document.querySelector<HTMLInputElement>("#edit-title");
  const notesInput = document.querySelector<HTMLTextAreaElement>("#edit-notes");
  const priorityInput = document.querySelector<HTMLSelectElement>("#edit-priority");
  const flaggedInput = document.querySelector<HTMLInputElement>("#edit-flagged");
  const dueInput = document.querySelector<HTMLInputElement>("#edit-due-date");
  const errEl = document.querySelector<HTMLElement>("#edit-error");

  if (titleInput) titleInput.value = r.title;
  if (notesInput) notesInput.value = r.desc;
  if (priorityInput) priorityInput.value = String(r.priority);
  if (flaggedInput) flaggedInput.checked = r.flagged;
  if (dueInput) dueInput.value = r.dueDate ? toLocalDatetimeInputValue(new Date(r.dueDate)) : "";
  if (errEl) errEl.textContent = "";

  editSheet.open();
}

// Every field applies immediately on change -- deliberately not gated
// behind an explicit "Done"/save button (see project plan: the native
// Reminders app delays applying a date change until "Done" is tapped,
// which design-critique sources flag as a source of confusion).
async function applyEdit(patch: Partial<Reminder>) {
  if (!editingReminder) return;
  const errEl = document.querySelector<HTMLElement>("#edit-error");
  if (errEl) errEl.textContent = "";
  try {
    const merged = { ...editingReminder, ...patch };
    const updated = await withLoading(() => invoke<Reminder>("update_reminder", { reminder: merged }));
    editingReminder = { ...editingReminder, ...updated };
    const idx = currentReminders.findIndex((x) => x.id === updated.id);
    if (idx >= 0) {
      currentReminders[idx] = { ...currentReminders[idx], ...updated };
      renderReminders(currentReminders, currentView?.kind === "smart");
    }
  } catch (err) {
    await reportMutationError(err, errEl);
  }
}

function bindEditSheet() {
  document.querySelector("#edit-title")?.addEventListener("change", (e) => {
    void applyEdit({ title: (e.target as HTMLInputElement).value });
  });
  document.querySelector("#edit-notes")?.addEventListener("change", (e) => {
    void applyEdit({ desc: (e.target as HTMLTextAreaElement).value });
  });
  document.querySelector("#edit-priority")?.addEventListener("change", (e) => {
    void applyEdit({ priority: Number((e.target as HTMLSelectElement).value) });
  });
  document.querySelector("#edit-flagged")?.addEventListener("change", (e) => {
    void applyEdit({ flagged: (e.target as HTMLInputElement).checked });
  });
  document.querySelector("#edit-due-date")?.addEventListener("change", (e) => {
    const v = (e.target as HTMLInputElement).value;
    void applyEdit({ dueDate: v ? new Date(v).toISOString() : null });
  });
  document.querySelector("#edit-delete-btn")?.addEventListener("click", () => {
    if (!editingReminder) return;
    const reminder = editingReminder;
    const errEl = document.querySelector<HTMLElement>("#edit-error");
    app.dialog.confirm(`「${reminder.title}」を削除しますか？この操作は取り消せません。`, async () => {
      try {
        await withLoading(() => invoke("delete_reminder", { reminder }));
        editSheet.close();
        await refreshCurrentView();
      } catch (err) {
        await reportMutationError(err, errEl);
      }
    });
  });
  document.querySelector("#edit-close-btn")?.addEventListener("click", () => editSheet.close());
}

function bindReminderInteractions() {
  remindersListEl?.addEventListener("click", (e) => {
    if (editModeActive) return; // dragging/deleting, not tap-to-edit, while reordering
    const target = e.target as HTMLElement;

    const checkbox = target.closest<HTMLInputElement>(".reminder-checkbox");
    if (checkbox) {
      e.stopPropagation();
      const id = checkbox.dataset.reminderId;
      if (id) void toggleCompleted(id, checkbox.checked);
      return;
    }

    const flag = target.closest<HTMLElement>(".reminder-flag");
    if (flag) {
      e.stopPropagation();
      const id = flag.dataset.reminderId;
      if (id) void toggleFlag(id);
      return;
    }

    const li = target.closest<HTMLElement>("li[data-reminder-id]");
    if (li?.dataset.reminderId) openEditSheet(li.dataset.reminderId);
  });
}

// After a drag-reorder, read the new order straight off the DOM (rather
// than trusting the sortable event's own from/to indices) so this stays
// correct regardless of exactly how Framework7 reports the move.
async function handleReorder() {
  if (!currentView || currentView.kind !== "list" || !remindersListEl) return;
  const listId = currentView.id;
  const ids = Array.from(remindersListEl.querySelectorAll<HTMLElement>("li[data-reminder-id]"))
    .map((li) => li.dataset.reminderId)
    .filter((id): id is string => Boolean(id));
  const list = cachedLists.find((l) => l.id === listId);
  if (!list) return;
  if (remindersErrorEl) remindersErrorEl.textContent = "";
  try {
    await withLoading(async () => {
      await invoke("reorder_list", { list, newOrder: ids });
      // The list record's own change tag just advanced server-side; refresh
      // the cache so a second reorder in this session doesn't submit a
      // stale tag and get rejected by CloudKit's optimistic-concurrency
      // check.
      cachedLists = await invoke<RemindersList[]>("list_lists");
    });
  } catch (err) {
    await reportMutationError(err, remindersErrorEl);
  }
}

function bindEditModeToggle() {
  editModeBtn?.addEventListener("click", (e) => {
    e.preventDefault();
    if (!remindersContainerEl || !currentView || currentView.kind !== "list") return;
    editModeActive = !editModeActive;
    if (editModeActive) {
      app.sortable.enable(remindersContainerEl);
      editModeBtn.textContent = "完了";
    } else {
      app.sortable.disable(remindersContainerEl);
      editModeBtn.textContent = "編集";
    }
  });

  remindersListEl?.addEventListener("sortable:sort", () => {
    void handleReorder();
  });
}

const createSheet = app.sheet.create({
  el: document.querySelector("#create-sheet") as HTMLElement,
  backdrop: true,
});

// Creating a reminder needs a target list regardless of which view is
// currently open (dashboard, a smart list, or a concrete list) -- it used
// to silently no-op unless a concrete list was selected, which broke
// entirely once the dashboard became the default landing view (GUI-11).
function populateCreateListPicker(preferredListId?: string) {
  const select = document.querySelector<HTMLSelectElement>("#create-list-id");
  if (!select) return;
  const preselectId = preferredListId ?? (currentView?.kind === "list" ? currentView.id : undefined);
  select.innerHTML = cachedLists
    .map((l) => `<option value="${escapeHtml(l.id)}">${escapeHtml(l.title)}</option>`)
    .join("");
  if (preselectId) select.value = preselectId;
}

const CREATE_SHEET_DEFAULT_TITLE = "新規リマインダー";

function openCreateSheet(heading: string, preferredListId?: string) {
  if (cachedLists.length === 0) return;
  const headingEl = document.querySelector<HTMLElement>("#create-sheet-title");
  if (headingEl) headingEl.textContent = heading;
  const titleInput = document.querySelector<HTMLInputElement>("#create-title");
  if (titleInput) titleInput.value = "";
  populateCreateListPicker(preferredListId);
  const errEl = document.querySelector<HTMLElement>("#create-error");
  if (errEl) errEl.textContent = "";
  createSheet.open();
}

// Per the task-lifecycle design agreed with the user (handan/0023): a
// completed task should immediately invite capturing whatever comes next,
// rather than just disappearing -- reuses the same create Sheet, scoped to
// the list the just-completed reminder belonged to.
function promptNextTask(completed: Reminder | AggregatedReminder) {
  openCreateSheet(`「${completed.title}」を完了しました。次のタスクを追加しますか?`, completed.listId);
}

function bindCreateSheet() {
  const addBtn = document.querySelector("#add-reminder-btn");
  addBtn?.addEventListener("click", (e) => {
    e.preventDefault();
    openCreateSheet(CREATE_SHEET_DEFAULT_TITLE);
  });

  document.querySelector("#create-skip-btn")?.addEventListener("click", () => createSheet.close());

  const form = document.querySelector<HTMLFormElement>("#create-form");
  form?.addEventListener("submit", async (e) => {
    e.preventDefault();
    const title = document.querySelector<HTMLInputElement>("#create-title")?.value.trim() ?? "";
    const listId = document.querySelector<HTMLSelectElement>("#create-list-id")?.value ?? "";
    const errEl = document.querySelector<HTMLElement>("#create-error");
    if (!title || !listId) return;
    try {
      await withLoading(() =>
        invoke("create_reminder", {
          listId,
          title,
          notes: "",
          priority: 0,
          flagged: false,
          dueDate: null,
        }),
      );
      createSheet.close();
      await refreshCurrentView();
    } catch (err) {
      if (errEl) errEl.textContent = friendlyError(err);
    }
  });
}

async function onReady() {
  setStatus("");
  bindListSelection();
  bindReminderInteractions();
  bindEditSheet();
  bindCreateSheet();
  bindEditModeToggle();
  if (listsErrorEl) listsErrorEl.textContent = "";
  try {
    const lists = await invoke<RemindersList[]>("list_lists");
    cachedLists = lists;
    renderLists(lists);
    // The dashboard (GUI-11's "new usage experience") is the default
    // landing view now, rather than auto-selecting the first list.
    await selectDashboard();
  } catch (err) {
    if (listsErrorEl) listsErrorEl.textContent = friendlyError(err);
  }
}

function bindLoginForm() {
  const form = document.querySelector<HTMLFormElement>("#login-form");
  const errorEl = document.querySelector<HTMLElement>("#login-error");
  form?.addEventListener("submit", async (e) => {
    e.preventDefault();
    const appleId = document.querySelector<HTMLInputElement>("#login-apple-id")?.value.trim() ?? "";
    const password = document.querySelector<HTMLInputElement>("#login-password")?.value ?? "";
    if (errorEl) errorEl.textContent = "";
    try {
      const result = await invoke<LoginResult>("login", { appleId, password });
      if (result.status === "two_factor_required") {
        loginSheet.close();
        twoFactorSheet.open();
      } else {
        loginSheet.close();
        await onReady();
      }
    } catch (err) {
      if (errorEl) errorEl.textContent = String(err);
    }
  });
}

function bindTwoFactorForm() {
  const form = document.querySelector<HTMLFormElement>("#two-factor-form");
  const errorEl = document.querySelector<HTMLElement>("#two-factor-error");
  form?.addEventListener("submit", async (e) => {
    e.preventDefault();
    const code = document.querySelector<HTMLInputElement>("#two-factor-code")?.value.trim() ?? "";
    if (errorEl) errorEl.textContent = "";
    try {
      await invoke("submit_two_factor_code", { code });
      twoFactorSheet.close();
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
    loginSheet.open();
    return;
  }

  const resumed = await invoke<boolean>("try_resume", { appleId });
  if (resumed) {
    await onReady();
    return;
  }

  setStatus("<p>再ログインが必要です。</p>");
  const idInput = document.querySelector<HTMLInputElement>("#login-apple-id");
  if (idInput) idInput.value = appleId;
  loginSheet.open();
}

boot();

export default app;
