// Smoke test for the real launcher window (launcher/dist/index.html +
// main.js), not a reimplementation of its logic - same principle as the
// bats suite for the Bash scripts. Deliberately only checks the static
// heading, not the service-status/LAN-toggle state: those are populated by
// async Tauri commands (webui_service_status, etc.) shortly after load, so
// asserting on them here would be racy without an explicit wait.
describe('Ollama Launcher window', () => {
  it('shows the app heading', async () => {
    const header = await $('main > h1');
    await header.waitForExist({ timeout: 10000 });
    await expect(header).toHaveText('Ollama Launcher');
  });

  it('has the service and LAN cards present', async () => {
    await expect($('#service-card')).toExist();
    await expect($('#lan-card')).toExist();
  });
});
