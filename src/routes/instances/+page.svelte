<script lang="ts">
  import { instances } from '$lib/stores';
  import type { Instance } from '$lib/stores';

  let instanceList: Instance[] = $state([]);
  let showCreateModal = $state(false);
  let newName = $state('');
  let newVersion = $state('1.21.4');
  let newLoader = $state('vanilla');
  let selectedInstance: Instance | null = $state(null);
  let showDeleteConfirm = $state(false);

  instances.subscribe(list => instanceList = list);

  const loaderOptions = ['vanilla', 'fabric', 'forge', 'quilt'];

  function formatDate(iso: string | null): string {
    if (!iso) return 'Never';
    const d = new Date(iso);
    return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' });
  }

  function formatPlayTime(seconds: number): string {
    const h = Math.floor(seconds / 3600);
    const m = Math.floor((seconds % 3600) / 60);
    if (h === 0) return `${m}m`;
    return `${h}h ${m}m`;
  }

  function handleCreate() {
    if (!newName.trim()) return;
    const newInstance: Instance = {
      id: crypto.randomUUID(),
      name: newName.trim(),
      mc_version: newVersion,
      loader: newLoader,
      loader_version: null,
      game_dir: `~/.local/share/yorucraft/instances/${crypto.randomUUID()}`,
      created: new Date().toISOString(),
      last_played: null,
      play_time_seconds: 0,
      icon: null,
      description: null,
    };
    instances.update(list => [...list, newInstance]);
    showCreateModal = false;
    newName = '';
  }

  function handleDelete(id: string) {
    instances.update(list => list.filter(i => i.id !== id));
    selectedInstance = null;
    showDeleteConfirm = false;
  }

  function handleSelect(inst: Instance) {
    selectedInstance = selectedInstance?.id === inst.id ? null : inst;
  }
</script>

<div class="instances-page">
  <div class="page-header">
    <div>
      <h1 class="page-title">Instances</h1>
      <p class="page-subtitle">{instanceList.length} instance{instanceList.length !== 1 ? 's' : ''}</p>
    </div>
    <button class="btn-primary" onclick={() => showCreateModal = true}>+ New Instance</button>
  </div>

  {#if instanceList.length === 0}
    <div class="empty-state">
      <span class="empty-icon">⊞</span>
      <h3>No Instances Yet</h3>
      <p>Create your first instance to get started</p>
      <button class="btn-primary" onclick={() => showCreateModal = true}>Create Instance</button>
    </div>
  {:else}
    <div class="instance-grid">
      {#each instanceList as inst (inst.id)}
        <button
          class="instance-card glass-card glass-card-hover"
          class:selected={selectedInstance?.id === inst.id}
          onclick={() => handleSelect(inst)}
        >
          <div class="instance-header">
            <div class="instance-icon" style="background: hsl({inst.name.charCodeAt(0) * 37 % 360}, 50%, 25%)">
              {inst.name[0]}
            </div>
            <div class="instance-meta">
              <h3 class="instance-name">{inst.name}</h3>
              <span class="instance-version">{inst.mc_version} · {inst.loader}</span>
            </div>
          </div>

          <div class="instance-details">
            <div class="detail-item">
              <span class="detail-label">Last Played</span>
              <span class="detail-value">{formatDate(inst.last_played)}</span>
            </div>
            <div class="detail-item">
              <span class="detail-label">Play Time</span>
              <span class="detail-value">{formatPlayTime(inst.play_time_seconds)}</span>
            </div>
          </div>

          {#if selectedInstance?.id === inst.id}
            <div class="instance-actions">
              <button class="btn-primary" onclick={(e) => e.stopPropagation()}>▶ Play</button>
              <button class="btn-secondary" onclick={(e) => e.stopPropagation()}>Clone</button>
              <button class="btn-danger" onclick={(e) => { e.stopPropagation(); showDeleteConfirm = true; }}>Delete</button>
            </div>
          {/if}
        </button>
      {/each}
    </div>
  {/if}
</div>

<!-- Create Modal -->
{#if showCreateModal}
  <div class="modal-overlay" onclick={() => showCreateModal = false} role="dialog" aria-modal="true">
    <div class="modal" onclick={(e) => e.stopPropagation()}>
      <h2 class="modal-title">Create Instance</h2>

      <div class="form-group">
        <label class="form-label" for="inst-name">Name</label>
        <input id="inst-name" class="input" type="text" placeholder="My Instance" bind:value={newName} />
      </div>

      <div class="form-group">
        <label class="form-label" for="inst-version">Version</label>
        <select id="inst-version" class="input" bind:value={newVersion}>
          {#each ['1.21.4', '1.21.3', '1.21.2', '1.21.1', '1.21', '1.20.4', '1.20.1'] as v}
            <option value={v}>{v}</option>
          {/each}
        </select>
      </div>

      <div class="form-group">
        <label class="form-label" for="inst-loader">Loader</label>
        <select id="inst-loader" class="input" bind:value={newLoader}>
          {#each loaderOptions as l}
            <option value={l}>{l.charAt(0).toUpperCase() + l.slice(1)}</option>
          {/each}
        </select>
      </div>

      <div class="modal-actions">
        <button class="btn-ghost" onclick={() => showCreateModal = false}>Cancel</button>
        <button class="btn-primary" onclick={handleCreate} disabled={!newName.trim()}>Create</button>
      </div>
    </div>
  </div>
{/if}

<!-- Delete Confirm -->
{#if showDeleteConfirm && selectedInstance}
  <div class="modal-overlay" onclick={() => showDeleteConfirm = false} role="dialog" aria-modal="true">
    <div class="modal" onclick={(e) => e.stopPropagation()}>
      <h2 class="modal-title">Delete Instance</h2>
      <p class="modal-text">Are you sure you want to delete "{selectedInstance.name}"? This cannot be undone.</p>
      <div class="modal-actions">
        <button class="btn-ghost" onclick={() => showDeleteConfirm = false}>Cancel</button>
        <button class="btn-danger" onclick={() => handleDelete(selectedInstance!.id)}>Delete</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .instances-page { max-width: 900px; margin: 0 auto; }

  .page-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    margin-bottom: 28px;
  }
  .page-title { font-size: 28px; font-weight: 800; margin-bottom: 4px; }
  .page-subtitle { font-size: 13px; color: var(--text-muted); }

  .empty-state {
    text-align: center;
    padding: 80px 20px;
    color: var(--text-secondary);
  }
  .empty-icon { font-size: 48px; display: block; margin-bottom: 16px; opacity: 0.3; }
  .empty-state h3 { font-size: 18px; margin-bottom: 6px; color: var(--text-primary); }
  .empty-state p { font-size: 14px; margin-bottom: 20px; }

  .instance-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(340px, 1fr));
    gap: 14px;
  }

  .instance-card {
    text-align: left;
    cursor: pointer;
    transition: all 150ms ease;
  }
  .instance-card.selected {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent), var(--shadow-lg);
  }

  .instance-header { display: flex; gap: 14px; align-items: center; margin-bottom: 14px; }

  .instance-icon {
    width: 44px; height: 44px; border-radius: var(--radius);
    display: flex; align-items: center; justify-content: center;
    font-size: 20px; font-weight: 700; color: var(--text-primary); flex-shrink: 0;
  }

  .instance-meta { min-width: 0; }
  .instance-name { font-size: 16px; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .instance-version { font-size: 12px; color: var(--text-muted); text-transform: capitalize; }

  .instance-details { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
  .detail-item { display: flex; flex-direction: column; }
  .detail-label { font-size: 11px; color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.04em; margin-bottom: 2px; }
  .detail-value { font-size: 13px; color: var(--text-secondary); }

  .instance-actions { display: flex; gap: 8px; margin-top: 14px; padding-top: 14px; border-top: 1px solid var(--border); }

  .modal-overlay {
    position: fixed; inset: 0; background: rgba(0, 0, 0, 0.6); z-index: 100;
    display: flex; align-items: center; justify-content: center;
    backdrop-filter: blur(4px);
  }
  .modal {
    background: var(--bg-secondary); border-radius: var(--radius-xl); padding: 28px;
    width: 100%; max-width: 420px; box-shadow: var(--shadow-xl);
  }
  .modal-title { font-size: 20px; font-weight: 700; margin-bottom: 20px; }
  .modal-text { font-size: 14px; color: var(--text-secondary); line-height: 1.6; margin-bottom: 20px; }
  .modal-actions { display: flex; justify-content: flex-end; gap: 10px; margin-top: 24px; }

  .form-group { margin-bottom: 16px; }
  .form-label { display: block; font-size: 13px; font-weight: 500; color: var(--text-secondary); margin-bottom: 6px; }
  select.input { cursor: pointer; -webkit-appearance: none; appearance: none; }
</style>
