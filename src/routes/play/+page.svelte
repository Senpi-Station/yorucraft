<script lang="ts">
  let logs = $state([
    { timestamp: '14:23:01', level: 'info', message: 'Launcher started' },
    { timestamp: '14:23:02', level: 'info', message: 'Fetching version manifest...' },
    { timestamp: '14:23:03', level: 'info', message: 'Version manifest cached (456 versions)' },
    { timestamp: '14:23:05', level: 'warn', message: 'Java not found in PATH, searching...' },
    { timestamp: '14:23:06', level: 'info', message: 'Found Java 21.0.2 at /usr/lib/jvm/java-21' },
    { timestamp: '14:23:10', level: 'info', message: 'Downloading client jar for 1.21.4...' },
    { timestamp: '14:23:15', level: 'info', message: 'Client jar verified (24.3 MB)' },
    { timestamp: '14:23:16', level: 'info', message: 'Downloading asset index...' },
    { timestamp: '14:23:18', level: 'info', message: 'Asset index downloaded (156 KB)' },
    { timestamp: '14:23:20', level: 'warn', message: 'Asset object f8c1e3b2... size mismatch, re-downloading' },
    { timestamp: '14:23:22', level: 'info', message: 'Asset downloaded successfully' },
    { timestamp: '14:23:25', level: 'info', message: 'Downloading 247 libraries...' },
    { timestamp: '14:23:40', level: 'error', message: 'Failed to download library org.lwjgl:lwjgl:3.3.1 - Connection timeout' },
    { timestamp: '14:23:41', level: 'info', message: 'Retrying download (attempt 2/3)...' },
    { timestamp: '14:23:45', level: 'info', message: 'Library downloaded on retry' },
    { timestamp: '14:23:50', level: 'info', message: 'All libraries downloaded successfully' },
    { timestamp: '14:23:51', level: 'info', message: 'Extracting natives...' },
    { timestamp: '14:23:52', level: 'info', message: 'Natives extracted (12 files)' },
    { timestamp: '14:23:53', level: 'info', message: 'Building classpath...' },
    { timestamp: '14:23:54', level: 'info', message: 'Launching Minecraft 1.21.4 with Fabric 0.15.0' },
    { timestamp: '14:23:54', level: 'info', message: 'JVM args: -Xmx4G -Xms2G -XX:+UseG1GC' },
  ]);

  let autoScroll = $state(true);
  let logContainer: HTMLDivElement;

  function levelColor(level: string): string {
    switch (level) {
      case 'error': return 'var(--error)';
      case 'warn': return 'var(--warning)';
      case 'info': return 'var(--text-muted)';
      default: return 'var(--text-secondary)';
    }
  }
</script>

<div class="logs-page">
  <div class="page-header">
    <h1 class="page-title">Game Logs</h1>
    <div class="header-actions">
      <button class="btn-ghost" onclick={() => logs = []}>Clear</button>
      <label class="toggle-label">
        <input type="checkbox" bind:checked={autoScroll} />
        <span>Auto-scroll</span>
      </label>
    </div>
  </div>

  <div class="log-container" bind:this={logContainer}>
    {#each logs as log}
      <div class="log-line">
        <span class="log-time">{log.timestamp}</span>
        <span class="log-level" style="color: {levelColor(log.level)}">[{log.level.toUpperCase()}]</span>
        <span class="log-message">{log.message}</span>
      </div>
    {:else}
      <div class="empty-logs">
        <p>No logs yet. Launch a game instance to see logs here.</p>
      </div>
    {/each}
  </div>
</div>

<style>
  .logs-page { max-width: 900px; margin: 0 auto; display: flex; flex-direction: column; height: calc(100vh - var(--titlebar-height) - 48px); }

  .page-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 16px; flex-shrink: 0; }
  .page-title { font-size: 28px; font-weight: 800; }
  .header-actions { display: flex; align-items: center; gap: 12px; }

  .toggle-label { display: flex; align-items: center; gap: 6px; font-size: 13px; color: var(--text-secondary); cursor: pointer; }
  .toggle-label input { accent-color: var(--accent); }

  .log-container {
    flex: 1; overflow-y: auto; background: var(--bg-secondary); border-radius: var(--radius-lg);
    padding: 16px; font-family: 'JetBrains Mono', 'Fira Code', monospace; font-size: 12px; line-height: 1.8;
  }

  .log-line { display: flex; gap: 10px; white-space: nowrap; }
  .log-time { color: var(--text-muted); flex-shrink: 0; }
  .log-level { font-weight: 600; flex-shrink: 0; min-width: 52px; }
  .log-message { color: var(--text-secondary); }

  .empty-logs { text-align: center; padding: 40px; color: var(--text-muted); font-family: inherit; font-size: 14px; }
</style>
