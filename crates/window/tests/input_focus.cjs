const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');

class Element {
  constructor() {
    this.isConnected = true;
    this.disabled = false;
    this.readOnly = false;
    this.dataset = {};
    this.style = {};
    this.classList = { add() {}, remove() {} };
  }
  addEventListener() {}
  appendChild() {}
  getClientRects() { return [1]; }
  closest(selector) { return selector === '.dialog' && this.dialog ? this.dialog : null; }
  focus() { document.activeElement = this; }
}
class HTMLInputElement extends Element {}
class HTMLTextAreaElement extends Element {}
class HTMLSelectElement extends Element {}
const hidden = new HTMLTextAreaElement();
const document = {
  activeElement: hidden,
  body: new Element(),
  getElementById: (id) => id === 'tsumugi-keys' ? hidden : null,
  createElement: () => new Element(),
  querySelector: () => null,
  addEventListener() {},
  hasFocus: () => true,
};
const sent = [];
const context = {
  document, Element, HTMLInputElement, HTMLTextAreaElement, HTMLSelectElement,
  MutationObserver: class { observe() {} },
  requestAnimationFrame() {},
  setTimeout() {},
  dioxus: { send(message) { sent.push(message); } },
  addEventListener() {},
};
context.window = context;
vm.createContext(context);
vm.runInContext(fs.readFileSync(require.resolve('../assets/js/input.js'), 'utf8'), context);
const take = () => vm.runInContext('take()', context);

// Tab from a field to the submit button must leave Space and Enter to it.
const button = new Element();
button.dialog = new Element();
document.activeElement = button;
take();
assert.equal(document.activeElement, button, 'a dialog button keeps focus');

// A dialog closing or hiding must give the terminal its keys back.
button.isConnected = false;
take();
assert.equal(document.activeElement, hidden, 'a removed dialog returns focus');
button.isConnected = true;
button.getClientRects = () => [];
document.activeElement = button;
take();
assert.equal(document.activeElement, hidden, 'a hidden dialog returns focus');

const field = new HTMLInputElement();
document.activeElement = field;
take();
assert.equal(document.activeElement, field, 'an editable field keeps focus');
field.disabled = true;
take();
assert.equal(document.activeElement, hidden, 'a disabled field returns focus');
document.activeElement = new Element();
take();
assert.equal(document.activeElement, hidden, 'ordinary page chrome returns focus');

// MC_ESC taps Control, selects the Latin input source, then sends Escape.
// The input-source key must reach the IME without breaking an app sequence.
const key = (name, code = name, ctrl = false) => {
  const event = {
    key: name, code, ctrlKey: ctrl, altKey: false, shiftKey: false, metaKey: false,
    preventDefault() { this.defaultPrevented = true; },
  };
  context.event = event;
  vm.runInContext('onKey(event, false)', context);
  return event;
};
for (const selector of ['NonConvert', 'Eisu', 'Alphanumeric']) {
  sent.length = 0;
  for (let tap = 0; tap < 2; tap++) {
    key('Control', 'ControlLeft', true);
    assert.equal(key(selector).defaultPrevented, undefined, selector + ' stays with the input method');
    key('Escape');
  }
  assert.deepEqual(sent.filter(m => m.kind === 'key' && m.key !== 'Control').map(m => m.key),
    ['Escape', 'Escape'], selector + ' does not interrupt consecutive Escapes');
}
