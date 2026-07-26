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

type LoginResult = { status: "complete" | "two_factor_required" };

type RemindersList = {
  id: string;
  title: string;
  reminderIds: string[];
  recordChangeTag: string | null;
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
            <div class="item-inner">
              <div class="item-title">${escapeHtml(l.title)}</div>
              <div class="item-after">${l.reminderIds.length}</div>
            </div>
          </a>
        </li>`,
    )
    .join("");
}

function renderReminders(reminders: Reminder[], showListTitle = false) {
  if (!remindersListEl) return;
  remindersListEl.innerHTML = reminders
    .map((r) => {
      const due = r.dueDate ? new Date(r.dueDate).toLocaleString("ja-JP") : "";
      const listBadge =
        showListTitle && "listTitle" in r ? `<div class="item-footer">${escapeHtml((r as AggregatedReminder).listTitle)}</div>` : "";
      return `
        <li data-reminder-id="${escapeHtml(r.id)}">
          <div class="item-content">
            <div class="item-media">
              <input type="checkbox" class="reminder-checkbox" data-reminder-id="${escapeHtml(r.id)}" ${r.completed ? "checked" : ""} />
            </div>
            <div class="item-inner">
              <div class="item-title-row">
                <div class="item-title">${escapeHtml(r.title)}</div>
                ${r.flagged ? '<div class="item-after">🚩</div>' : ""}
              </div>
              ${due || r.desc ? `<div class="item-subtitle">${escapeHtml(due)}</div>` : ""}
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

type ViewState = { kind: "list"; id: string; title: string } | { kind: "smart"; smart: SmartListKind; title: string };
let currentView: ViewState | null = null;
let currentReminders: (Reminder | AggregatedReminder)[] = [];

async function selectList(listId: string, title: string) {
  currentView = { kind: "list", id: listId, title };
  if (mainTitleEl) mainTitleEl.textContent = title;
  if (remindersErrorEl) remindersErrorEl.textContent = "";
  try {
    const reminders = await invoke<Reminder[]>("list_reminders", {
      listId,
      includeCompleted: false,
    });
    currentReminders = reminders;
    renderReminders(reminders);
    if (remindersContainerEl) remindersContainerEl.style.display = "";
  } catch (err) {
    if (remindersErrorEl) remindersErrorEl.textContent = String(err);
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
    renderReminders(filtered, true);
    if (remindersContainerEl) remindersContainerEl.style.display = "";
  } catch (err) {
    if (remindersErrorEl) remindersErrorEl.textContent = String(err);
  }
  app.panel.close("left");
}

async function refreshCurrentView() {
  if (!currentView) return;
  if (currentView.kind === "list") await selectList(currentView.id, currentView.title);
  else await selectSmartList(currentView.smart, currentView.title);
}

const SMART_LIST_TITLES: Record<SmartListKind, string> = {
  today: "今日",
  scheduled: "予定",
  flagged: "フラグ付き",
  all: "すべて",
};

function bindListSelection() {
  const panelContent = document.querySelector<HTMLElement>("#lists-panel-content");
  panelContent?.addEventListener("click", (e) => {
    const target = e.target as HTMLElement;

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

async function toggleCompleted(id: string, completed: boolean) {
  const r = currentReminders.find((x) => x.id === id);
  if (!r) return;
  try {
    await invoke<Reminder>("update_reminder", { reminder: { ...r, completed } });
    await refreshCurrentView();
  } catch (err) {
    if (remindersErrorEl) remindersErrorEl.textContent = String(err);
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
    const updated = await invoke<Reminder>("update_reminder", { reminder: merged });
    editingReminder = { ...editingReminder, ...updated };
    const idx = currentReminders.findIndex((x) => x.id === updated.id);
    if (idx >= 0) {
      currentReminders[idx] = { ...currentReminders[idx], ...updated };
      renderReminders(currentReminders, currentView?.kind === "smart");
    }
  } catch (err) {
    if (errEl) errEl.textContent = String(err);
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
  document.querySelector("#edit-delete-btn")?.addEventListener("click", async () => {
    if (!editingReminder) return;
    const errEl = document.querySelector<HTMLElement>("#edit-error");
    try {
      await invoke("delete_reminder", { reminder: editingReminder });
      editSheet.close();
      await refreshCurrentView();
    } catch (err) {
      if (errEl) errEl.textContent = String(err);
    }
  });
  document.querySelector("#edit-close-btn")?.addEventListener("click", () => editSheet.close());
}

function bindReminderInteractions() {
  remindersListEl?.addEventListener("click", (e) => {
    const target = e.target as HTMLElement;

    const checkbox = target.closest<HTMLInputElement>(".reminder-checkbox");
    if (checkbox) {
      e.stopPropagation();
      const id = checkbox.dataset.reminderId;
      if (id) void toggleCompleted(id, checkbox.checked);
      return;
    }

    const li = target.closest<HTMLElement>("li[data-reminder-id]");
    if (li?.dataset.reminderId) openEditSheet(li.dataset.reminderId);
  });
}

const createSheet = app.sheet.create({
  el: document.querySelector("#create-sheet") as HTMLElement,
  backdrop: true,
});

function bindCreateSheet() {
  const addBtn = document.querySelector("#add-reminder-btn");
  addBtn?.addEventListener("click", (e) => {
    e.preventDefault();
    if (!currentView || currentView.kind !== "list") return;
    const titleInput = document.querySelector<HTMLInputElement>("#create-title");
    if (titleInput) titleInput.value = "";
    const errEl = document.querySelector<HTMLElement>("#create-error");
    if (errEl) errEl.textContent = "";
    createSheet.open();
  });

  const form = document.querySelector<HTMLFormElement>("#create-form");
  form?.addEventListener("submit", async (e) => {
    e.preventDefault();
    if (!currentView || currentView.kind !== "list") return;
    const title = document.querySelector<HTMLInputElement>("#create-title")?.value.trim() ?? "";
    const errEl = document.querySelector<HTMLElement>("#create-error");
    if (!title) return;
    try {
      await invoke("create_reminder", {
        listId: currentView.id,
        title,
        notes: "",
        priority: 0,
        flagged: false,
        dueDate: null,
      });
      createSheet.close();
      await refreshCurrentView();
    } catch (err) {
      if (errEl) errEl.textContent = String(err);
    }
  });
}

async function onReady() {
  setStatus("");
  bindListSelection();
  bindReminderInteractions();
  bindEditSheet();
  bindCreateSheet();
  if (listsErrorEl) listsErrorEl.textContent = "";
  try {
    const lists = await invoke<RemindersList[]>("list_lists");
    cachedLists = lists;
    renderLists(lists);
    if (lists.length > 0) {
      await selectList(lists[0].id, lists[0].title);
    }
  } catch (err) {
    if (listsErrorEl) listsErrorEl.textContent = String(err);
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
