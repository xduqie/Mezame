import { invoke } from '@tauri-apps/api/core';
import { load } from '@tauri-apps/plugin-store';

interface PopupConfig {
    message: string;
    delay_secs: number;
    auto_dismiss_secs: number;
    idle_threshold_secs: number;
}

async function loadConfig() {
    const store = await load('settings.json', { autoSave: false });
    const saved = await store.get<PopupConfig>('popup_config');
    if (saved) {
        (document.getElementById('message') as HTMLTextAreaElement).value = saved.message;
        (document.getElementById('delay') as HTMLInputElement).value = saved.delay_secs.toString();
        (document.getElementById('dismiss') as HTMLInputElement).value = saved.auto_dismiss_secs.toString();
        (document.getElementById('threshold') as HTMLInputElement).value = saved.idle_threshold_secs.toString();
        await invoke('set_popup_config', { config: saved });
    }
}

async function saveConfig() {
    const config: PopupConfig = {
        message: (document.getElementById('message') as HTMLTextAreaElement).value,
        delay_secs: parseInt((document.getElementById('delay') as HTMLInputElement).value, 10) || 2,
        auto_dismiss_secs: parseInt((document.getElementById('dismiss') as HTMLInputElement).value, 10) || 5,
        idle_threshold_secs: parseInt((document.getElementById('threshold') as HTMLInputElement).value, 10) || 5,
    };

    const store = await load('settings.json', { autoSave: false });
    await store.set('popup_config', config);
    await store.save();
    await invoke('set_popup_config', { config });
    alert('Settings saved!');
}

window.addEventListener('DOMContentLoaded', () => {
    loadConfig();
    document.getElementById('save')!.addEventListener('click', saveConfig);
    document.getElementById('test-popup')!.addEventListener('click', () => {
        invoke('trigger_popup');
    });
});