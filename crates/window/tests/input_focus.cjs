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
const context = {
  document, Element, HTMLInputElement, HTMLTextAreaElement, HTMLSelectElement,
  MutationObserver: class { observe() {} },
  requestAnimationFrame() {},
  setTimeout() {},
  dioxus: { send() {} },
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
