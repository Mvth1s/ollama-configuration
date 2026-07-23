// Vanilla JS, no npm dependency: window.__TAURI__ is injected because
// tauri.conf.json sets app.withGlobalTauri = true.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const USAGES = ['texte', 'code', 'reflexion', 'embeddings'];
const USAGE_LABELS = { texte: 'Texte', code: 'Code', reflexion: 'Raisonnement', embeddings: 'Embeddings' };
const USAGE_DESC = {
  texte: 'Conversation générale',
  code: 'Assistant de programmation',
  reflexion: 'Réflexion pas à pas',
  embeddings: 'Recherche sémantique',
};
const STEP_LABELS = ['Détection', 'Modèles', 'Installation', 'Terminé'];
const STEP_ICONS = { 1: '⌖', 2: '▤', 3: '⇩', 4: '🚀' };
const HEADERS = {
  1: ['Analyse de la machine', 'Scan des composants pour choisir la meilleure configuration.'],
  2: ['Modèles recommandés', 'Sélection automatique selon ta machine — modifiable.'],
  3: ['Installation en cours', 'Ollama, moteur, modèles et interface web.'],
  4: ['Terminé', 'Tout est installé et fonctionne en local.'],
};

const state = {
  step: 1,
  maxStep: 1,
  skipModels: false,
  skipWebui: false,
  detect: null, // DetectResult from the backend, once available
  models: {}, // usage -> chosen model name (mutable copy of detect.tierModels)
  openMenu: null,
  installSteps: new Map(), // id -> { label, status }
  installDone: null, // { success, message } once install-done fires
};

const el = (id) => document.getElementById(id);
const stepperEl = el('stepper');
const stepIconEl = el('step-icon');
const stepTitleEl = el('step-title');
const stepMetaEl = el('step-meta');
const stepSubtitleEl = el('step-subtitle');
const configChipEl = el('config-chip');
const detectRowsEl = el('detect-rows');
const detectResultEl = el('detect-result');
const detectResultTextEl = el('detect-result-text');
const detectErrorEl = el('detect-error');
const modelCardsEl = el('model-cards');
const modelsFooterEl = el('models-footer');
const installLabelEl = el('install-label');
const installPctEl = el('install-pct');
const progressFillEl = el('progress-fill');
const installLogEl = el('install-log');
const doneSubtitleEl = el('done-subtitle');
const summaryEl = el('summary');
const webuiBtn = el('webui-btn');
const optionsStripEl = el('options-strip');
const skipModelsInput = el('skip-models');
const skipWebuiInput = el('skip-webui');
const backBtn = el('back-btn');
const nextBtn = el('next-btn');
const dotsEl = el('dots');

function renderStepper() {
  stepperEl.innerHTML = '';
  STEP_LABELS.forEach((label, idx) => {
    const n = idx + 1;
    const done = state.step > n;
    const active = state.step === n;
    const reachable = n <= state.maxStep;

    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = `step${done ? ' is-done' : ''}${active ? ' is-active' : ''}`;
    btn.disabled = !reachable;
    btn.innerHTML = `<span class="step-badge">${done ? '✓' : n}</span><span class="step-label">${label}</span>`;
    btn.addEventListener('click', () => goStep(n));
    stepperEl.appendChild(btn);
  });
}

function renderDots() {
  dotsEl.innerHTML = '';
  for (let i = 1; i <= 4; i++) {
    const dot = document.createElement('span');
    dot.className = `dot${state.step === i ? ' is-active' : state.step > i ? ' is-past' : ''}`;
    dotsEl.appendChild(dot);
  }
}

function renderHeader() {
  const [title, subtitle] = HEADERS[state.step];
  stepIconEl.textContent = STEP_ICONS[state.step];
  stepTitleEl.textContent = title;
  stepSubtitleEl.textContent = subtitle;
  stepMetaEl.textContent = `ÉTAPE ${state.step} / 4`;

  if (state.step === 2 && state.detect) {
    configChipEl.textContent = `${state.detect.ramGb} Go · ${state.detect.gpuVendor === 'none' ? 'CPU' : state.detect.gpuVendor.toUpperCase()} · Tier ${state.detect.tier}`;
    configChipEl.classList.remove('hidden');
  } else if (state.step === 4) {
    configChipEl.textContent = 'En ligne · local';
    configChipEl.classList.remove('hidden');
  } else {
    configChipEl.classList.add('hidden');
  }
}

function renderPanels() {
  for (let i = 1; i <= 4; i++) {
    el(`step-${i}`).classList.toggle('hidden', state.step !== i);
  }
  optionsStripEl.classList.toggle('hidden', state.step >= 3);
}

function renderNav() {
  backBtn.classList.toggle('hidden', !(state.step > 1 && state.step !== 3));

  if (state.step === 3) {
    nextBtn.disabled = true;
    nextBtn.textContent = 'Installation…';
  } else if (state.step === 4) {
    nextBtn.disabled = true;
    nextBtn.textContent = 'Terminé';
  } else {
    nextBtn.disabled = state.step === 1 && !state.detect;
    nextBtn.textContent = state.step === 1 ? 'Continuer' : "Lancer l'installation";
  }
}

function render() {
  renderStepper();
  renderDots();
  renderHeader();
  renderPanels();
  renderNav();
}

function goStep(n) {
  if (n > state.maxStep) return;
  state.step = n;
  state.openMenu = null;
  render();
  if (n === 4) renderDone();
}

// ---------------------------------------------------------------------------
// Step 1: detection
// ---------------------------------------------------------------------------
const DETECT_ROW_DEFS = [
  { key: 'gpu', label: 'Carte graphique' },
  { key: 'cpu', label: 'Processeur' },
  { key: 'ram', label: 'Mémoire vive' },
  { key: 'distro', label: 'Distribution' },
];

function detectRowValue(key, d) {
  switch (key) {
    case 'gpu':
      return d.gpuVendor === 'none' ? 'Aucun GPU dédié — CPU' : `${d.gpuName || d.gpuVendor} (${d.gpuVendor})`;
    case 'cpu':
      return `${d.cpuModel} · ${d.cpuThreads} threads`;
    case 'ram':
      return `${d.ramGb} Go`;
    case 'distro':
      return d.distroPretty;
    default:
      return '';
  }
}

async function runDetection() {
  detectRowsEl.innerHTML = '';
  detectErrorEl.classList.add('hidden');
  detectResultEl.classList.add('hidden');

  const rowEls = new Map();
  for (const def of DETECT_ROW_DEFS) {
    const row = document.createElement('div');
    row.className = 'detect-row';
    row.innerHTML = `
      <span class="detect-row-icon">${def.key === 'gpu' ? '🖥' : def.key === 'cpu' ? '⚙' : def.key === 'ram' ? '▦' : '🐧'}</span>
      <div class="detect-row-body">
        <div class="detect-row-label">${def.label}</div>
        <div class="detect-row-value"></div>
      </div>
      <span class="detect-row-status"><span class="spinner"></span></span>
    `;
    detectRowsEl.appendChild(row);
    rowEls.set(def.key, row);
  }

  let detect;
  try {
    detect = await invoke('detect_system', { options: { tier: null } });
  } catch (err) {
    detectErrorEl.textContent = String(err);
    detectErrorEl.classList.remove('hidden');
    return;
  }

  state.detect = detect;
  state.models = { ...detect.tierModels };

  for (const def of DETECT_ROW_DEFS) {
    // Real values are already known; the staggered reveal below is purely a
    // UI transition, not fake/simulated detection.
    await new Promise((r) => setTimeout(r, 260));
    const row = rowEls.get(def.key);
    row.querySelector('.detect-row-value').textContent = detectRowValue(def.key, detect);
    row.querySelector('.detect-row-status').innerHTML = '✓';
    row.classList.add('is-visible');
  }

  const gpuNote =
    detect.gpuVendor === 'none'
      ? 'Configuration légère : on installe des modèles compacts qui tournent bien sur CPU.'
      : 'GPU détecté : les modèles peuvent être accélérés.';
  detectResultTextEl.innerHTML = `<b>Profil détecté · Tier ${detect.tier}.</b> ${gpuNote}`;
  detectResultEl.classList.remove('hidden');

  if (state.step === 1) {
    state.maxStep = Math.max(state.maxStep, 1);
    renderNav();
  }
}

// ---------------------------------------------------------------------------
// Step 2: model cards
// ---------------------------------------------------------------------------
function renderModelCards() {
  modelCardsEl.innerHTML = '';
  if (!state.detect) return;

  for (const usage of USAGES) {
    const candidates = state.detect.candidates[usage] || [];
    const current = state.models[usage];
    const isOpen = state.openMenu === usage;

    const card = document.createElement('div');
    card.className = 'model-card';

    const changeBtn = candidates.length
      ? `<button type="button" class="model-change-btn${isOpen ? ' is-open' : ''}" data-usage="${usage}">Changer ▾</button>`
      : '';

    const menu = candidates.length
      ? `<div class="model-menu${isOpen ? ' is-open' : ''}">${candidates
          .map(
            (c) => `
        <button type="button" class="model-option${c.model === current ? ' is-current' : ''}" data-usage="${usage}" data-model="${c.model}">
          <div><div class="model-option-name">${c.model}</div><div class="model-option-desc">${c.desc}</div></div>
          ${c.model === current ? '✓' : ''}
        </button>`
          )
          .join('')}</div>`
      : '';

    card.innerHTML = `
      <div class="model-card-top">
        <span class="model-usage-pill ${usage}">${USAGE_LABELS[usage]}</span>
      </div>
      <div class="model-name">${current}</div>
      <div class="model-desc">${USAGE_DESC[usage]}</div>
      ${changeBtn}
      ${menu}
    `;
    modelCardsEl.appendChild(card);
  }

  modelCardsEl.querySelectorAll('.model-change-btn').forEach((btn) => {
    btn.addEventListener('click', () => {
      const usage = btn.dataset.usage;
      state.openMenu = state.openMenu === usage ? null : usage;
      renderModelCards();
    });
  });
  modelCardsEl.querySelectorAll('.model-option').forEach((btn) => {
    btn.addEventListener('click', () => {
      state.models[btn.dataset.usage] = btn.dataset.model;
      state.openMenu = null;
      renderModelCards();
    });
  });

  modelsFooterEl.textContent = state.skipModels
    ? '0 modèle : téléchargement ignoré'
    : `${USAGES.length} modèles adaptés à votre machine — choisis automatiquement`;
}

// ---------------------------------------------------------------------------
// Step 3: installation (driven by real install-step / install-log events)
// ---------------------------------------------------------------------------
function computeProgress() {
  const steps = [...state.installSteps.values()];
  const counted = steps.filter((s) => s.status !== 'skipped');
  if (counted.length === 0) return 0;
  const done = counted.filter((s) => s.status === 'done').length;
  return Math.round((done / counted.length) * 100);
}

function renderProgress() {
  const pct = computeProgress();
  progressFillEl.style.width = `${pct}%`;
  installPctEl.textContent = `${pct}%`;
  installLabelEl.textContent = state.installDone ? (state.installDone.success ? 'Terminé' : "Échec de l'installation") : 'Installation en cours…';
}

function appendLog(stream, text) {
  const line = document.createElement('div');
  line.className = `log-line ${stream}`;
  line.textContent = text;
  installLogEl.appendChild(line);
  installLogEl.scrollTop = installLogEl.scrollHeight;
}

async function startInstall() {
  installLogEl.textContent = '';
  state.installSteps = new Map();
  state.installDone = null;
  renderProgress();

  const options = {
    tier: state.detect ? state.detect.tier : null,
    skipModels: state.skipModels,
    skipWebui: state.skipWebui,
    models: state.skipModels ? null : state.models,
  };

  try {
    await invoke('run_install', { options });
  } catch (err) {
    // install-done already reflects the failure via its own event; this
    // only guards against an unhandled promise rejection in the console.
    console.error(err);
  }
}

listen('install-log', (event) => {
  appendLog(event.payload.stream, event.payload.text);
});

listen('install-step', (event) => {
  const { id, label, status } = event.payload;
  state.installSteps.set(id, { label, status });
  if (state.step === 3) renderProgress();
});

listen('install-done', (event) => {
  state.installDone = event.payload;
  renderProgress();
  if (event.payload.success) {
    state.step = 4;
    state.maxStep = Math.max(state.maxStep, 4);
    render();
    renderDone();
  }
});

// ---------------------------------------------------------------------------
// Step 4: done summary
// ---------------------------------------------------------------------------
function renderDone() {
  doneSubtitleEl.textContent = state.skipWebui
    ? "Ollama tourne en local. Open WebUI n'a pas été installé."
    : 'Ouvre l’interface pour discuter avec ton IA locale. Aucune donnée ne quitte cet ordinateur.';

  const items = [
    { label: 'Ollama installé et lancé', tag: '127.0.0.1:11434' },
    state.skipModels
      ? { label: 'Modèles — ignorés', tag: 'à installer plus tard' }
      : { label: 'Modèles téléchargés', tag: `${USAGES.length} modèles` },
    state.skipWebui
      ? { label: 'Open WebUI — ignoré', tag: 'terminal seul' }
      : { label: 'Open WebUI démarré', tag: 'localhost:8080' },
  ];

  summaryEl.innerHTML = items
    .map((it) => `<div class="summary-item"><span class="summary-item-label">${it.label}</span><span class="summary-item-tag">${it.tag}</span></div>`)
    .join('');
}

webuiBtn.addEventListener('click', async () => {
  try {
    await invoke('open_webui_window');
  } catch (err) {
    console.error(err);
  }
});

// ---------------------------------------------------------------------------
// Options + nav
// ---------------------------------------------------------------------------
skipModelsInput.addEventListener('change', () => {
  state.skipModels = skipModelsInput.checked;
  if (state.step === 2) renderModelCards();
});
skipWebuiInput.addEventListener('change', () => {
  state.skipWebui = skipWebuiInput.checked;
});

backBtn.addEventListener('click', () => {
  if (state.step > 1 && state.step !== 3) goStep(state.step - 1);
});

nextBtn.addEventListener('click', () => {
  if (state.step === 1) {
    if (!state.detect) return;
    state.step = 2;
    state.maxStep = Math.max(state.maxStep, 2);
    render();
    renderModelCards();
  } else if (state.step === 2) {
    state.step = 3;
    state.maxStep = Math.max(state.maxStep, 3);
    render();
    startInstall();
  }
});

// Some WebView engines restore a checkbox's previous checked state from
// their form-data cache across relaunches; force both to match `state`
// explicitly rather than trusting whatever the DOM loaded with.
skipModelsInput.checked = state.skipModels;
skipWebuiInput.checked = state.skipWebui;

render();
runDetection();
