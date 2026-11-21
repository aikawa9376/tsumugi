(() => {
  let armed = false;
  let mark = null;
  let seq = 0;
  // The elements picked, by the number each was given, and the numbers the
  // workspace lists them by: a marker shows where each instruction is for.
  const picked = new Map();
  let listed = [];
  let pins = [];
  let hidden = false;
  const properties = ['display', 'position', 'color', 'background-color', 'font-family', 'font-size', 'font-weight', 'line-height', 'width', 'height', 'margin', 'padding', 'gap', 'border', 'border-radius', 'align-items', 'justify-content'];
  const hide = () => { mark?.remove(); mark = null; };
  const selector = (element) => {
    const parts = [];
    let node = element;
    while (node && node.nodeType === 1) {
      if (node.id) { parts.unshift('#' + CSS.escape(node.id)); break; }
      const tag = node.localName;
      const siblings = node.parentElement ? [...node.parentElement.children].filter(e => e.localName === tag) : [];
      parts.unshift(tag + (siblings.length > 1 ? ':nth-of-type(' + (siblings.indexOf(node) + 1) + ')' : ''));
      node = node.parentElement;
    }
    const root = element.getRootNode();
    return (root.host ? selector(root.host) + ' >>> ' : '') + parts.join(' > ');
  };
  const html = (element, limit) => {
    const copy = element.cloneNode(true);
    for (const el of [copy, ...copy.querySelectorAll('input, textarea, script, style')]) {
      if (el.matches('input, textarea')) { el.removeAttribute('value'); if (el.matches('textarea')) el.textContent = ''; }
      if (el.matches('script, style')) el.remove();
    }
    return copy.outerHTML.slice(0, limit);
  };
  const source = (element) => {
    const key = Object.keys(element).find(k => k.startsWith('__reactFiber$'));
    let fiber = key ? element[key] : null;
    for (let i = 0; fiber && i < 20; i++, fiber = fiber.return) {
      const s = fiber._debugSource;
      if (s?.fileName) return s.fileName + ':' + (s.lineNumber || 1);
    }
    return element.getAttribute('data-source') || null;
  };
  const describe = (element) => {
    const r = element.getBoundingClientRect();
    const css = getComputedStyle(element);
    return {
      url: location.href, title: document.title.slice(0, 256), selector: selector(element).slice(0, 2048),
      tag: element.localName, text: (element.innerText || element.getAttribute('aria-label') || '').slice(0, 2000),
      html: html(element, 8000), nearby: element.parentElement ? html(element.parentElement, 8000) : '',
      styles: Object.fromEntries(properties.map(p => [p, css.getPropertyValue(p).slice(0, 512)])),
      rect: [r.x, r.y, r.width, r.height], viewport: [innerWidth, innerHeight], source: source(element),
    };
  };
  // Markers only say a number: what was written for an element stays in the
  // workspace, out of the page's reach.
  const place = () => {
    for (const pin of pins) pin.remove();
    pins = [];
    if (hidden) return;
    listed.forEach((key, at) => {
      const element = picked.get(key);
      if (!element?.isConnected) return;
      const r = element.getBoundingClientRect();
      if (r.width <= 0 || r.height <= 0) return;
      const pin = document.createElement('div');
      pin.textContent = String(at + 1);
      pin.style.cssText = 'position:fixed;pointer-events:none;z-index:2147483646;box-sizing:border-box;border:2px solid #3989ff;background:rgba(57,137,255,.08);';
      Object.assign(pin.style, { left: r.x + 'px', top: r.y + 'px', width: r.width + 'px', height: r.height + 'px' });
      const badge = document.createElement('span');
      badge.textContent = String(at + 1);
      badge.style.cssText = 'position:absolute;left:-2px;top:-2px;transform:translateY(-100%);min-width:18px;padding:0 5px;border-radius:9px 9px 9px 0;background:#3989ff;color:#fff;font:600 12px/18px system-ui,sans-serif;text-align:center;';
      pin.textContent = '';
      pin.append(badge);
      document.documentElement.append(pin);
      pins.push(pin);
    });
  };
  const marks = (keys) => {
    listed = Array.isArray(keys) ? keys.filter(Number.isInteger) : [];
    for (const key of picked.keys()) if (!listed.includes(key)) picked.delete(key);
    place();
  };
  const cover = (on) => { hidden = !!on; hide(); place(); };
  // Asked again while it holds, the hover highlight stays.
  const set = (on) => { on = !!on; if (armed === on) return; armed = on; hide(); };
  Object.defineProperty(window, 'tsumugiInspector', { value: Object.freeze({ set, describe, marks, cover }), configurable: false });
  let queued = false;
  const follow = () => {
    if (queued || !listed.length) return;
    queued = true;
    requestAnimationFrame(() => { queued = false; place(); });
  };
  window.addEventListener('scroll', follow, true);
  window.addEventListener('resize', follow, true);
  window.addEventListener('pointermove', (event) => {
    if (!armed || !event.isTrusted) return;
    const element = event.composedPath().find(e => e instanceof Element && e !== mark);
    if (!element) return;
    if (!mark) {
      mark = document.createElement('div');
      mark.style.cssText = 'position:fixed;pointer-events:none;z-index:2147483647;border:2px solid #3989ff;background:rgba(57,137,255,.12);box-sizing:border-box;';
      document.documentElement.append(mark);
    }
    const r = element.getBoundingClientRect();
    Object.assign(mark.style, {left:r.x+'px', top:r.y+'px', width:r.width+'px', height:r.height+'px'});
  }, true);
  // While picking, the page itself does not see the pointer.
  for (const type of ['pointerdown', 'mousedown', 'pointerup', 'mouseup', 'auxclick', 'dblclick', 'contextmenu']) {
    window.addEventListener(type, (event) => {
      if (armed && event.isTrusted) { event.preventDefault(); event.stopImmediatePropagation(); }
    }, true);
  }
  window.addEventListener('click', (event) => {
    if (!armed || !event.isTrusted) return;
    const element = event.composedPath().find(e => e instanceof Element && e !== mark);
    if (!element) return;
    event.preventDefault(); event.stopImmediatePropagation();
    hide();
    const key = ++seq;
    picked.set(key, element);
    const data = { ...describe(element), key };
    requestAnimationFrame(() => window.ipc.postMessage(JSON.stringify(data)));
  }, true);
  window.addEventListener('keydown', (event) => {
    if (armed && event.isTrusted && event.key === 'Escape') {
      event.preventDefault(); event.stopImmediatePropagation(); set(false); window.ipc.postMessage('annotation-cancel');
    }
  }, true);
})();
