import { StartThemingTool } from '../../../../shared/styles';
import { configureStore } from '@reduxjs/toolkit';
import { invoke } from '@tauri-apps/api/core';
import { InvokeHandler, LauncherHistory, Settings, UIColors } from 'seelen-core';

import { Actions, RootSlice } from './app';

export const store = configureStore({
  reducer: RootSlice.reducer,
});

async function initUIColors() {
  function loadColors(colors: UIColors) {
    UIColors.setAssCssVariables(colors);
    store.dispatch(Actions.setColors(colors));
  }
  loadColors(await UIColors.getAsync());
  await UIColors.onChange(loadColors);
}

export async function initStore() {
  const dispatch = store.dispatch;
  const settings = await Settings.getAsync();

  dispatch(Actions.setSettings(settings.launcher));
  dispatch(Actions.setApps(await invoke(InvokeHandler.GetLauncherApps)));
  dispatch(Actions.setHistory(await LauncherHistory.getAsync()));

  Settings.onChange((settings) => dispatch(Actions.setSettings(settings.launcher)));
  LauncherHistory.onChange((history) => dispatch(Actions.setHistory(history)));

  await initUIColors();
  await StartThemingTool();
}