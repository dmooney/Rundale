/**
 * Diorama scene-state IPC.
 *
 * Thin wrapper over the shared transport seam. Web mode reads
 * `/api/scene-state`; Tauri mode invokes `get_scene_state`.
 */

import type { SceneState } from '../types';
import { command } from './transport';

export const getSceneState = () =>
	command<SceneState | null>('get_scene_state');
