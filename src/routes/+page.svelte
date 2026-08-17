<script lang="ts">
  import { instances } from '$lib/stores';

  let playState: 'idle' | 'installing' | 'running' | 'error' = $state('idle');
  let selectedVersion = $state('1.21.4');
  let username = $state('Steve');
  let instanceCount = $state(0);
  let totalPlayTime = $state(0);

  instances.subscribe(list => {
    instanceCount = list.length;
    totalPlayTime = list.reduce((sum, i) => sum + i.play_time_seconds, 0);
  });

  const versionOptions = ['1.21.4', '1.21.3', '1.21.2', '1.21.1', '1.21', '1.20.4', '1.20.1'];

  function formatPlayTime(seconds: number): string {
    const h = Math.floor(seconds / 3600);
    const m = Math.floor((seconds % 3600) / 60);
    if (h === 0) return `${m}m`;
    return `${h}h ${m}m`;
  }

  async function handlePlay() {
    if (playState === 'running') return;
    playState = 'installing';
    // Simulate installation
    await new Promise(r => setTimeout(r, 2000));
    playState = 'running';
  }

  const newsItems = [
    { title: 'Minecraft 1.21.4 Released', desc: 'The Garden Awakens update is here with new blocks and mobs.', date: 'Dec 2024', color: '#22c55e' },
    { title: 'Minecraft Live 2024 Recap', desc: 'All the announcements from this year\'s Minecraft Live event.', date: 'Sep 2024', color: '#3b82f6' },
    { title: 'Java 21 Now Required', desc: 'Starting from 1.21, Minecraft requires Java 21 or newer.', date: 'Jun 2024', color: '#f59e0b' },
  ];
</script>

<div class="home">
  <!-- Hero Section -->
  <section class="hero">
    <div class="hero-bg"></div>
    <div class="hero-content">
      <h1 class="hero-title">YoruCraft</h1>
      <p class="hero-subtitle">Minecraft Launcher</p>

      <div class="play-area">
        {#if playState === 'running'}
          <button class="play-btn running animate-pulse" disabled>
            <span class="play-icon">▶</span> Playing
          </button>
        {:else if playState === 'installing'}
          <button class="play-btn installing" disabled>
            <span class="spinner"></span> Installing...
          </button>
        {:else}
          <button class="play-btn idle" onclick={handlePlay}>
            <span class="play-icon">▶</span> Play
          </button>
        {/if}

        <select class="version-select" bind:value={selectedVersion} aria-label="Select Minecraft version">
          {#each versionOptions as v}
            <option value={v}>{v}</option>
          {/each}
        </select>

        <div class="user-badge">
          <span class="user-avatar">{username[0]}</span>
          <span class="user-name">{username}</span>
        </div>
      </div>
    </div>
  </section>

  <!-- Stats Row -->
  <section class="stats">
    <div class="stat-card glass-card">
      <span class="stat-icon">⊞</span>
      <div class="stat-info">
        <span class="stat-value">{instanceCount}</span>
        <span class="stat-label">Instances</span>
      </div>
    </div>
    <div class="stat-card glass-card">
      <span class="stat-icon">◈</span>
      <div class="stat-info">
        <span class="stat-value">0</span>
        <span class="stat-label">Mods</span>
      </div>
    </div>
    <div class="stat-card glass-card">
      <span class="stat-icon">◷</span>
      <div class="stat-info">
        <span class="stat-value">{formatPlayTime(totalPlayTime)}</span>
        <span class="stat-label">Play Time</span>
      </div>
    </div>
  </section>

  <!-- News -->
  <section class="news">
    <h2 class="section-title">News</h2>
    <div class="news-grid">
      {#each newsItems as item, i}
        <div class="news-card glass-card glass-card-hover" style="animation-delay: {i * 80}ms">
          <div class="news-accent" style="background: {item.color}"></div>
          <div class="news-body">
            <h3 class="news-title">{item.title}</h3>
            <p class="news-desc">{item.desc}</p>
            <span class="news-date">{item.date}</span>
          </div>
        </div>
      {/each}
    </div>
  </section>
</div>

<style>
  .home {
    max-width: 900px;
    margin: 0 auto;
    padding-bottom: 40px;
  }

  .hero {
    position: relative;
    border-radius: var(--radius-xl);
    overflow: hidden;
    margin-bottom: 24px;
    min-height: 280px;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .hero-bg {
    position: absolute;
    inset: 0;
    background: linear-gradient(135deg, var(--bg-tertiary) 0%, var(--bg-elevated) 50%, rgba(245, 158, 11, 0.08) 100%);
  }

  .hero-content {
    position: relative;
    text-align: center;
    z-index: 1;
    padding: 40px 24px;
  }

  .hero-title {
    font-size: 40px;
    font-weight: 800;
    letter-spacing: -0.02em;
    margin-bottom: 4px;
    background: linear-gradient(135deg, var(--text-primary) 60%, var(--accent));
    -webkit-background-clip: text;
    -webkit-text-fill-color: transparent;
    background-clip: text;
  }

  .hero-subtitle {
    font-size: 14px;
    color: var(--text-muted);
    margin-bottom: 32px;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }

  .play-area {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 16px;
  }

  .play-btn {
    width: 200px;
    height: 56px;
    border-radius: var(--radius-xl);
    font-size: 18px;
    font-weight: 700;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    letter-spacing: 0.02em;
  }

  .play-btn.idle {
    background: var(--accent);
    color: #000;
    animation: pulse 2.5s ease-in-out infinite;
  }

  .play-btn.idle:hover {
    background: var(--accent-hover);
    transform: scale(1.02);
  }

  .play-btn.running {
    background: var(--success);
    color: #fff;
    animation: none;
  }

  .play-btn.installing {
    background: var(--bg-elevated);
    color: var(--text-secondary);
    cursor: wait;
  }

  .play-icon {
    font-size: 16px;
  }

  .spinner {
    width: 18px;
    height: 18px;
    border: 2px solid var(--text-muted);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  .version-select {
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 6px 16px;
    color: var(--text-primary);
    font-size: 13px;
    cursor: pointer;
    -webkit-appearance: none;
    appearance: none;
    padding-right: 32px;
    background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 12 12'%3E%3Cpath fill='%23a0a0a0' d='M3 5l3 3 3-3'/%3E%3C/svg%3E");
    background-repeat: no-repeat;
    background-position: right 10px center;
  }

  .version-select:focus {
    border-color: var(--accent);
  }

  .user-badge {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text-secondary);
    font-size: 13px;
  }

  .user-avatar {
    width: 24px;
    height: 24px;
    border-radius: 50%;
    background: var(--accent-dim);
    color: var(--accent);
    display: flex;
    align-items: center;
    justify-content: center;
    font-weight: 700;
    font-size: 12px;
  }

  .stats {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 16px;
    margin-bottom: 32px;
  }

  .stat-card {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 18px 20px;
  }

  .stat-icon {
    font-size: 24px;
    width: 44px;
    height: 44px;
    border-radius: var(--radius);
    background: var(--accent-dim);
    color: var(--accent);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .stat-info {
    display: flex;
    flex-direction: column;
  }

  .stat-value {
    font-size: 22px;
    font-weight: 700;
    line-height: 1.2;
  }

  .stat-label {
    font-size: 12px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .section-title {
    font-size: 18px;
    font-weight: 700;
    margin-bottom: 16px;
  }

  .news-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 14px;
  }

  .news-card {
    position: relative;
    overflow: hidden;
    animation: fadeIn 0.3s ease forwards;
    opacity: 0;
    padding: 0;
  }

  .news-accent {
    height: 3px;
    width: 100%;
  }

  .news-body {
    padding: 16px 20px;
  }

  .news-title {
    font-size: 15px;
    font-weight: 600;
    margin-bottom: 6px;
  }

  .news-desc {
    font-size: 13px;
    color: var(--text-secondary);
    line-height: 1.5;
    margin-bottom: 10px;
  }

  .news-date {
    font-size: 11px;
    color: var(--text-muted);
  }
</style>
