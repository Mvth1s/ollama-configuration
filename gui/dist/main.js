// Vanilla JS, no npm dependency: window.__TAURI__ is injected because
// tauri.conf.json sets app.withGlobalTauri = true.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const tierSelect = document.getElementById('tier');
const skipModels = document.getElementById('skip-models');
const skipWebui = document.getElementById('skip-webui');
const installBtn = document.getElementById('install-btn');
const webuiBtn = document.getElementById('webui-btn');
const logEl = document.getElementById('log');
const statusEl = document.getElementById('status');
const stepsEl = document.getElementById('steps');

// One event per script on Linux (ollama/gpu/models/webui); a single combined
// "windows" step on Windows, since setup.ps1 runs as one opaque elevated
// process there. Elements are created on first sight rather than hardcoded,
// so the same code renders either shape.
const stepEls = new Map();

function upsertStep(id, label, status) {
  let el = stepEls.get(id);
  if (!el) {
    el = document.createElement('div');
    el.innerHTML = '<span class="step-dot"></span><span class="step-label"></span>';
    stepsEl.appendChild(el);
    stepEls.set(id, el);
  }
  el.className = `step step-${status}`;
  el.querySelector('.step-label').textContent = label;
}

function resetSteps() {
  stepsEl.innerHTML = '';
  stepEls.clear();
}

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

listen('install-step', (event) => {
  const { id, label, status } = event.payload;
  upsertStep(id, label, status);
});

listen('install-done', (event) => {
  installBtn.disabled = false;
  installBtn.textContent = 'Install';
  setStatus(event.payload.message, event.payload.success ? 'ok' : 'error');

  if (!event.payload.success) {
    // A failed run stops at the step that failed; anything still shown as
    // pending/running never actually ran, so make that visible instead of
    // leaving it looking stuck.
    for (const el of stepEls.values()) {
      if (el.classList.contains('step-pending') || el.classList.contains('step-running')) {
        el.className = 'step step-aborted';
      }
    }
  }
});

installBtn.addEventListener('click', async () => {
  logEl.textContent = '';
  resetSteps();
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

// Always enabled: Open WebUI may already be running from a previous
// install. If it isn't reachable yet, the window just shows a connection
// error, same as opening the URL in any browser.
webuiBtn.addEventListener('click', async () => {
  try {
    await invoke('open_webui_window');
  } catch (err) {
    console.error(err);
  }
});
