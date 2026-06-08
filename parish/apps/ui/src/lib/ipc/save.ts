/**
 * Save-file and branch lifecycle IPC (#1200 TD-054).
 */

import type { SaveFileInfo, SaveState } from '../types';
import { command } from './transport';

export const discoverSaveFiles = () =>
	command<SaveFileInfo[]>('discover_save_files');

export const saveGame = () => command<string>('save_game', {});

export const loadBranch = (filePath: string, branchId: number) =>
	command<void>('load_branch', { filePath, branchId });

export const createBranch = (name: string, parentBranchId: number) =>
	command<string>('create_branch', { name, parentBranchId });

export const newSaveFile = () => command<void>('new_save_file', {});

export const newGame = () => command<void>('new_game', {});

export const getSaveState = () => command<SaveState>('get_save_state');
