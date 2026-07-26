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

const statusBlock = document.querySelector<HTMLElement>("#status-block");
function setStatus(html: string) {
  if (statusBlock) statusBlock.innerHTML = html;
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

function onReady() {
  setStatus("<p>ログイン済みです。次のマイルストーンでリスト表示を実装します。</p>");
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
        onReady();
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
      onReady();
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
    onReady();
    return;
  }

  setStatus("<p>再ログインが必要です。</p>");
  const idInput = document.querySelector<HTMLInputElement>("#login-apple-id");
  if (idInput) idInput.value = appleId;
  loginSheet.open();
}

boot();

export default app;
