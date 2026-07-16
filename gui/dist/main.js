// Vanilla JS, no npm dependency: window.__TAURI__ is injected because
// tauri.conf.json sets app.withGlobalTauri = true.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const tierSelect = document.getElementById('tier');
const skipModels = document.getElementById('skip-models');
const skipWebui = document.getElementById('skip-webui');
const installBtn = document.getElementById('install-btn');
const logEl = document.getElementById('log');
const statusEl = document.getElementById('status');

function appendLog(stream, text) {
  const line = document.createElement('div');
  line.className = `log-line log-${stream}`;
  line.textContent = text;
  logEl.appendChild(line);
  logEl.scrollTop = logEl.scrollHeight;
}

function setStatus(text, kind) {
  statusEl.textContent = text;
  statusEl.className = `status status-${kind}`;
}

listen('install-log', (event) => {
  appendLog(event.payload.stream, event.payload.text);
});

listen('install-done', (event) => {
  installBtn.disabled = false;
  installBtn.textContent = 'Install';
  setStatus(event.payload.message, event.payload.success ? 'ok' : 'error');
});

installBtn.addEventListener('click', async () => {
  logEl.textContent = '';
  installBtn.disabled = true;
  installBtn.textContent = 'Installing...';
  setStatus('Running...', 'running');

  const options = {
    tier: tierSelect.value === 'auto' ? null : tierSelect.value,
    skipModels: skipModels.checked,
    skipWebui: skipWebui.checked,
  };

  try {
    await invoke('run_install', { options });
  } catch (err) {
    // install-done already reflects the failure in the UI; this only
    // guards against an unhandled promise rejection in the console.
    console.error(err);
  }
});
