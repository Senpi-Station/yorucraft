<script lang="ts">
  let maxMemory = $state('4');
  let minMemory = $state('2');
  let javaPath = $state('');
  let username = $state('Steve');
  let resolutionW = $state('1920');
  let resolutionH = $state('1080');
  let fullscreen = $state(false);
  let closeOnLaunch = $state(false);
  let jvmArgs = $state('-XX:+UseG1GC');
  let downloadDir = $state('~/.local/share/yorucraft/assets');

  const memoryPresets = [
    { label: '2 GB', value: '2' },
    { label: '4 GB', value: '4' },
    { label: '6 GB', value: '6' },
    { label: '8 GB', value: '8' },
    { label: '16 GB', value: '16' },
  ];

  function saveSettings() {
    // In a real app, this would call a Tauri command
    console.log('Saving settings...');
  }
</script>

<div class="settings-page">
  <div class="page-header">
    <h1 class="page-title">Settings</h1>
  </div>

  <!-- Profile -->
  <section class="settings-section">
    <h2 class="section-title">Profile</h2>
    <div class="settings-card glass-card">
      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">Username</span>
          <span class="setting-desc">Display name for offline mode</span>
        </div>
        <input class="input setting-input" type="text" bind:value={username} />
      </div>
    </div>
  </section>

  <!-- Java -->
  <section class="settings-section">
    <h2 class="section-title">Java</h2>
    <div class="settings-card glass-card">
      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">Java Path</span>
          <span class="setting-desc">Leave empty for auto-detection</span>
        </div>
        <input class="input setting-input-wide" type="text" placeholder="Auto-detect" bind:value={javaPath} />
      </div>

      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">Memory Allocation</span>
          <span class="setting-desc">Maximum heap memory for the JVM</span>
        </div>
        <div class="memory-presets">
          {#each memoryPresets as preset}
            <button
              class="preset-btn"
              class:active={maxMemory === preset.value}
              onclick={() => maxMemory = preset.value}
            >{preset.label}</button>
          {/each}
        </div>
      </div>

      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">JVM Arguments</span>
          <span class="setting-desc">Additional JVM flags</span>
        </div>
        <input class="input setting-input-wide" type="text" bind:value={jvmArgs} />
      </div>
    </div>
  </section>

  <!-- Game -->
  <section class="settings-section">
    <h2 class="section-title">Game</h2>
    <div class="settings-card glass-card">
      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">Resolution</span>
          <span class="setting-desc">Game window size</span>
        </div>
        <div class="resolution-inputs">
          <input class="input resolution-input" type="number" bind:value={resolutionW} />
          <span class="resolution-x">x</span>
          <input class="input resolution-input" type="number" bind:value={resolutionH} />
        </div>
      </div>

      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">Fullscreen</span>
          <span class="setting-desc">Launch game in fullscreen mode</span>
        </div>
        <label class="toggle">
          <input type="checkbox" bind:checked={fullscreen} />
          <span class="toggle-track"></span>
        </label>
      </div>

      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">Close launcher on game start</span>
          <span class="setting-desc">Hide launcher window when game launches</span>
        </div>
        <label class="toggle">
          <input type="checkbox" bind:checked={closeOnLaunch} />
          <span class="toggle-track"></span>
        </label>
      </div>
    </div>
  </section>

  <!-- Storage -->
  <section class="settings-section">
    <h2 class="section-title">Storage</h2>
    <div class="settings-card glass-card">
      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">Download Directory</span>
          <span class="setting-desc">Where game assets are cached</span>
        </div>
        <input class="input setting-input-wide" type="text" bind:value={downloadDir} />
      </div>

      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">Clear Asset Cache</span>
          <span class="setting-desc">Remove all downloaded game assets</span>
        </div>
        <button class="btn-danger" onclick={() => console.log('Clear cache')}>Clear</button>
      </div>
    </div>
  </section>

  <div class="settings-footer">
    <button class="btn-primary" onclick={saveSettings}>Save Settings</button>
  </div>
</div>

<style>
  .settings-page { max-width: 700px; margin: 0 auto; padding-bottom: 40px; }
  .page-header { margin-bottom: 28px; }
  .page-title { font-size: 28px; font-weight: 800; }

  .settings-section { margin-bottom: 28px; }
  .section-title { font-size: 16px; font-weight: 700; margin-bottom: 12px; }

  .settings-card { padding: 0; overflow: hidden; }

  .setting-row {
    display: flex; align-items: center; justify-content: space-between; gap: 20px;
    padding: 16px 20px;
    border-bottom: 1px solid var(--border);
  }
  .setting-row:last-child { border-bottom: none; }

  .setting-info { flex: 1; min-width: 0; }
  .setting-label { display: block; font-size: 14px; font-weight: 500; margin-bottom: 2px; }
  .setting-desc { display: block; font-size: 12px; color: var(--text-muted); }

  .setting-input { max-width: 200px; }
  .setting-input-wide { max-width: 320px; }

  .memory-presets { display: flex; gap: 6px; }
  .preset-btn {
    padding: 6px 14px; border-radius: var(--radius); font-size: 13px;
    background: var(--bg-elevated); color: var(--text-secondary); font-weight: 500;
  }
  .preset-btn.active { background: var(--accent); color: #000; }
  .preset-btn:hover:not(.active) { background: var(--bg-hover); }

  .resolution-inputs { display: flex; align-items: center; gap: 8px; }
  .resolution-input { width: 90px; text-align: center; }
  .resolution-x { color: var(--text-muted); font-size: 14px; }

  .toggle { position: relative; display: inline-block; width: 40px; height: 22px; cursor: pointer; flex-shrink: 0; }
  .toggle input { opacity: 0; width: 0; height: 0; }
  .toggle-track {
    position: absolute; inset: 0; background: var(--bg-elevated); border-radius: 11px; transition: background 200ms ease;
  }
  .toggle-track::after {
    content: ''; position: absolute; width: 18px; height: 18px; border-radius: 50%;
    background: var(--text-muted); top: 2px; left: 2px; transition: all 200ms ease;
  }
  .toggle input:checked + .toggle-track { background: var(--accent); }
  .toggle input:checked + .toggle-track::after { transform: translateX(18px); background: #000; }

  .settings-footer { margin-top: 8px; display: flex; justify-content: flex-end; }
</style>
