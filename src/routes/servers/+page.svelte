<script lang="ts">
  interface ServerBookmark {
    name: string;
    address: string;
    ping: number | null;
    online: boolean;
    players: { max: number; online: number } | null;
  }

  let servers: ServerBookmark[] = $state([
    { name: 'Hypixel', address: 'mc.hypixel.net', ping: 42, online: true, players: { max: 200000, online: 45231 } },
    { name: 'Mineplex', address: 'us.mineplex.com', ping: 78, online: true, players: { max: 50000, online: 3421 } },
    { name: '2b2t', address: '2b2t.org', ping: 156, online: true, players: { max: 1000, online: 892 } },
  ]);

  let newName = $state('');
  let newAddress = $state('');
  let showAdd = $state(false);

  function handleAdd() {
    if (!newName.trim() || !newAddress.trim()) return;
    servers = [...servers, { name: newName.trim(), address: newAddress.trim(), ping: null, online: false, players: null }];
    newName = '';
    newAddress = '';
    showAdd = false;
  }

  function handleRemove(address: string) {
    servers = servers.filter(s => s.address !== address);
  }

  function formatPlayers(players: { max: number; online: number } | null): string {
    if (!players) return '?';
    return `${players.online.toLocaleString()} / ${players.max.toLocaleString()}`;
  }

  function formatPing(ping: number | null): string {
    if (ping === null) return '?ms';
    return `${ping}ms`;
  }

  function pingColor(ping: number | null): string {
    if (ping === null) return 'var(--text-muted)';
    if (ping < 60) return 'var(--success)';
    if (ping < 120) return 'var(--warning)';
    return 'var(--error)';
  }
</script>

<div class="servers-page">
  <div class="page-header">
    <div>
      <h1 class="page-title">Servers</h1>
      <p class="page-subtitle">{servers.length} bookmarked</p>
    </div>
    <button class="btn-primary" onclick={() => showAdd = true}>+ Add Server</button>
  </div>

  <div class="server-list">
    {#each servers as server (server.address)}
      <div class="server-card glass-card">
        <div class="server-status">
          <span class="status-dot" style="background: {server.online ? 'var(--success)' : 'var(--error)'}"></span>
        </div>
        <div class="server-info">
          <h3 class="server-name">{server.name}</h3>
          <span class="server-address">{server.address}</span>
        </div>
        <div class="server-stats">
          <div class="server-stat">
            <span class="stat-label">Players</span>
            <span class="stat-value">{formatPlayers(server.players)}</span>
          </div>
          <div class="server-stat">
            <span class="stat-label">Ping</span>
            <span class="stat-value" style="color: {pingColor(server.ping)}">{formatPing(server.ping)}</span>
          </div>
        </div>
        <button class="btn-ghost" onclick={() => handleRemove(server.address)} aria-label="Remove server">✕</button>
      </div>
    {:else}
      <div class="empty-state">
        <span class="empty-icon">◎</span>
        <h3>No Servers</h3>
        <p>Add your favorite servers for quick access</p>
      </div>
    {/each}
  </div>
</div>

{#if showAdd}
  <div class="modal-overlay" onclick={() => showAdd = false} role="dialog" aria-modal="true">
    <div class="modal" onclick={(e) => e.stopPropagation()}>
      <h2 class="modal-title">Add Server</h2>
      <div class="form-group">
        <label class="form-label" for="server-name">Name</label>
        <input id="server-name" class="input" type="text" placeholder="My Server" bind:value={newName} />
      </div>
      <div class="form-group">
        <label class="form-label" for="server-addr">Address</label>
        <input id="server-addr" class="input" type="text" placeholder="play.example.com" bind:value={newAddress} />
      </div>
      <div class="modal-actions">
        <button class="btn-ghost" onclick={() => showAdd = false}>Cancel</button>
        <button class="btn-primary" onclick={handleAdd} disabled={!newName.trim() || !newAddress.trim()}>Add</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .servers-page { max-width: 700px; margin: 0 auto; }
  .page-header { display: flex; justify-content: space-between; align-items: flex-start; margin-bottom: 24px; }
  .page-title { font-size: 28px; font-weight: 800; margin-bottom: 4px; }
  .page-subtitle { font-size: 13px; color: var(--text-muted); }

  .server-list { display: flex; flex-direction: column; gap: 8px; }

  .server-card {
    display: flex; align-items: center; gap: 16px;
    padding: 16px 20px; transition: background 150ms ease;
  }
  .server-card:hover { background: var(--bg-tertiary); }

  .server-status { flex-shrink: 0; }
  .status-dot { width: 10px; height: 10px; border-radius: 50%; display: block; }

  .server-info { flex: 1; min-width: 0; }
  .server-name { font-size: 15px; font-weight: 600; margin-bottom: 2px; }
  .server-address { font-size: 12px; color: var(--text-muted); font-family: monospace; }

  .server-stats { display: flex; gap: 24px; flex-shrink: 0; }
  .server-stat { text-align: right; }
  .stat-label { display: block; font-size: 10px; color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.05em; margin-bottom: 2px; }
  .stat-value { font-size: 13px; font-weight: 500; }

  .empty-state { text-align: center; padding: 60px 20px; color: var(--text-secondary); }
  .empty-icon { font-size: 36px; display: block; margin-bottom: 12px; opacity: 0.3; }
  .empty-state h3 { font-size: 16px; margin-bottom: 6px; color: var(--text-primary); }

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
  .modal-actions { display: flex; justify-content: flex-end; gap: 10px; margin-top: 24px; }
  .form-group { margin-bottom: 16px; }
  .form-label { display: block; font-size: 13px; font-weight: 500; color: var(--text-secondary); margin-bottom: 6px; }
</style>
