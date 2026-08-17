import { writable } from 'svelte/store';
import type { Writable } from 'svelte/store';

export interface Instance {
  id: string;
  name: string;
  mc_version: string;
  loader: string;
  loader_version: string | null;
  game_dir: string;
  created: string;
  last_played: string | null;
  play_time_seconds: number;
  icon: string | null;
  description: string | null;
}

export interface InstanceProfile {
  java_path: string | null;
  jvm_args: string[];
  max_memory: string;
  min_memory: string;
  resolution_width: number | null;
  resolution_height: number | null;
  fullscreen: boolean;
  game_args: string[];
  environment_vars: Record<string, string>;
}

export interface ModInfo {
  name: string;
  filename: string;
  version: string;
  description: string;
  enabled: boolean;
}

export const currentInstance: Writable<Instance | null> = writable(null);
export const instances: Writable<Instance[]> = writable([]);
export const mods: Writable<ModInfo[]> = writable([]);
export const theme: Writable<'dark' | 'light'> = writable('dark');
export const sidebarCollapsed: Writable<boolean> = writable(false);
