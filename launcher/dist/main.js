// Vanilla JS, no npm dependency: window.__TAURI__ is injected because
// tauri.conf.json sets app.withGlobalTauri = true. qrcode.js (vendored,
// MIT-licensed "qrcode-generator" by Kazuhiko Arase) provides window.qrcode.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const webuiBtn = document.getElementById('webui-btn');
const serviceCard = document.getElementById('service-card');
const serviceDot = document.getElementById('service-dot');
const serviceStatusText = document.getElementById('service-status-text');
const serviceToggleBtn = document.getElementById('service-toggle-btn');
const lanIcon = document.getElementById('lan-icon');
const lanToggle = document.getElementById('lan-toggle');
const lanStateLabel = document.getElementById('lan-state-label');
const lanExplain = document.getElementById('lan-explain');
const lanUrlWrap = document.getElementById('lan-url-wrap');
const lanUrlText = document.getElementById('lan-url-text');
const copyUrlBtn = document.getElementById('copy-url-btn');
const qrToggleBtn = document.getElementById('qr-toggle-btn');
const qrWrap = document.getElementById('qr-wrap');
const qrCanvas = document.getElementById('qr-canvas');
const secNote = document.getElementById('sec-note');
const secIcon = document.getElementById('sec-icon');
const secText = document.getElementById('sec-text');
const modelsEmpty = document.getElementById('models-empty');
const modelsTable = document.getElementById('models-table');
const modelsBody = document.getElementById('models-body');
const pullInput = document.getElementById('pull-input');
const pullBtn = document.getElementById('pull-btn');
const pullStatus = document.getElementById('pull-status');
const errorEl = document.getElementById('error');

function showError(message) {
  errorEl.textContent = message;
  errorEl.classList.remove('hidden');
}

function clearError() {
  errorEl.textContent = '';
  errorEl.classList.add('hidden');
}

function formatBytes(bytes) {
  if (!bytes) return '-';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(1)} ${units[unit]}`;
}

async function refreshModels() {
  try {
    const models = await invoke('list_models');
    clearError();
    renderModels(models);
  } catch (err) {
    showError(String(err));
    modelsEmpty.textContent = 'Could not load models.';
    modelsEmpty.classList.remove('hidden');
    modelsTable.classList.add('hidden');
  }
}

function renderModels(models) {
  modelsBody.innerHTML = '';

  if (models.length === 0) {
    modelsEmpty.textContent = 'No model installed yet.';
    modelsEmpty.classList.remove('hidden');
    modelsTable.classList.add('hidden');
    return;
  }

  modelsEmpty.classList.add('hidden');
  modelsTable.classList.remove('hidden');

  for (const model of models) {
    const row = document.createElement('tr');

    const nameCell = document.createElement('td');
    nameCell.textContent = model.name;
    row.appendChild(nameCell);

    const sizeCell = document.createElement('td');
    sizeCell.textContent = formatBytes(model.size);
    row.appendChild(sizeCell);

    const paramsCell = document.createElement('td');
    paramsCell.textContent = model.parameterSize || '-';
    row.appendChild(paramsCell);

    const quantCell = document.createElement('td');
    quantCell.textContent = model.quantizationLevel || '-';
    row.appendChild(quantCell);

    const actionCell = document.createElement('td');
    const deleteBtn = document.createElement('button');
    deleteBtn.type = 'button';
    deleteBtn.className = 'secondary small';
    deleteBtn.textContent = 'Delete';
    deleteBtn.addEventListener('click', () => deleteModel(model.name));
    actionCell.appendChild(deleteBtn);
    row.appendChild(actionCell);

    modelsBody.appendChild(row);
  }
}

async function deleteModel(name) {
  if (!confirm(`Delete ${name}? This cannot be undone.`)) {
    return;
  }
  try {
    await invoke('delete_model', { model: name });
    clearError();
    await refreshModels();
  } catch (err) {
    showError(String(err));
  }
}

webuiBtn.addEventListener('click', async () => {
  try {
    await invoke('open_webui_window');
  } catch (err) {
    showError(String(err));
  }
});

// ---------------------------------------------------------------------------
// Service status (Open WebUI start/stop)
// ---------------------------------------------------------------------------
async function refreshServiceStatus() {
  try {
    const status = await invoke('webui_service_status');
    const active = status === 'active';
    const notInstalled = status === 'not-installed';

    serviceCard.classList.toggle('is-active', active);
    serviceDot.classList.toggle('is-active', active);
    serviceStatusText.classList.toggle('is-active', active);
    serviceStatusText.textContent = notInstalled ? 'Non installé' : active ? 'Actif · localhost:8080' : `Inactif (${status})`;

    serviceToggleBtn.disabled = notInstalled;
    serviceToggleBtn.classList.toggle('is-off', !active);
    serviceToggleBtn.textContent = active ? 'Arrêter' : 'Démarrer';
  } catch (err) {
    console.error(err);
  }
}

serviceToggleBtn.addEventListener('click', async () => {
  const wantEnabled = serviceToggleBtn.textContent === 'Démarrer';
  serviceToggleBtn.disabled = true;
  try {
    await invoke('set_webui_service', { enabled: wantEnabled });
    clearError();
  } catch (err) {
    showError(String(err));
  } finally {
    await refreshServiceStatus();
  }
});

// ---------------------------------------------------------------------------
// LAN toggle + shareable URL + QR
// ---------------------------------------------------------------------------
let lanOn = false;
let lanUrl = null;

function renderLan() {
  lanToggle.classList.toggle('is-on', lanOn);
  lanIcon.classList.toggle('is-on', lanOn);
  lanIcon.textContent = lanOn ? '📡' : '🔒';
  lanStateLabel.classList.toggle('is-on', lanOn);
  lanStateLabel.textContent = lanOn ? 'Ouvert au réseau local' : 'Local uniquement';
  lanExplain.textContent = lanOn
    ? "L'interface est accessible depuis les autres appareils de ton réseau (téléphone, autre PC)."
    : "L'interface reste visible uniquement sur cet ordinateur (localhost). Rien n'est exposé au réseau.";

  secNote.classList.toggle('is-warn', lanOn);
  secIcon.textContent = lanOn ? '⚠' : '🛡';
  secText.textContent = lanOn
    ? 'Sur ton réseau local, aucun mot de passe : à éviter sur un Wi-Fi public ou partagé.'
    : 'Protégé : rien ne sort de cet ordinateur.';

  lanUrlWrap.classList.toggle('hidden', !lanOn);
  if (!lanOn) {
    qrWrap.classList.add('hidden');
    qrToggleBtn.textContent = 'Afficher le QR code';
  }
}

async function refreshLanUrl() {
  if (!lanOn) return;
  try {
    lanUrl = await invoke('get_lan_url');
    lanUrlText.textContent = lanUrl;
  } catch (err) {
    lanUrl = null;
    lanUrlText.textContent = 'URL indisponible sur ce réseau';
  }
}

async function refreshLanStatus() {
  try {
    lanOn = await invoke('webui_lan_status');
    renderLan();
    await refreshLanUrl();
  } catch (err) {
    console.error(err);
  }
}

lanToggle.addEventListener('click', async () => {
  const wantEnabled = !lanOn;
  lanToggle.disabled = true;
  try {
    await invoke('set_webui_lan', { enabled: wantEnabled });
    lanOn = wantEnabled;
    clearError();
    renderLan();
    await refreshLanUrl();
  } catch (err) {
    showError(String(err));
  } finally {
    lanToggle.disabled = false;
  }
});

copyUrlBtn.addEventListener('click', () => {
  if (!lanUrl) return;
  try {
    navigator.clipboard && navigator.clipboard.writeText(lanUrl);
  } catch (err) {
    // clipboard access can fail silently in some webview contexts; the URL
    // is still shown in the row for manual copy.
  }
  const original = copyUrlBtn.textContent;
  copyUrlBtn.textContent = 'Copié';
  setTimeout(() => {
    copyUrlBtn.textContent = original;
  }, 1600);
});

qrToggleBtn.addEventListener('click', () => {
  const showing = !qrWrap.classList.contains('hidden');
  if (showing) {
    qrWrap.classList.add('hidden');
    qrToggleBtn.textContent = 'Afficher le QR code';
    return;
  }
  qrWrap.classList.remove('hidden');
  qrToggleBtn.textContent = 'Masquer le QR code';
  renderQr();
});

function renderQr() {
  if (!lanUrl || !window.qrcode) return;
  qrCanvas.innerHTML = '';
  const qr = window.qrcode(0, 'M');
  qr.addData(lanUrl);
  qr.make();
  qrCanvas.innerHTML = qr.createSvgTag({ cellSize: 4, margin: 0, scalable: true });
  const svg = qrCanvas.querySelector('svg');
  if (svg) {
    svg.style.width = '100%';
    svg.style.height = '100%';
  }
}

// ---------------------------------------------------------------------------
// Pull a model
// ---------------------------------------------------------------------------
listen('pull-progress', (event) => {
  const { status, completed, total } = event.payload;
  if (total && completed) {
    const pct = Math.round((completed / total) * 100);
    pullStatus.textContent = `${status} (${pct}%)`;
  } else {
    pullStatus.textContent = status;
  }
});

pullBtn.addEventListener('click', async () => {
  const name = pullInput.value.trim();
  if (!name) {
    return;
  }

  pullBtn.disabled = true;
  pullInput.disabled = true;
  pullStatus.textContent = 'Starting...';
  clearError();

  try {
    await invoke('pull_model', { model: name });
    pullStatus.textContent = `${name} ready.`;
    pullInput.value = '';
    await refreshModels();
  } catch (err) {
    pullStatus.textContent = '';
    showError(String(err));
  } finally {
    pullBtn.disabled = false;
    pullInput.disabled = false;
  }
});

refreshModels();
refreshServiceStatus();
refreshLanStatus();
