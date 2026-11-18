# Mezame (v0.3)

Shows your reminder (title, text, photo) when you come back to your PC, and on a schedule.

## Update your project
Copy over: `package.json`, `vite.config.ts`, `tsconfig.json`, `index.html`, `popup.html`, `src/*`,
`src-tauri/Cargo.toml` (adds `chrono`), `tauri.conf.json`, `src/*.rs`. Keep your own icons and identifier.

    npm install
    npm run tauri dev

## Popup styles
- Minimal: soft focus-in and rise; mirrored on dismiss.
- Glide: the window slides in from the screen edge and slides back out on click.
  Centered popups enter from the left and leave to the right.
- Funky: bouncy tilted card, bright colours, confetti on arrival and on dismiss.

## Schedule
Settings > Schedule: every N minutes, or at a set time on chosen days. If the PC is locked when one is due,
it shows right after unlock.

## Lock screen
Windows doesn't allow live windows over the lock screen or password box. "Put it on the lock screen" renders the
reminder into your lock-screen picture instead. Set Settings > Personalization > Lock screen to Picture (not Spotlight).
