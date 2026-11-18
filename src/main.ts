import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

interface Config {
  title: string;
  message: string;
  photo_path: string | null;
  popup_on_unlock: boolean;
  popup_on_display_wake: boolean;
  set_lock_screen: boolean;
  delay_ms: number;
  dismiss_secs: number;
  style: string;
  theme: string;
  font: string;
  accent: string;
  size: string;
  position: string;
  show_clock: boolean;
  show_timer: boolean;
  schedule_enabled: boolean;
  schedule_mode: string;
  schedule_interval_min: number;
  schedule_time: string;
  schedule_days: number[];
}

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const status = $("status");
const thumb = $("thumb");
const autostart = $<HTMLInputElement>("autostart");
const accentInput = $<HTMLInputElement>("accent");
const interval = $<HTMLInputElement>("interval");

let cfg: Config;
const seg: Record<string, string> = {};
let days = new Set<number>();
let accent = "#ffc933";
let photoPath: string | null = null;

function say(text: string, error = false) {
  status.textContent = text;
  status.classList.toggle("error", error);
}

// ---------- tabs
document.querySelectorAll<HTMLButtonElement>(".tab").forEach((t) =>
  t.addEventListener("click", () => {
    document.querySelectorAll(".tab").forEach((x) => {
      x.classList.toggle("on", x === t);
      x.setAttribute("aria-selected", String(x === t));
    });
    document.querySelectorAll(".panel").forEach((p) => p.classList.toggle("on", p.id === `tab-${t.dataset.tab}`));
  }),
);

// ---------- segmented groups (theme, font, style, size, mode)
function paintSegs() {
  document.querySelectorAll<HTMLElement>("[data-seg]").forEach((g) => {
    const key = g.dataset.seg!;
    g.querySelectorAll<HTMLButtonElement>("button[data-v]").forEach((b) =>
      b.setAttribute("aria-pressed", String(b.dataset.v === seg[key])),
    );
  });
  $("mode-interval").classList.toggle("on", seg.schedule_mode !== "daily");
  $("mode-daily").classList.toggle("on", seg.schedule_mode === "daily");
}
document.querySelectorAll<HTMLElement>("[data-seg]").forEach((g) =>
  g.addEventListener("click", (e) => {
    const b = (e.target as HTMLElement).closest<HTMLButtonElement>("button[data-v]");
    if (!b) return;
    seg[g.dataset.seg!] = b.dataset.v!;
    paintSegs();
  }),
);

// ---------- days, quick intervals, accent
function paintDays() {
  document.querySelectorAll<HTMLButtonElement>("#days button").forEach((b) =>
    b.setAttribute("aria-pressed", String(days.has(Number(b.dataset.d)))),
  );
}
$("days").addEventListener("click", (e) => {
  const b = (e.target as HTMLElement).closest<HTMLButtonElement>("button[data-d]");
  if (!b) return;
  const d = Number(b.dataset.d);
  days.has(d) ? days.delete(d) : days.add(d);
  paintDays();
});
$("quick").addEventListener("click", (e) => {
  const b = (e.target as HTMLElement).closest<HTMLButtonElement>("button[data-m]");
  if (b) interval.value = b.dataset.m!;
});

function paintAccent() {
  document.querySelectorAll<HTMLButtonElement>("#swatches button").forEach((b) =>
    b.setAttribute("aria-pressed", String(b.dataset.c === accent)),
  );
  accentInput.value = accent;
  document.documentElement.style.setProperty("--gold", accent);
}
$("swatches").addEventListener("click", (e) => {
  const b = (e.target as HTMLElement).closest<HTMLButtonElement>("button[data-c]");
  if (!b) return;
  accent = b.dataset.c!;
  paintAccent();
});
accentInput.addEventListener("input", () => {
  accent = accentInput.value;
  paintAccent();
});

function paintSchedule() {
  $("schedule-body").classList.toggle("off", !$<HTMLInputElement>("schedule_enabled").checked);
}
$("schedule_enabled").addEventListener("change", paintSchedule);

// ---------- photo
async function showPhoto() {
  const uri = photoPath ? await invoke<string | null>("photo_preview", { path: photoPath }) : null;
  thumb.style.backgroundImage = uri ? `url("${uri}")` : "";
  thumb.classList.toggle("has", !!uri);
  if (photoPath && !uri) say("That photo couldn't be read. Try a PNG, JPG, WebP or BMP file.", true);
}
$("pick").addEventListener("click", async () => {
  const picked = await open({
    multiple: false,
    filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp", "bmp"] }],
  });
  if (typeof picked === "string") {
    photoPath = picked;
    await showPhoto();
  }
});
$("clear").addEventListener("click", async () => {
  photoPath = null;
  await showPhoto();
});

// ---------- read / fill
function fill(c: Config) {
  document.querySelectorAll<HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement>("[data-k]").forEach((el) => {
    const v = (c as unknown as Record<string, unknown>)[el.dataset.k!];
    if (el instanceof HTMLInputElement && el.type === "checkbox") el.checked = !!v;
    else el.value = String(Number(el.dataset.mul) ? (v as number) / Number(el.dataset.mul) : v);
  });
  for (const k of ["style", "theme", "font", "size", "schedule_mode"] as const) seg[k] = c[k];
  days = new Set(c.schedule_days);
  accent = c.accent;
  photoPath = c.photo_path;
  paintSegs();
  paintDays();
  paintAccent();
  paintSchedule();
}

function read(): Config {
  const c = { ...cfg } as unknown as Record<string, unknown>;
  document.querySelectorAll<HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement>("[data-k]").forEach((el) => {
    const k = el.dataset.k!;
    if (el instanceof HTMLInputElement && el.type === "checkbox") c[k] = el.checked;
    else if (el instanceof HTMLInputElement && el.type === "number")
      c[k] = Math.max(0, Math.round((Number(el.value) || 0) * (Number(el.dataset.mul) || 1)));
    else c[k] = el.value.trim();
  });
  Object.assign(c, seg, { accent, photo_path: photoPath, schedule_days: [...days].sort() });
  if (!(c.schedule_interval_min as number)) c.schedule_interval_min = 1;
  return c as unknown as Config;
}

async function save(): Promise<boolean> {
  say("Saving…");
  try {
    const next = read();
    const msg = await invoke<string>("save_config", { config: next });
    await invoke("set_autostart", { enabled: autostart.checked });
    cfg = next;
    say(msg, /couldn't|failed/.test(msg));
    return true;
  } catch (e) {
    say(String(e), true);
    return false;
  }
}

$("save").addEventListener("click", () => void save());
$("test").addEventListener("click", async () => {
  if (await save()) await invoke("trigger_popup");
});

const mainEl = document.querySelector("main")!;
const unsaved = () => say("Unsaved changes. Press Save to apply them.");
mainEl.addEventListener("input", unsaved);
mainEl.addEventListener("click", (e) => {
  if ((e.target as HTMLElement).closest("button")) unsaved();
});

(async () => {
  cfg = await invoke<Config>("get_config");
  fill(cfg);
  autostart.checked = await invoke<boolean>("get_autostart");
  await showPhoto();
})().catch((e) => say(String(e), true));
