// What an agent's browser commands read and do on this page. It holds no way
// to reach the workspace: its answers go back only as the result of the
// script the workspace ran, never through the page's own message channel, so a
// page that calls it learns nothing it could not read itself.
(() => {
  if (Object.prototype.hasOwnProperty.call(window, 'tsumugiAgent')) return;
  // The elements the last snapshot gave references to.
  const refs = new Map();
  // What the last snapshot taken with `delta` showed, to tell what changed.
  let before = null;
  const SKIP = new Set(['SCRIPT', 'STYLE', 'NOSCRIPT', 'TEMPLATE', 'HEAD', 'META', 'LINK']);
  const INTERACTIVE = new Set(['link', 'button', 'textbox', 'checkbox', 'radio', 'combobox', 'switch', 'tab', 'menuitem', 'option', 'slider', 'searchbox']);
  const text = (value, limit = 80) => {
    const flat = String(value ?? '').replace(/\s+/g, ' ').trim();
    return flat.length > limit ? flat.slice(0, limit - 1) + '…' : flat;
  };
  const hidden = (element) => {
    if (element.hidden || element.getAttribute('aria-hidden') === 'true') return true;
    const style = getComputedStyle(element);
    return style.display === 'none' || style.visibility === 'hidden';
  };
  const INPUT_ROLES = { button: 'button', submit: 'button', reset: 'button', image: 'button', checkbox: 'checkbox', radio: 'radio', range: 'slider', search: 'searchbox', hidden: null };
  const TAG_ROLES = { A: 'link', BUTTON: 'button', SELECT: 'combobox', TEXTAREA: 'textbox', H1: 'heading', H2: 'heading', H3: 'heading', H4: 'heading', H5: 'heading', H6: 'heading', IMG: 'img', UL: 'list', OL: 'list', LI: 'listitem', NAV: 'navigation', MAIN: 'main', HEADER: 'banner', FOOTER: 'contentinfo', FORM: 'form', TABLE: 'table', DIALOG: 'dialog', ASIDE: 'complementary', SECTION: null, P: 'paragraph' };
  const roleOf = (element) => {
    const given = element.getAttribute('role');
    if (given) return given.split(/\s+/)[0];
    if (element.tagName === 'A') return element.hasAttribute('href') ? 'link' : null;
    if (element.tagName === 'INPUT') {
      const type = (element.getAttribute('type') || 'text').toLowerCase();
      return type in INPUT_ROLES ? INPUT_ROLES[type] : 'textbox';
    }
    if (element.isContentEditable && element.getAttribute('contenteditable') !== null) return 'textbox';
    if (element.hasAttribute('onclick') && !TAG_ROLES[element.tagName]) return 'button';
    return TAG_ROLES[element.tagName] ?? null;
  };
  const labelled = (element) => {
    const ids = element.getAttribute('aria-labelledby');
    if (ids) {
      const joined = ids.split(/\s+/).map((id) => document.getElementById(id)?.textContent ?? '').join(' ');
      if (text(joined)) return text(joined);
    }
    const label = element.getAttribute('aria-label');
    if (label && text(label)) return text(label);
    if (element.labels?.length) return text([...element.labels].map((l) => l.textContent).join(' '));
    return '';
  };
  const nameOf = (element, role) => {
    const given = labelled(element);
    if (given) return given;
    if (element.tagName === 'IMG') return text(element.getAttribute('alt'));
    if (element.tagName === 'INPUT' && ['button', 'submit', 'reset'].includes(element.type)) return text(element.value);
    if (['link', 'button', 'heading', 'tab', 'menuitem', 'option', 'listitem', 'checkbox', 'radio', 'switch'].includes(role)) {
      const own = text(element.textContent);
      if (own) return own;
    }
    return text(element.getAttribute('title') || element.getAttribute('placeholder') || '');
  };
  const quote = (value) => JSON.stringify(value);
  const describe = (element, role, name, ref) => {
    let line = `${role}${name ? ' ' + quote(name) : ''}`;
    if (ref) line += ` [ref=${ref}]`;
    if (role === 'heading') line += ` [level=${element.tagName[1] ?? element.getAttribute('aria-level') ?? 2}]`;
    if (role === 'link' && element.getAttribute('href')) line += ` href=${quote(text(element.getAttribute('href'), 120))}`;
    if (['checkbox', 'radio', 'switch'].includes(role)) line += element.checked || element.getAttribute('aria-checked') === 'true' ? ' [checked]' : '';
    if (element.disabled) line += ' [disabled]';
    if (['textbox', 'searchbox', 'combobox'].includes(role)) {
      // What a password field holds is never read.
      if (element.tagName === 'INPUT' && element.type === 'password') line += ' value=(hidden)';
      else if ('value' in element && element.value) line += ` value=${quote(text(element.value, 120))}`;
    }
    return line;
  };
  const key = (line) => line.replace(/ \[ref=@e\d+\]/, '');
  const walk = (root, options) => {
    const lines = [];
    let next = 1;
    const emit = (depth, line) => lines.push(options.interactive ? line : '  '.repeat(depth) + '- ' + line);
    const visit = (element, depth) => {
      if (SKIP.has(element.tagName) || hidden(element)) return;
      const role = roleOf(element);
      const named = role && (role !== 'paragraph' || text(element.textContent));
      if (!named) {
        for (const child of element.childNodes) {
          if (child.nodeType === 1) visit(child, depth);
          else if (child.nodeType === 3 && !options.interactive && text(child.nodeValue)) {
            if (options.depth == null || depth < options.depth) emit(depth, 'text ' + quote(text(child.nodeValue)));
          }
        }
        return;
      }
      const interactive = INTERACTIVE.has(role);
      if (options.interactive && !interactive) {
        for (const child of element.children) visit(child, depth);
        return;
      }
      if (options.depth != null && depth >= options.depth) return;
      const ref = interactive ? '@e' + next++ : null;
      if (ref) refs.set(ref, new WeakRef(element));
      emit(depth, describe(element, role, nameOf(element, role), ref));
      // What a link, a button or a heading says is its name.
      if (['link', 'button', 'heading', 'img', 'checkbox', 'radio', 'tab', 'menuitem', 'option'].includes(role)) return;
      if (element.tagName === 'SELECT' || element.tagName === 'TEXTAREA' || element.tagName === 'INPUT') return;
      if (role === 'paragraph') {
        const flat = text(element.textContent, 200);
        if (!element.querySelector('a, button, input, select, textarea')) {
          emit(depth + 1, 'text ' + quote(flat));
          return;
        }
      }
      for (const child of element.childNodes) {
        if (child.nodeType === 1) visit(child, depth + 1);
        else if (child.nodeType === 3 && !options.interactive && text(child.nodeValue)) emit(depth + 1, 'text ' + quote(text(child.nodeValue)));
      }
    };
    visit(root, 0);
    return lines;
  };
  const snapshot = (options = {}) => {
    refs.clear();
    const root = options.selector ? document.querySelector(options.selector) : document.body;
    if (!root) return JSON.stringify({ error: options.selector ? `no element matches ${options.selector}` : 'the page has no body yet' });
    let lines = walk(root, options);
    if (options.delta) {
      const was = before;
      before = new Set(lines.map(key));
      if (was) {
        const changed = lines.filter((line) => !was.has(key(line)));
        const unchanged = lines.length - changed.length;
        lines = changed.length ? changed : ['(nothing changed)'];
        if (unchanged && changed.length) lines.push(`(${unchanged} unchanged)`);
      }
    }
    return JSON.stringify({ url: location.href, title: document.title, snapshot: lines.join('\n') });
  };
  const target = (ref) => {
    const element = refs.get(ref)?.deref();
    if (!element || !element.isConnected) throw new Error(`${ref} is stale or unknown: take a new snapshot and use its references`);
    return element;
  };
  const failure = (error) => JSON.stringify({ error: error.message });
  const click = (ref) => {
    try {
      const element = target(ref);
      element.scrollIntoView({ block: 'center', inline: 'center' });
      if (element.disabled) throw new Error(`${ref} is disabled`);
      element.focus?.({ preventScroll: true });
      element.click();
      return JSON.stringify({ clicked: ref, url: location.href });
    } catch (error) {
      return failure(error);
    }
  };
  const fill = (ref, value) => {
    try {
      const element = target(ref);
      element.scrollIntoView({ block: 'center', inline: 'center' });
      element.focus?.({ preventScroll: true });
      if (element.tagName === 'SELECT') {
        const option = [...element.options].find((o) => o.value === value || o.text.trim() === value);
        if (!option) throw new Error(`${ref} has no option ${quote(value)}`);
        element.value = option.value;
      } else if (element.tagName === 'INPUT' || element.tagName === 'TEXTAREA') {
        if (element.tagName === 'INPUT' && ['checkbox', 'radio', 'file', 'button', 'submit', 'reset', 'image'].includes(element.type)) throw new Error(`${ref} is not a field to fill; click it`);
        // The page's own setter, so that frameworks that watch it see the change.
        const proto = element.tagName === 'INPUT' ? HTMLInputElement.prototype : HTMLTextAreaElement.prototype;
        Object.getOwnPropertyDescriptor(proto, 'value').set.call(element, value);
      } else if (element.isContentEditable) {
        element.textContent = value;
      } else {
        throw new Error(`${ref} is not a field to fill`);
      }
      element.dispatchEvent(new Event('input', { bubbles: true }));
      element.dispatchEvent(new Event('change', { bubbles: true }));
      return JSON.stringify({ filled: ref });
    } catch (error) {
      return failure(error);
    }
  };
  // What the page wrote to its console and asked of the network since it
  // loaded, for an agent to read: the newest entries replace the oldest, and
  // bodies, headers and what a field holds are not kept.
  const KEEP = 500;
  const LONGEST = 500;
  const consoleLog = { entries: [], total: 0 };
  const networkLog = { entries: [], total: 0 };
  let recording = true;
  let pending = 0;
  let lastRequest = Date.now();
  const record = (log, entry) => {
    if (!recording) return;
    log.total += 1;
    log.entries.push(entry);
    if (log.entries.length > KEEP) log.entries.shift();
  };
  const show = (value) => {
    if (typeof value === 'string') return value;
    if (value instanceof Error) return value.stack || `${value.name}: ${value.message}`;
    try {
      const json = JSON.stringify(value);
      return json === undefined ? String(value) : json;
    } catch (error) {
      return String(value);
    }
  };
  for (const [name, level] of [['log', 'log'], ['info', 'info'], ['warn', 'warning'], ['error', 'error'], ['debug', 'debug']]) {
    const original = console[name];
    if (typeof original !== 'function') continue;
    console[name] = function (...args) {
      try {
        record(consoleLog, { level, text: text(args.map(show).join(' '), LONGEST) });
      } catch (error) {}
      return original.apply(this, args);
    };
  }
  window.addEventListener('error', (event) => {
    // A script that failed, not a picture that did.
    if (event.target !== window) return;
    record(consoleLog, { level: 'error', text: text(`${event.message} (${event.filename}:${event.lineno})`, LONGEST) });
  });
  window.addEventListener('unhandledrejection', (event) => {
    record(consoleLog, { level: 'error', text: text(`Unhandled rejection: ${show(event.reason)}`, LONGEST) });
  });
  const absolute = (address) => {
    try {
      return new URL(address, location.href).href;
    } catch (error) {
      return String(address);
    }
  };
  const mimeOf = (type) => (type ? String(type).split(';')[0].trim() : undefined);
  const begin = (method, url) => {
    pending += 1;
    lastRequest = Date.now();
    const entry = { method, url: text(absolute(url), LONGEST) };
    record(networkLog, entry);
    return entry;
  };
  const finish = (entry, status, type) => {
    pending = Math.max(0, pending - 1);
    lastRequest = Date.now();
    if (status) entry.status = status;
    const mime = mimeOf(type);
    if (mime) entry.mimeType = mime;
  };
  const nativeFetch = window.fetch;
  if (typeof nativeFetch === 'function') {
    window.fetch = function (input, init) {
      let entry;
      try {
        const method = String(init?.method || (input instanceof Request ? input.method : 'GET')).toUpperCase();
        entry = begin(method, input instanceof Request ? input.url : String(input));
      } catch (error) {}
      const done = (status, type) => entry && finish(entry, status, type);
      return nativeFetch.apply(this, arguments).then(
        (response) => {
          done(response.status, response.headers.get('content-type'));
          return response;
        },
        (error) => {
          done(0);
          throw error;
        },
      );
    };
  }
  const open = XMLHttpRequest.prototype.open;
  const send = XMLHttpRequest.prototype.send;
  XMLHttpRequest.prototype.open = function (method, url) {
    this.__tsumugi = { method: String(method).toUpperCase(), url: String(url) };
    return open.apply(this, arguments);
  };
  XMLHttpRequest.prototype.send = function () {
    const asked = this.__tsumugi;
    if (asked) {
      const entry = begin(asked.method, asked.url);
      this.addEventListener('loadend', () => finish(entry, this.status, this.getResponseHeader('content-type')));
    }
    return send.apply(this, arguments);
  };
  try {
    // Everything else the page loads: pictures, scripts, styles, fonts.
    new PerformanceObserver((list) => {
      for (const item of list.getEntries()) {
        if (item.initiatorType === 'fetch' || item.initiatorType === 'xmlhttprequest') continue;
        lastRequest = Date.now();
        const entry = { method: 'GET', url: text(item.name, LONGEST) };
        if (item.responseStatus) entry.status = item.responseStatus;
        record(networkLog, entry);
      }
    }).observe({ type: 'resource', buffered: true });
  } catch (error) {}
  {
    const [navigation] = performance.getEntriesByType('navigation');
    const entry = { method: 'GET', url: text(location.href, LONGEST) };
    if (navigation?.responseStatus) entry.status = navigation.responseStatus;
    const mime = mimeOf(document.contentType);
    if (mime) entry.mimeType = mime;
    record(networkLog, entry);
  }
  const tail = (log, limit) => {
    const count = Math.max(1, Math.min(Number.isInteger(limit) ? limit : 100, KEEP));
    return JSON.stringify({ total: log.total, entries: log.entries.slice(-count) });
  };
  const capture = (on) => {
    if (on) {
      consoleLog.entries.length = 0;
      consoleLog.total = 0;
      networkLog.entries.length = 0;
      networkLog.total = 0;
    }
    recording = !!on;
    return JSON.stringify({ capturing: recording });
  };
  const visible = (element) => {
    if (!element.getClientRects().length) return false;
    const style = getComputedStyle(element);
    return style.visibility !== 'hidden' && style.display !== 'none';
  };
  const flat = (value) => String(value ?? '').replace(/\s+/g, ' ').trim();
  // Whether the page is as an agent waits for it to be; the address is the
  // window's to judge. What a field holds is never part of the text.
  const check = (wanted = {}) => {
    try {
      if (wanted.load) {
        const ready = document.readyState;
        const loaded = wanted.load === 'domcontentloaded' ? ready !== 'loading' : ready === 'complete';
        if (!loaded) return JSON.stringify({ met: false });
        if (wanted.load === 'networkidle' && (pending > 0 || Date.now() - lastRequest < 500)) return JSON.stringify({ met: false });
      }
      if (wanted.selector) {
        const found = document.querySelector(wanted.selector);
        const shown = !!found && visible(found);
        if ((wanted.state === 'hidden') === shown) return JSON.stringify({ met: false });
      }
      if (wanted.text && !flat(document.body?.innerText).includes(flat(wanted.text))) return JSON.stringify({ met: false });
      return JSON.stringify({ met: true });
    } catch (error) {
      return failure(error);
    }
  };
  Object.defineProperty(window, 'tsumugiAgent', {
    value: Object.freeze({
      snapshot,
      click,
      fill,
      check,
      capture,
      size: () => JSON.stringify({
        width: Math.max(document.documentElement.scrollWidth, document.body?.scrollWidth ?? 0),
        height: Math.max(document.documentElement.scrollHeight, document.body?.scrollHeight ?? 0),
      }),
      console: (limit) => tail(consoleLog, limit),
      network: (limit) => tail(networkLog, limit),
    }),
  });
})();
