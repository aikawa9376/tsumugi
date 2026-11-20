const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');

const events = new Map();
const on = (name, f) => events.set(name, [...(events.get(name) || []), f]);
const fire = (name, event = {}) => (events.get(name) || []).forEach(f => f(event));
const sent = [];
let redraw;
let details = [];
let scrolls = [];
const parent = { postMessage: m => sent.push(m) };
const context = {
  Set, parent, scrollY: 0, innerHeight: 500,
  addEventListener: on,
  requestAnimationFrame: f => f(),
  MutationObserver: class { constructor(f) { redraw = f; } observe() {} },
  document: {
    addEventListener: on,
    documentElement: { scrollHeight: 2000 },
    querySelector: () => ({ getAttribute: () => 'initial', querySelectorAll: () => details }),
  },
  scrollTo: (x, y) => { context.scrollY = Math.min(y, 1500); scrolls.push(y); },
};
vm.runInNewContext(fs.readFileSync(`${__dirname}/../assets/js/conversation-page.js`, 'utf8'), context);
fire('DOMContentLoaded');
assert.equal(scrolls.at(-1), 2000, 'a conversation opens at the latest reply');

context.scrollY = 300;
fire('scroll');
scrolls = [];
fire('message', { source: parent, data: { tsumugiViewer: 1, markdown: 'updated', conversation: 'first' } });
redraw();
assert.equal(scrolls.at(-1), 2000, 'a different conversation starts at its latest reply');

context.scrollY = 300;
fire('scroll');
scrolls = [];
fire('message', { source: parent, data: { tsumugiViewer: 1, markdown: 'appended', conversation: 'first' } });
context.scrollY = 0;
redraw();
assert.equal(scrolls.at(-1), 300, 'new replies do not move someone reading earlier messages');

const detail = { matches: () => true, getAttribute: () => '1', open: true };
fire('toggle', { target: detail });
details = [{ ...detail, open: false }];
redraw();
assert.equal(details[0].open, true, 'expanded progress survives a refresh');
fire('message', { source: parent, data: { tsumugiViewer: 1, markdown: 'cleared', conversation: 'second' } });
redraw();
assert.equal(details[0].open, false, 'a new conversation does not inherit expanded progress');

const raw = '**Original**\n\n```rust\nlet x = 1;\n```';
fire('mousedown');
assert.equal(sent.at(-1).tsumugiConversationFocus, 1, 'clicking the conversation directs reading keys away from the prompt');
fire('click', { target: { closest: () => ({ getAttribute: () => raw }) } });
assert.equal(sent.at(-1).text, raw, 'copy uses original Markdown');

// The transcript may arrive after srcdoc is chosen but before its scripts load.
// A finished session sends no further change to recover the lost update.
const loads = [];
const delivered = [];
let loaded = false;
const frame = {
  contentWindow: { postMessage: m => { if (loaded) delivered.push(m); } },
  addEventListener: (name, f) => { if (name === 'load') loads.push(f); },
};
const updateContext = {
  document: { getElementById: () => frame },
  id: 'conversation', message: { markdown: 'First', conversation: 'session' },
};
const updateScript = fs.readFileSync(`${__dirname}/../assets/js/conversation-update.js`, 'utf8');
vm.runInNewContext(updateScript, updateContext);
updateContext.message = { markdown: 'Latest', conversation: 'session' };
vm.runInNewContext(updateScript, updateContext);
loaded = true;
loads.forEach(f => f());
assert.equal(delivered.at(-1).markdown, 'Latest', 'the latest transcript is replayed when the frame loads');
assert.equal(loads.length, 1, 'updates install one load listener per frame');
updateContext.message = { markdown: 'Appended', conversation: 'session' };
vm.runInNewContext(updateScript, updateContext);
assert.equal(delivered.at(-1).markdown, 'Appended', 'a loaded frame receives updates immediately');
assert.equal(delivered.at(-1).tsumugiViewer, 1);
