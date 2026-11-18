import { invoke } from "@tauri-apps/api/core";

interface PopupData {
  title: string;
  message: string;
  photo: string | null;
  dismiss_secs: number;
  style: "minimal" | "glide" | "funky";
  theme: string;
  font: string;
  accent: string;
  show_clock: boolean;
  show_timer: boolean;
  scale: number;
  pad: number;
}

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const reduceMotion = matchMedia("(prefers-reduced-motion: reduce)").matches;
const COLORS = ["#ff3d81", "#ffe14d", "#2de2e6", "#7b5cff", "#5ef2a0", "#ff8a3d"];

function luminance(hex: string): number {
  const n = parseInt(hex.slice(1), 16);
  const [r, g, b] = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((v) => {
    const c = v / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** Confetti bursts out from the middle of the window and stays inside it. */
function confetti(count: number) {
  if (reduceMotion) return;
  const layer = $("confetti");
  const w = innerWidth, h = innerHeight;
  for (let i = 0; i < count; i++) {
    const size = 6 + Math.random() * 9;
    const round = Math.random() < 0.33;
    const el = document.createElement("i");
    el.style.cssText =
      `position:absolute;left:50%;top:50%;width:${size}px;height:${round ? size : size * 1.8}px;` +
      `background:${COLORS[i % COLORS.length]};border-radius:${round ? "50%" : "2px"};`;
    layer.appendChild(el);
    const a = Math.random() * Math.PI * 2;
    const cos = Math.cos(a), sin = Math.sin(a);
    const edge = Math.min(w / 2 / Math.max(Math.abs(cos), 0.001), h / 2 / Math.max(Math.abs(sin), 0.001));
    const dist = edge * (0.45 + Math.random() * 0.5);
    const dx = cos * dist, dy = sin * dist;
    const rot = (Math.random() - 0.5) * 900;
    const anim = el.animate(
      [
        { transform: "translate(-50%,-50%) scale(.2) rotate(0deg)", opacity: 1 },
        { transform: `translate(calc(-50% + ${dx}px),calc(-50% + ${dy}px)) scale(1) rotate(${rot}deg)`, opacity: 1, offset: 0.75 },
        { transform: `translate(calc(-50% + ${dx}px),calc(-50% + ${dy + 18}px)) scale(1) rotate(${rot * 1.1}deg)`, opacity: 0 },
      ],
      { duration: 900 + Math.random() * 600, easing: "cubic-bezier(.12,.7,.3,1)", fill: "forwards" },
    );
    anim.onfinish = () => el.remove();
  }
}

async function init() {
  const d = await invoke<PopupData>("get_popup_data");
  const card = $("card");
  const root = document.documentElement;

  const accent = /^#[0-9a-f]{6}$/i.test(d.accent) ? d.accent : "#ffc933";
  root.style.fontSize = `${16 * d.scale}px`;
  root.style.setProperty("--pad", `${d.pad}px`);
  root.style.setProperty("--accent", accent);
  root.style.setProperty("--on-accent", luminance(accent) > 0.45 ? "#1a1033" : "#ffffff");
  document.body.classList.add(`style-${d.style}`, `theme-${d.theme}`, `font-${d.font}`);

  $("title").textContent = d.title || "Reminder";
  $("message").textContent = d.message;
  if (d.show_clock) {
    const t = $("clock");
    t.textContent = new Date().toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
    t.hidden = false;
  }
  if (d.photo) {
    const img = $<HTMLImageElement>("photo");
    img.src = d.photo;
    img.hidden = false;
    $("hero").classList.add("photo");
  }

  let leaving = false;
  const closeNow = (() => {
    let done = false;
    return () => {
      if (!done) {
        done = true;
        void invoke("close_popup");
      }
    };
  })();

  function dismiss() {
    if (leaving) return;
    leaving = true;
    if (d.style === "glide") return closeNow(); // Rust slides the window out
    card.classList.add("leaving");
    if (d.style === "funky") confetti(26);
    card.addEventListener("animationend", (e) => e.target === card && closeNow());
    window.setTimeout(closeNow, 600); // safety net
  }

  if (d.show_timer && d.dismiss_secs > 0) {
    $("timer").hidden = false;
    $("bar").style.animationDuration = `${d.dismiss_secs}s`;
  }
  if (d.dismiss_secs > 0) window.setTimeout(dismiss, d.dismiss_secs * 1000);

  document.addEventListener("click", dismiss);
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" || e.key === "Enter" || e.key === " ") dismiss();
  });

  if (d.style === "funky") window.setTimeout(() => confetti(44), 280);
  requestAnimationFrame(() => requestAnimationFrame(() => void invoke("popup_ready")));
}

init();
