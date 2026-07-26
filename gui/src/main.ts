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

function renderReminders(reminders: Reminder[]) {
  if (!remindersListEl) return;
  remindersListEl.innerHTML = reminders
    .map((r) => {
      const due = r.dueDate ? new Date(r.dueDate).toLocaleString("ja-JP") : "";
      return `
        <li>
          <div class="item-content">
            <div class="item-media">
              <input type="checkbox" disabled ${r.completed ? "checked" : ""} />
            </div>
            <div class="item-inner">
              <div class="item-title-row">
                <div class="item-title">${escapeHtml(r.title)}</div>
                ${r.flagged ? '<div class="item-after">🚩</div>' : ""}
              </div>
              ${due || r.desc ? `<div class="item-subtitle">${escapeHtml(due)}</div>` : ""}
              ${r.desc ? `<div class="item-text">${escapeHtml(r.desc)}</div>` : ""}
            </div>
          </div>
        </li>`;
    })
    .join("");
}

async function selectList(listId: string, title: string) {
  if (mainTitleEl) mainTitleEl.textContent = title;
  if (remindersErrorEl) remindersErrorEl.textContent = "";
  try {
    const reminders = await invoke<Reminder[]>("list_reminders", {
      listId,
      includeCompleted: false,
    });
    renderReminders(reminders);
    if (remindersContainerEl) remindersContainerEl.style.display = "";
  } catch (err) {
    if (remindersErrorEl) remindersErrorEl.textContent = String(err);
  }
  app.panel.close("left");
}

function bindListSelection() {
  listsListEl?.addEventListener("click", (e) => {
    const link = (e.target as HTMLElement).closest<HTMLElement>(".list-item");
    if (!link) return;
    e.preventDefault();
    const listId = link.dataset.listId;
    const title = link.querySelector(".item-title")?.textContent ?? "";
    if (listId) void selectList(listId, title);
  });
}

async function onReady() {
  setStatus("");
  bindListSelection();
  if (listsErrorEl) listsErrorEl.textContent = "";
  try {
    const lists = await invoke<RemindersList[]>("list_lists");
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
