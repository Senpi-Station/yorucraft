<script lang="ts">
  import { page } from '$app/stores';
  import { sidebarCollapsed } from '$lib/stores';

  const navItems = [
    { path: '/', label: 'Home', icon: '⌂' },
    { path: '/instances', label: 'Instances', icon: '⊞' },
    { path: '/versions', label: 'Versions', icon: '↻' },
    { path: '/mods', label: 'Mods', icon: '◈' },
    { path: '/servers', label: 'Servers', icon: '◎' },
    { path: '/settings', label: 'Settings', icon: '⚙' },
  ];

  let collapsed = $state(false);

  sidebarCollapsed.subscribe(v => collapsed = v);

  function toggleSidebar() {
    sidebarCollapsed.update(v => !v);
  }
</script>

<div class="app-layout">
  <!-- Titlebar drag region -->
  <div class="titlebar" data-tauri-drag-region>
    <span class="titlebar-text">YoruCraft</span>
  </div>

  <div class="app-body">
    <!-- Sidebar -->
    <nav class="sidebar" class:collapsed>
      <div class="sidebar-nav">
        {#each navItems as item}
          <a
            href={item.path}
            class="nav-item"
            class:active={$page.url.pathname === item.path}
            title={collapsed ? item.label : undefined}
          >
            <span class="nav-icon">{item.icon}</span>
            {#if !collapsed}
              <span class="nav-label">{item.label}</span>
            {/if}
          </a>
        {/each}
      </div>

      <div class="sidebar-footer">
        <button class="nav-item collapse-btn" onclick={toggleSidebar} aria-label="Toggle sidebar">
          <span class="nav-icon">{collapsed ? '›' : '‹'}</span>
        </button>
      </div>
    </nav>

    <!-- Main content -->
    <main class="main-content">
      <div class="scroll-area">
        <slot />
      </div>
    </main>
  </div>
</div>

<style>
  .app-layout {
    display: flex;
    flex-direction: column;
    height: 100vh;
    overflow: hidden;
  }

  .titlebar {
    height: var(--titlebar-height);
    background: var(--bg-secondary);
    display: flex;
    align-items: center;
    justify-content: center;
    -webkit-app-region: drag;
    flex-shrink: 0;
    border-bottom: 1px solid var(--border);
  }

  .titlebar-text {
    font-size: 12px;
    color: var(--text-muted);
    font-weight: 500;
    letter-spacing: 0.05em;
  }

  .app-body {
    display: flex;
    flex: 1;
    overflow: hidden;
  }

  .sidebar {
    width: var(--sidebar-width);
    background: var(--bg-secondary);
    display: flex;
    flex-direction: column;
    border-right: 1px solid var(--border);
    flex-shrink: 0;
    transition: width 200ms ease;
    overflow: hidden;
  }

  .sidebar.collapsed {
    width: 56px;
  }

  .sidebar-nav {
    flex: 1;
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    overflow-y: auto;
  }

  .sidebar-footer {
    padding: 8px;
    border-top: 1px solid var(--border);
  }

  .nav-item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    border-radius: var(--radius);
    color: var(--text-secondary);
    text-decoration: none;
    font-size: 14px;
    font-weight: 500;
    transition: all 150ms ease;
    white-space: nowrap;
  }

  .nav-item:hover {
    color: var(--text-primary);
    background: var(--bg-elevated);
  }

  .nav-item.active {
    color: var(--accent);
    background: var(--accent-dim);
  }

  .nav-icon {
    font-size: 18px;
    width: 24px;
    text-align: center;
    flex-shrink: 0;
  }

  .collapse-btn {
    cursor: pointer;
    width: 100%;
  }

  .main-content {
    flex: 1;
    overflow: hidden;
    position: relative;
  }

  .scroll-area {
    height: 100%;
    overflow-y: auto;
    padding: 24px;
  }
</style>
