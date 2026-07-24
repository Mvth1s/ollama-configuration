// Smoke test for the real installer wizard window (gui/dist/index.html +
// main.js), not a reimplementation of its logic - same principle as the
// bats suite for the Bash scripts. Only checks what step 1 renders
// synchronously at startup (see main.js's HEADERS/renderStepper), so this
// doesn't depend on detect_system's async result and stays fast/stable.
describe('Ollama Configuration installer wizard', () => {
  it('opens on step 1 with the detection title and subtitle', async () => {
    const title = await $('#step-title');
    await title.waitForExist({ timeout: 10000 });
    await expect(title).toHaveText('Analyse de la machine');

    const subtitle = await $('#step-subtitle');
    await expect(subtitle).toHaveText('Scan des composants pour choisir la meilleure configuration.');
  });

  it('shows all 4 steps in the stepper, with only step 1 active', async () => {
    const steps = await $$('#stepper button.step');
    expect(steps.length).toBe(4);

    const active = await $$('#stepper button.step.is-active');
    expect(active.length).toBe(1);
    await expect(steps[0]).toHaveElementClass('is-active');
  });

  it('shows the detection panel and hides the later steps', async () => {
    await expect($('#step-1')).not.toHaveElementClass('hidden');
    await expect($('#step-2')).toHaveElementClass('hidden');
    await expect($('#step-3')).toHaveElementClass('hidden');
    await expect($('#step-4')).toHaveElementClass('hidden');
  });
});
