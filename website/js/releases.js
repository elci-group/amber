(async function () {
  const container = document.getElementById('releases-container');
  const filters = document.querySelectorAll('[data-filter]');
  let allReleases = [];

  const platformIcons = {
    windows: `<svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M0 3.449 9.75 2.1v9.451H0m10.949-9.602L24 0v11.4H10.949M0 12.6h9.75v9.451L0 20.699M10.949 12.6H24V24l-12.9-1.801"/></svg>`,
    macos: `<svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M18.71 19.5c-.83 1.24-1.71 2.45-3.05 2.47-1.34.03-1.77-.79-3.29-.79-1.53 0-2 .77-3.27.82-1.31.05-2.3-1.32-3.14-2.53C4.25 17 2.94 12.45 4.7 9.39c.87-1.52 2.43-2.48 4.12-2.51 1.28-.02 2.5.87 3.29.87.78 0 2.26-1.07 3.81-.91.65.03 2.47.26 3.64 1.98-.09.06-2.17 1.28-2.15 3.81.03 3.02 2.65 4.03 2.68 4.04-.03.07-.42 1.44-1.38 2.83M13 3.5c.73-.83 1.94-1.46 2.94-1.5.13 1.17-.34 2.35-1.04 3.19-.69.85-1.83 1.51-2.95 1.42-.15-1.15.41-2.35 1.05-3.11z"/></svg>`,
    linux: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="2" y="3" width="20" height="14" rx="2"></rect><line x1="8" y1="21" x2="16" y2="21"></line><line x1="12" y1="17" x2="12" y2="21"></line><line x1="6" y1="8" x2="6.01" y2="8"></line><line x1="10" y1="8" x2="10.01" y2="8"></line></svg>`,
    source: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="16" y1="13" x2="8" y2="13"/><line x1="16" y1="17" x2="8" y2="17"/><polyline points="10 9 9 9 8 9"/></svg>`
  };

  function channelClass(channel, deprecated) {
    if (deprecated) return 'tag-deprecated';
    if (channel === 'prerelease') return 'tag-prerelease';
    return 'tag-stable';
  }

  function channelLabel(channel, deprecated) {
    if (deprecated) return 'Deprecated';
    if (channel === 'prerelease') return 'Pre-release';
    return 'Stable';
  }

  function renderRelease(release) {
    const repo = allReleases.repository || 'https://github.com/elci-group/amber';
    const downloadBase = release.local
      ? `data/releases/${release.tag}`
      : `${repo}/releases/download/${release.tag}`;
    const notice = release.deprecated && release.deprecationNotice
      ? `<div class="deprecated-notice"><svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"/><line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/></svg>${escapeHtml(release.deprecationNotice)}</div>`
      : '';

    const assets = release.assets.map(asset => {
      const icon = platformIcons[asset.platform] || platformIcons.source;
      return `<a class="asset" href="${downloadBase}/${asset.filename}" download>
        ${icon}
        <span>${escapeHtml(asset.name)}</span>
      </a>`;
    }).join('');

    return `
      <article class="release" data-channel="${release.channel}" data-deprecated="${release.deprecated}">
        <div class="release-header">
          <div class="release-title">
            <h3>${escapeHtml(release.version)}</h3>
            <span class="tag ${channelClass(release.channel, release.deprecated)}">${channelLabel(release.channel, release.deprecated)}</span>
          </div>
          <div class="release-meta">${escapeHtml(release.date)}</div>
        </div>
        <div class="release-body">
          ${notice}
          <p class="release-notes">${escapeHtml(release.notes)}</p>
          <div class="asset-grid">${assets}</div>
        </div>
      </article>
    `;
  }

  function escapeHtml(str) {
    return String(str)
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&#039;');
  }

  function render(filter) {
    const items = allReleases.releases.filter(r => {
      if (filter === 'all') return true;
      if (filter === 'deprecated') return r.deprecated;
      return r.channel === filter && !r.deprecated;
    });

    // Sort: stable > prerelease > deprecated, then newest first
    const order = { stable: 0, prerelease: 1, deprecated: 2 };
    items.sort((a, b) => {
      const ao = a.deprecated ? 2 : order[a.channel] ?? 3;
      const bo = b.deprecated ? 2 : order[b.channel] ?? 3;
      if (ao !== bo) return ao - bo;
      return new Date(b.date) - new Date(a.date);
    });

    container.innerHTML = items.length
      ? items.map(renderRelease).join('')
      : `<p class="text-center" style="color: var(--text-muted);">No releases match this filter.</p>`;
  }

  try {
    const response = await fetch('data/releases.json');
    if (!response.ok) throw new Error('Network response was not ok');
    const data = await response.json();
    allReleases = data;
    render('all');
  } catch (err) {
    container.innerHTML = `<p class="text-center" style="color: var(--danger);">Could not load releases: ${escapeHtml(err.message)}</p>`;
  }

  filters.forEach(btn => {
    btn.addEventListener('click', () => {
      filters.forEach(b => b.classList.remove('active'));
      btn.classList.add('active');
      render(btn.dataset.filter);
    });
  });
})();
