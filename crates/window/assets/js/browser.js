// This page has only focus notifications, never the workspace's command IPC.
(() => {
  const tell = (message) => window.ipc.postMessage(message);
  const focus = () => tell(location.href === 'about:blank' ? 'address' : 'focus');
  window.addEventListener('focus', focus);
  window.addEventListener('blur', () => tell('blur'));
  window.addEventListener('pointerdown', focus, true);
  window.addEventListener('keydown', (event) => {
    if (event.isTrusted && event.ctrlKey && event.shiftKey && event.key === 'Escape') {
      event.preventDefault();
      tell('workspace');
    }
    if (event.isTrusted && (event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'l') {
      event.preventDefault();
      tell('address');
    }
  }, true);
})();
