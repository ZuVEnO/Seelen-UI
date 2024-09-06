import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export enum InvokeHandler {
  GetSystemColors = 'get_system_colors',
  GetSettings = 'state_get_settings',
  GetLauncherApps = 'launcher_get_apps',
}

export enum EventHandler {
  UIColors = 'colors-changed',
  Settings = 'settings-changed',
}

export function Obtainable<T>(invokeKey: InvokeHandler, eventKey: EventHandler) {
  return class {
    static async getAsync(): Promise<T> {
      return await invoke(invokeKey);
    }

    static async onChange(cb: (value: T) => void) {
      await listen<T>(eventKey, (event) => {
        cb(event.payload);
      });
    }
  };
}