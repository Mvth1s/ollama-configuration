// Vanilla JS, no npm dependency: window.__TAURI__ is injected because
// tauri.conf.json sets app.withGlobalTauri = true.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const webuiBtn = document.getElementById('webui-btn');
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
