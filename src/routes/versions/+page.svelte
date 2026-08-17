<script lang="ts">
  let searchQuery = $state('');
  let filterType: 'all' | 'release' | 'snapshot' = $state('all');
  let installedVersions = $state<string[]>(['1.21.4', '1.20.1']);
  let installing = $state<string | null>(null);

  const versions = [
    { id: '1.21.4', type: 'release' as const, date: 'Dec 3, 2024' },
    { id: '1.21.3', type: 'release' as const, date: 'Oct 23, 2024' },
    { id: '1.21.2', type: 'release' as const, date: 'Oct 22, 2024' },
    { id: '1.21.2-pre4', type: 'snapshot' as const, date: 'Oct 17, 2024' },
    { id: '1.21.2-pre3', type: 'snapshot' as const, date: 'Oct 15, 2024' },
    { id: '1.21.2-pre2', type: 'snapshot' as const, date: 'Oct 10, 2024' },
    { id: '1.21.2-pre1', type: 'snapshot' as const, date: 'Oct 3, 2024' },
    { id: '1.21.1', type: 'release' as const, date: 'Aug 8, 2024' },
    { id: '1.21', type: 'release' as const, date: 'Jun 13, 2024' },
    { id: '1.20.6', type: 'release' as const, date: 'Apr 25, 2024' },
    { id: '1.20.4', type: 'release' as const, date: 'Dec 7, 2023' },
    { id: '1.20.1', type: 'release' as const, date: 'Jun 12, 2023' },
    { id: '1.19.4', type: 'release' as const, date: 'Mar 14, 2023' },
    { id: '1.19.2', type: 'release' as const, date: 'Aug 5, 2022' },
    { id: '1.18.2', type: 'release' as const, date: 'Feb 28, 2022' },
  ];

  let filtered = $derived(
    versions.filter(v => {
      if (!v.id.includes(searchQuery)) return false;
      if (filterType !== 'all' && v.type !== filterType) return false;
      return true;
    })
  );

  function isInstalled(id: string): boolean {
    return installedVersions.includes(id);
  }

  async function handleInstall(id: string) {
    installing = id;
    await new Promise(r => setTimeout(r, 1500));
    installedVersions = [...installedVersions, id];
    installing = null;
  }

  function handleDelete(id: string) {
    installedVersions = installedVersions.filter(v => v !== id);
  }
</script>

<div class="versions-page">
  <div class="page-header">
    <div>
      <h1 class="page-title">Versions</h1>
      <p class="page-subtitle">{installedVersions.length} installed</p>
    </div>
  </div>

  <div class="toolbar">
    <input
      class="input search-input"
      type="text"
      placeholder="Search versions..."
      bind:value={searchQuery}
    />
    <div class="filter-tabs">
      {#each [['all', 'All'], ['release', 'Releases'], ['snapshot', 'Snapshots']] as [value, label]}
        <button
          class="filter-tab"
          class:active={filterType === value}
          onclick={() => filterType = value as typeof filterType}
        >{label}</button>
      {/each}
    </div>
  </div>

  <div class="version-list">
    {#each filtered as v (v.id)}
      <div class="version-item" class:installed={isInstalled(v.id)}>
        <div class="version-info">
          <span class="version-id">{v.id}</span>
          <div class="version-meta">
            <span class="badge" class:badge-accent={v.type === 'release'} class:badge-error={v.type === 'snapshot'}>
              {v.type}
            </span>
            <span class="version-date">{v.date}</span>
          </div>
        </div>
        <div class="version-actions">
          {#if isInstalled(v.id)}
            <span class="badge badge-success">Installed</span>
            <button class="btn-ghost" onclick={() => handleDelete(v.id)}>Remove</button>
          {:else if installing === v.id}
            <span class="installing-text"><span class="spinner"></span> Installing</span>
          {:else}
            <button class="btn-secondary" onclick={() => handleInstall(v.id)}>Install</button>
          {/if}
        </div>
      </div>
    {/each}
  </div>
</div>

<style>
  .versions-page { max-width: 700px; margin: 0 auto; }

  .page-header { margin-bottom: 24px; }
  .page-title { font-size: 28px; font-weight: 800; margin-bottom: 4px; }
  .page-subtitle { font-size: 13px; color: var(--text-muted); }

  .toolbar { display: flex; gap: 12px; align-items: center; margin-bottom: 20px; flex-wrap: wrap; }
  .search-input { max-width: 280px; }

  .filter-tabs { display: flex; gap: 4px; background: var(--bg-secondary); border-radius: var(--radius); padding: 3px; }
  .filter-tab {
    padding: 6px 14px; border-radius: 6px; font-size: 13px; font-weight: 500;
    background: transparent; color: var(--text-secondary);
  }
  .filter-tab.active { background: var(--bg-elevated); color: var(--text-primary); }
  .filter-tab:hover { color: var(--text-primary); }

  .version-list { display: flex; flex-direction: column; gap: 4px; }

  .version-item {
    display: flex; align-items: center; justify-content: space-between;
    padding: 14px 18px; border-radius: var(--radius);
    background: var(--bg-secondary); transition: background 150ms ease;
  }
  .version-item:hover { background: var(--bg-tertiary); }
  .version-item.installed { border-left: 3px solid var(--success); }

  .version-info { display: flex; flex-direction: column; gap: 4px; }
  .version-id { font-size: 14px; font-weight: 600; font-family: 'JetBrains Mono', monospace; }
  .version-meta { display: flex; align-items: center; gap: 10px; }
  .version-date { font-size: 12px; color: var(--text-muted); }

  .version-actions { display: flex; align-items: center; gap: 10px; }

  .installing-text {
    display: flex; align-items: center; gap: 8px;
    font-size: 13px; color: var(--text-secondary);
  }
  .spinner {
    width: 14px; height: 14px;
    border: 2px solid var(--border);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
</style>
