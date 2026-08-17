<script lang="ts">
  import { mods } from '$lib/stores';
  import type { ModInfo } from '$lib/stores';

  let modList: ModInfo[] = $state([]);
  let searchQuery = $state('');
  let activeTab: 'installed' | 'available' = $state('installed');
  let dragOver = $state(false);

  mods.subscribe(list => modList = list);

  // Demo mods for display
  const demoMods: ModInfo[] = [
    { name: 'Fabric API', filename: 'fabric-api-0.100.0.jar', version: '0.100.0', description: 'Core API for Fabric mods', enabled: true },
    { name: 'OptiFine', filename: 'OptiFine_1.21.4_HD_U_I6.jar', version: 'HD U I6', description: 'Performance and rendering improvements', enabled: true },
    { name: 'Sodium', filename: 'sodium-0.6.0.jar', version: '0.6.0', description: 'Modern rendering engine for Fabric', enabled: true },
    { name: 'Iris Shaders', filename: 'iris-1.8.0.jar', version: '1.8.0', description: 'Shader mod for Fabric', enabled: false },
    { name: 'ModMenu', filename: 'modmenu-12.0.0.jar', version: '12.0.0', description: 'Adds a mod list screen', enabled: true },
  ];

  if (modList.length === 0) {
    mods.set(demoMods);
  }

  let displayed = $derived(
    modList.filter(m => m.name.toLowerCase().includes(searchQuery.toLowerCase()))
  );

  function toggleMod(filename: string) {
    mods.update(list =>
      list.map(m => m.filename === filename ? { ...m, enabled: !m.enabled } : m)
    );
  }

  function removeMod(filename: string) {
    mods.update(list => list.filter(m => m.filename !== filename));
  }

  function handleDrop(e: DragEvent) {
    e.preventDefault();
    dragOver = false;
    const files = e.dataTransfer?.files;
    if (!files) return;
    for (const file of files) {
      if (file.name.endsWith('.jar')) {
        const newMod: ModInfo = {
          name: file.name.replace('.jar', '').replace(/[-_]\d+\.\d+\.\d+.*$/, ''),
          filename: file.name,
          version: 'unknown',
          description: 'Drag & drop mod',
          enabled: true,
        };
        mods.update(list => [...list, newMod]);
      }
    }
  }
</script>

<div
  class="mods-page"
  class:drag-over={dragOver}
  ondragover={(e) => { e.preventDefault(); dragOver = true; }}
  ondragleave={() => dragOver = false}
  ondrop={handleDrop}
>
  <div class="page-header">
    <div>
      <h1 class="page-title">Mods</h1>
      <p class="page-subtitle">{modList.length} mod{modList.length !== 1 ? 's' : ''}</p>
    </div>
    <label class="btn-primary upload-btn">
      + Add Mod
      <input type="file" accept=".jar" multiple hidden onchange={() => {}} />
    </label>
  </div>

  <div class="toolbar">
    <input class="input search-input" type="text" placeholder="Search mods..." bind:value={searchQuery} />
    <div class="filter-tabs">
      <button class="filter-tab" class:active={activeTab === 'installed'} onclick={() => activeTab = 'installed'}>
        Installed
      </button>
      <button class="filter-tab" class:active={activeTab === 'available'} onclick={() => activeTab = 'available'}>
        Browse
      </button>
    </div>
  </div>

  {#if activeTab === 'installed'}
    <div class="mod-list">
      {#each displayed as mod (mod.filename)}
        <div class="mod-item" class:disabled={!mod.enabled}>
          <div class="mod-info">
            <div class="mod-header">
              <span class="mod-name">{mod.name}</span>
              <span class="mod-version">{mod.version}</span>
            </div>
            <p class="mod-desc">{mod.description}</p>
            <span class="mod-filename">{mod.filename}</span>
          </div>
          <div class="mod-actions">
            <label class="toggle" aria-label="Toggle mod">
              <input type="checkbox" checked={mod.enabled} onchange={() => toggleMod(mod.filename)} />
              <span class="toggle-track"></span>
            </label>
            <button class="btn-ghost" onclick={() => removeMod(mod.filename)} aria-label="Remove mod">✕</button>
          </div>
        </div>
      {:else}
        <div class="empty-state">
          <p>No mods found. Drop .jar files here or click "Add Mod".</p>
        </div>
      {/each}
    </div>
  {:else}
    <div class="empty-state">
      <span class="empty-icon">◈</span>
      <h3>Browse Mods</h3>
      <p>Modrinth and CurseForge browsing coming in Phase 6</p>
    </div>
  {/if}

  {#if dragOver}
    <div class="drop-overlay">
      <div class="drop-content">
        <span class="drop-icon">↓</span>
        <p>Drop .jar files to install</p>
      </div>
    </div>
  {/if}
</div>

<style>
  .mods-page { max-width: 700px; margin: 0 auto; position: relative; }
  .page-header { display: flex; justify-content: space-between; align-items: flex-start; margin-bottom: 24px; }
  .page-title { font-size: 28px; font-weight: 800; margin-bottom: 4px; }
  .page-subtitle { font-size: 13px; color: var(--text-muted); }

  .upload-btn { cursor: pointer; }

  .toolbar { display: flex; gap: 12px; align-items: center; margin-bottom: 20px; flex-wrap: wrap; }
  .search-input { max-width: 280px; }

  .filter-tabs { display: flex; gap: 4px; background: var(--bg-secondary); border-radius: var(--radius); padding: 3px; }
  .filter-tab {
    padding: 6px 14px; border-radius: 6px; font-size: 13px; font-weight: 500;
    background: transparent; color: var(--text-secondary);
  }
  .filter-tab.active { background: var(--bg-elevated); color: var(--text-primary); }

  .mod-list { display: flex; flex-direction: column; gap: 6px; }

  .mod-item {
    display: flex; align-items: center; justify-content: space-between; gap: 16px;
    padding: 16px 18px; border-radius: var(--radius);
    background: var(--bg-secondary); transition: all 150ms ease;
  }
  .mod-item:hover { background: var(--bg-tertiary); }
  .mod-item.disabled { opacity: 0.5; }

  .mod-info { min-width: 0; flex: 1; }
  .mod-header { display: flex; align-items: center; gap: 10px; margin-bottom: 4px; }
  .mod-name { font-size: 14px; font-weight: 600; }
  .mod-version { font-size: 11px; color: var(--text-muted); font-family: monospace; }
  .mod-desc { font-size: 13px; color: var(--text-secondary); margin-bottom: 4px; }
  .mod-filename { font-size: 11px; color: var(--text-muted); font-family: monospace; }

  .mod-actions { display: flex; align-items: center; gap: 8px; flex-shrink: 0; }

  .toggle { position: relative; display: inline-block; width: 36px; height: 20px; cursor: pointer; }
  .toggle input { opacity: 0; width: 0; height: 0; }
  .toggle-track {
    position: absolute; inset: 0; background: var(--bg-elevated); border-radius: 10px; transition: background 200ms ease;
  }
  .toggle-track::after {
    content: ''; position: absolute; width: 16px; height: 16px; border-radius: 50%;
    background: var(--text-muted); top: 2px; left: 2px; transition: all 200ms ease;
  }
  .toggle input:checked + .toggle-track { background: var(--accent); }
  .toggle input:checked + .toggle-track::after { transform: translateX(16px); background: #000; }

  .empty-state { text-align: center; padding: 60px 20px; color: var(--text-secondary); }
  .empty-icon { font-size: 36px; display: block; margin-bottom: 12px; opacity: 0.3; }
  .empty-state h3 { font-size: 16px; margin-bottom: 6px; color: var(--text-primary); }
  .empty-state p { font-size: 13px; }

  .drop-overlay {
    position: absolute; inset: 0; background: rgba(245, 158, 11, 0.1);
    border: 2px dashed var(--accent); border-radius: var(--radius-xl);
    display: flex; align-items: center; justify-content: center; z-index: 10;
  }
  .drop-content { text-align: center; color: var(--accent); }
  .drop-icon { font-size: 32px; display: block; margin-bottom: 8px; }
  .drag-over { border-color: var(--accent); }
</style>
