import { writable } from 'svelte/store';

export const currentVersion = writable('1.21.4');
export const playerName = writable('Player');
export const isLaunching = writable(false);
