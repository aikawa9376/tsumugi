// This page has only focus notifications, never the workspace's command IPC.
(() => {
  const tell = (message) => window.ipc.postMessage(message);
  const focus = () => tell(location.href === 'about:blank' ? 'address' : 'focus');
  window.addEventListener('focus', focus);
  window.addEventListener('blur', () => tell('blur'));
  window.addEventListener('pointerdown', focus, true);
  // The page tells where it is scrolled to, so that a page freed to save
  // memory can be scrolled back, and that it was typed into, so that it is
  // not freed with what was typed lost.
  if (window.top === window) {
    let scrolling = 0;
    window.addEventListener('scroll', () => {
      clearTimeout(scrolling);
      scrolling = setTimeout(() => tell(`scroll:${window.scrollX},${window.scrollY}`), 200);
    }, { passive: true });
    window.addEventListener('input', (event) => {
      if (event.isTrusted) tell('edited');
    }, true);
  }
  window.addEventListener('keydown', (event) => {
    if (event.isTrusted && event.ctrlKey && event.shiftKey && event.key === 'Escape') {
      event.preventDefault();
      tell('workspace');
    }
    if (event.isTrusted && (event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'l') {
      event.preventDefault();
      tell('address');
    }
    // As in a browser, the page's own keys for searching it and for
    // bringing back the page closed last are the workspace's.
    if (event.isTrusted && (event.ctrlKey || event.metaKey) && !event.shiftKey && event.key.toLowerCase() === 'f') {
      event.preventDefault();
      tell('find');
    }
    if (event.isTrusted && (event.ctrlKey || event.metaKey) && event.shiftKey && event.key.toLowerCase() === 't') {
      event.preventDefault();
      tell('reopen');
    }
  }, true);
})();
