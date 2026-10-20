// NetizenCore browser prelude: DOM + BOM written in JavaScript on top of the
// native primitives exposed as __dom and __host. Every section is isolated so a
// failure in one part never takes the others down.
var __prelude_errors = [];
function __section(name, fn) {
  try { fn(); } catch (e) { __prelude_errors.push(name + ': ' + (e && e.message)); }
}

var window = globalThis;
var self = globalThis;
var top = globalThis;
var parent = globalThis;
var frames = globalThis;

__section('events', function () {
  var L = '@@l';

  class Event {
    constructor(type, init) {
      this.type = String(type);
      this.bubbles = !!(init && init.bubbles);
      this.cancelable = !!(init && init.cancelable);
      this.composed = false;
      this.defaultPrevented = false;
      this.target = null;
      this.currentTarget = null;
      this.eventPhase = 0;
      this.isTrusted = false;
      this.timeStamp = __host.time();
      this['@@stop'] = false;
    }
    preventDefault() { this.defaultPrevented = true; }
    stopPropagation() { this['@@stop'] = true; }
    stopImmediatePropagation() { this['@@stop'] = true; this['@@stopi'] = true; }
    composedPath() { return []; }
    initEvent(type, bubbles, cancelable) { this.type = String(type); this.bubbles = !!bubbles; this.cancelable = !!cancelable; }
  }
  class CustomEvent extends Event {
    constructor(type, init) {
      super(type, init);
      this.detail = init && init.detail !== undefined ? init.detail : null;
    }
    initCustomEvent(type, bubbles, cancelable, detail) { this.initEvent(type, bubbles, cancelable); this.detail = detail === undefined ? null : detail; }
  }
  class UIEvent extends Event {}
  class MouseEvent extends UIEvent {
    constructor(type, init) {
      super(type, init);
      this.clientX = (init && init.clientX) || 0;
      this.clientY = (init && init.clientY) || 0;
      this.button = (init && init.button) || 0;
      this.ctrlKey = !!(init && init.ctrlKey);
      this.shiftKey = !!(init && init.shiftKey);
      this.altKey = !!(init && init.altKey);
      this.metaKey = !!(init && init.metaKey);
    }
  }
  class KeyboardEvent extends UIEvent {
    constructor(type, init) {
      super(type, init);
      this.key = (init && init.key) || '';
      this.code = (init && init.code) || '';
      this.keyCode = (init && init.keyCode) || 0;
    }
  }
  class InputEvent extends UIEvent {
    constructor(type, init) {
      super(type, init);
      this.data = (init && init.data) || null;
      this.inputType = (init && init.inputType) || '';
    }
  }
  class FocusEvent extends UIEvent {}
  class PopStateEvent extends Event {}
  class MessageEvent extends Event {
    constructor(type, init) {
      super(type, init);
      this.data = init && init.data;
      this.origin = (init && init.origin) || '';
    }
  }

  function listenersOf(target, type) {
    var l = target[L];
    return l && l[type] ? l[type].slice() : [];
  }

  function invoke(target, ev) {
    ev.currentTarget = target;
    var list = listenersOf(target, ev.type);
    var h = target['on' + ev.type];
    if (typeof h === 'function') {
      try { h.call(target, ev); } catch (e) { __host.error(e); }
    }
    for (var i = 0; i < list.length; i++) {
      var entry = list[i];
      if (entry.once) { target.removeEventListener(ev.type, entry.fn); }
      try {
        if (typeof entry.fn === 'function') { entry.fn.call(target, ev); }
        else if (entry.fn && typeof entry.fn.handleEvent === 'function') { entry.fn.handleEvent(ev); }
      } catch (e) { __host.error(e); }
      if (ev['@@stopi']) { break; }
    }
  }

  class EventTarget {
    addEventListener(type, fn, opts) {
      if (typeof fn !== 'function' && !(fn && typeof fn.handleEvent === 'function')) { return; }
      var t = this === undefined || this === null ? globalThis : this;
      var l = t[L] || (t[L] = {});
      var arr = l[type] || (l[type] = []);
      for (var i = 0; i < arr.length; i++) { if (arr[i].fn === fn) { return; } }
      arr.push({ fn: fn, once: !!(opts && typeof opts === 'object' && opts.once) });
    }
    removeEventListener(type, fn) {
      var t = this === undefined || this === null ? globalThis : this;
      var l = t[L];
      if (!l || !l[type]) { return; }
      l[type] = l[type].filter(function (e) { return e.fn !== fn; });
    }
    dispatchEvent(ev) {
      var t = this === undefined || this === null ? globalThis : this;
      if (!ev.target) { ev.target = t; }
      var path = [t];
      var n = t;
      if (ev.bubbles !== false) {
        while (n && n.parentNode) { n = n.parentNode; path.push(n); }
        if (t !== window && path.indexOf(window) < 0) { path.push(window); }
      }
      for (var i = 0; i < path.length; i++) {
        invoke(path[i], ev);
        if (ev['@@stop']) { break; }
      }
      return !ev.defaultPrevented;
    }
  }

  globalThis.Event = Event;
  globalThis.CustomEvent = CustomEvent;
  globalThis.UIEvent = UIEvent;
  globalThis.MouseEvent = MouseEvent;
  globalThis.PointerEvent = MouseEvent;
  globalThis.KeyboardEvent = KeyboardEvent;
  globalThis.InputEvent = InputEvent;
  globalThis.FocusEvent = FocusEvent;
  globalThis.PopStateEvent = PopStateEvent;
  globalThis.MessageEvent = MessageEvent;
  globalThis.EventTarget = EventTarget;
  globalThis.addEventListener = EventTarget.prototype.addEventListener;
  globalThis.removeEventListener = EventTarget.prototype.removeEventListener;
  globalThis.dispatchEvent = EventTarget.prototype.dispatchEvent;
});

__section('dom', function () {
  var D = __dom;
  var ID = '@@id';
  var wrappers = new Map();
  var documentObj = null;

  function wrap(id) {
    if (id === undefined || id === null || id < 0) { return null; }
    var w = wrappers.get(id);
    if (w) { return w; }
    var t = D.type(id);
    if (t === 9) { w = documentObj; }
    else if (t === 3) { w = new Text(id); }
    else if (D.tag(id) === '#fragment') { w = new DocumentFragment(id); }
    else { w = new Element(id); }
    wrappers.set(id, w);
    return w;
  }

  function parseStyle(s) {
    var o = {};
    if (!s) { return o; }
    var parts = s.split(';');
    for (var i = 0; i < parts.length; i++) {
      var idx = parts[i].indexOf(':');
      if (idx > 0) { o[parts[i].slice(0, idx).trim().toLowerCase()] = parts[i].slice(idx + 1).trim(); }
    }
    return o;
  }
  function styleText(o) {
    var out = '';
    for (var k in o) { if (o[k] !== '' && o[k] !== undefined) { out += k + ': ' + o[k] + '; '; } }
    return out.trim();
  }
  function kebab(k) { return k.replace(/[A-Z]/g, function (m) { return '-' + m.toLowerCase(); }); }

  function makeStyle(el) {
    var id = el[ID];
    return new Proxy({}, {
      get: function (t, k) {
        if (typeof k !== 'string') { return undefined; }
        var cur = D.getAttr(id, 'style') || '';
        if (k === 'cssText') { return cur; }
        if (k === 'length') { return Object.keys(parseStyle(cur)).length; }
        if (k === 'setProperty') {
          return function (n, v) { var o = parseStyle(D.getAttr(id, 'style')); o[String(n).toLowerCase()] = String(v); D.setAttr(id, 'style', styleText(o)); };
        }
        if (k === 'getPropertyValue') { return function (n) { return parseStyle(D.getAttr(id, 'style'))[String(n).toLowerCase()] || ''; }; }
        if (k === 'removeProperty') {
          return function (n) { var o = parseStyle(D.getAttr(id, 'style')); var old = o[String(n).toLowerCase()] || ''; delete o[String(n).toLowerCase()]; D.setAttr(id, 'style', styleText(o)); return old; };
        }
        return parseStyle(cur)[kebab(k)] || '';
      },
      set: function (t, k, v) {
        if (typeof k !== 'string') { return true; }
        if (k === 'cssText') { D.setAttr(id, 'style', String(v)); return true; }
        var o = parseStyle(D.getAttr(id, 'style'));
        var key = kebab(k);
        if (v === null || v === undefined || v === '') { delete o[key]; } else { o[key] = String(v); }
        D.setAttr(id, 'style', styleText(o));
        return true;
      },
      has: function (t, k) { return true; }
    });
  }

  class DOMTokenList {
    constructor(el) { this.el = el; }
    list() { var c = D.getAttr(this.el[ID], 'class'); return c ? c.split(/\s+/).filter(function (x) { return x; }) : []; }
    save(arr) { D.setAttr(this.el[ID], 'class', arr.join(' ')); }
    get length() { return this.list().length; }
    get value() { return this.list().join(' '); }
    item(i) { return this.list()[i] || null; }
    contains(c) { return this.list().indexOf(String(c)) >= 0; }
    add() {
      var l = this.list();
      for (var i = 0; i < arguments.length; i++) { var c = String(arguments[i]); if (l.indexOf(c) < 0) { l.push(c); } }
      this.save(l);
    }
    remove() {
      var l = this.list();
      for (var i = 0; i < arguments.length; i++) { var c = String(arguments[i]); l = l.filter(function (x) { return x !== c; }); }
      this.save(l);
    }
    toggle(c, force) {
      c = String(c);
      var has = this.contains(c);
      var want = force === undefined ? !has : !!force;
      if (want && !has) { this.add(c); }
      if (!want && has) { this.remove(c); }
      return want;
    }
    replace(a, b) { if (!this.contains(a)) { return false; } this.remove(a); this.add(b); return true; }
    forEach(fn) { this.list().forEach(fn); }
    toString() { return this.value; }
  }

  class Node extends EventTarget {
    constructor(id) { super(); this[ID] = id; }
    get nodeType() { return D.type(this[ID]); }
    get nodeName() {
      var t = D.type(this[ID]);
      if (t === 3) { return '#text'; }
      if (t === 9) { return '#document'; }
      return D.tag(this[ID]).toUpperCase();
    }
    get nodeValue() { return D.type(this[ID]) === 3 ? D.value(this[ID]) : null; }
    set nodeValue(v) { if (D.type(this[ID]) === 3) { D.setValue(this[ID], String(v)); } }
    get parentNode() { return wrap(D.parent(this[ID])); }
    get parentElement() {
      var p = D.parent(this[ID]);
      return p >= 0 && D.type(p) === 1 ? wrap(p) : null;
    }
    get childNodes() { return D.children(this[ID]).map(wrap); }
    get firstChild() { var c = D.children(this[ID]); return c.length ? wrap(c[0]) : null; }
    get lastChild() { var c = D.children(this[ID]); return c.length ? wrap(c[c.length - 1]) : null; }
    get nextSibling() {
      var id = this[ID]; var p = D.parent(id);
      if (p < 0) { return null; }
      var c = D.children(p); var i = c.indexOf(id);
      return i >= 0 && i + 1 < c.length ? wrap(c[i + 1]) : null;
    }
    get previousSibling() {
      var id = this[ID]; var p = D.parent(id);
      if (p < 0) { return null; }
      var c = D.children(p); var i = c.indexOf(id);
      return i > 0 ? wrap(c[i - 1]) : null;
    }
    get textContent() { return D.textContent(this[ID]); }
    set textContent(v) { D.setTextContent(this[ID], v === null || v === undefined ? '' : String(v)); }
    get isConnected() { return D.connected(this[ID]); }
    get ownerDocument() { return documentObj; }
    hasChildNodes() { return D.children(this[ID]).length > 0; }
    appendChild(n) { return this.insertBefore(n, null); }
    insertBefore(n, ref) {
      if (!n || typeof n !== 'object') { throw new TypeError('Failed to execute insertBefore: parameter 1 is not of type Node.'); }
      var refId = ref ? ref[ID] : -1;
      if (n instanceof DocumentFragment) {
        var kids = D.children(n[ID]);
        for (var i = 0; i < kids.length; i++) { D.insertBefore(this[ID], kids[i], refId); }
        return n;
      }
      D.insertBefore(this[ID], n[ID], refId);
      return n;
    }
    removeChild(n) { D.remove(n[ID]); return n; }
    replaceChild(n, old) { this.insertBefore(n, old); this.removeChild(old); return old; }
    cloneNode(deep) { return wrap(D.clone(this[ID], !!deep)); }
    contains(n) {
      var cur = n;
      while (cur) { if (cur === this) { return true; } cur = cur.parentNode; }
      return false;
    }
    remove() { D.remove(this[ID]); }
    isEqualNode(n) { return this === n; }
    normalize() {}
    getRootNode() { return documentObj; }
  }

  class Text extends Node {
    get data() { return D.value(this[ID]); }
    set data(v) { D.setValue(this[ID], String(v)); }
    get length() { return D.value(this[ID]).length; }
    get wholeText() { return this.data; }
  }
  class DocumentFragment extends Node {
    querySelector(s) { var r = D.query(this[ID], String(s), false); return r.length ? wrap(r[0]) : null; }
    querySelectorAll(s) { return D.query(this[ID], String(s), true).map(wrap); }
    getElementById(i) { return this.querySelector('#' + i); }
    get children() { return D.children(this[ID]).filter(function (c) { return D.type(c) === 1; }).map(wrap); }
    append() { for (var i = 0; i < arguments.length; i++) { var n = arguments[i]; this.appendChild(typeof n === 'object' ? n : documentObj.createTextNode(String(n))); } }
  }

  function reflect(proto, prop, attr) {
    Object.defineProperty(proto, prop, {
      get: function () { var v = D.getAttr(this[ID], attr); return v === null ? '' : v; },
      set: function (v) { D.setAttr(this[ID], attr, String(v)); }
    });
  }

  class Element extends Node {
    get tagName() { return D.tag(this[ID]).toUpperCase(); }
    get localName() { return D.tag(this[ID]); }
    get namespaceURI() { return 'http://www.w3.org/1999/xhtml'; }
    get id() { var v = D.getAttr(this[ID], 'id'); return v === null ? '' : v; }
    set id(v) { D.setAttr(this[ID], 'id', String(v)); }
    get className() { var v = D.getAttr(this[ID], 'class'); return v === null ? '' : v; }
    set className(v) { D.setAttr(this[ID], 'class', String(v)); }
    get classList() { return new DOMTokenList(this); }
    get style() { return makeStyle(this); }
    set style(v) { D.setAttr(this[ID], 'style', String(v)); }
    get dataset() {
      var id = this[ID];
      return new Proxy({}, {
        get: function (t, k) {
          if (typeof k !== 'string') { return undefined; }
          var v = D.getAttr(id, 'data-' + kebab(k));
          return v === null ? undefined : v;
        },
        set: function (t, k, v) { D.setAttr(id, 'data-' + kebab(String(k)), String(v)); return true; },
        has: function (t, k) { return D.getAttr(id, 'data-' + kebab(String(k))) !== null; }
      });
    }
    getAttribute(n) { return D.getAttr(this[ID], String(n)); }
    setAttribute(n, v) { D.setAttr(this[ID], String(n), String(v)); }
    removeAttribute(n) { D.removeAttr(this[ID], String(n)); }
    hasAttribute(n) { return D.getAttr(this[ID], String(n)) !== null; }
    hasAttributes() { return D.attrNames(this[ID]).length > 0; }
    toggleAttribute(n, force) {
      var has = this.hasAttribute(n);
      var want = force === undefined ? !has : !!force;
      if (want && !has) { this.setAttribute(n, ''); }
      if (!want && has) { this.removeAttribute(n); }
      return want;
    }
    getAttributeNames() { return D.attrNames(this[ID]); }
    get attributes() {
      var id = this[ID];
      return D.attrNames(id).map(function (n) { return { name: n, value: D.getAttr(id, n) }; });
    }
    get children() { return D.children(this[ID]).filter(function (c) { return D.type(c) === 1; }).map(wrap); }
    get childElementCount() { return this.children.length; }
    get firstElementChild() { var c = this.children; return c.length ? c[0] : null; }
    get lastElementChild() { var c = this.children; return c.length ? c[c.length - 1] : null; }
    get nextElementSibling() {
      var n = this.nextSibling;
      while (n && n.nodeType !== 1) { n = n.nextSibling; }
      return n;
    }
    get previousElementSibling() {
      var n = this.previousSibling;
      while (n && n.nodeType !== 1) { n = n.previousSibling; }
      return n;
    }
    get innerHTML() { return D.innerHTML(this[ID]); }
    set innerHTML(v) { D.setInnerHTML(this[ID], v === null || v === undefined ? '' : String(v)); }
    get outerHTML() { return D.outerHTML(this[ID]); }
    get innerText() { return D.textContent(this[ID]); }
    set innerText(v) { D.setTextContent(this[ID], String(v)); }
    querySelector(s) { var r = D.query(this[ID], String(s), false); return r.length ? wrap(r[0]) : null; }
    querySelectorAll(s) { return D.query(this[ID], String(s), true).map(wrap); }
    getElementsByTagName(t) { return this.querySelectorAll(String(t)); }
    getElementsByClassName(c) {
      var parts = String(c).trim().split(/\s+/).filter(function (x) { return x; });
      if (!parts.length) { return []; }
      return this.querySelectorAll('.' + parts.join('.'));
    }
    matches(s) { return D.matches(this[ID], String(s)); }
    webkitMatchesSelector(s) { return this.matches(s); }
    closest(s) {
      var n = this;
      while (n && n.nodeType === 1) { if (n.matches(s)) { return n; } n = n.parentElement; }
      return null;
    }
    append() { for (var i = 0; i < arguments.length; i++) { var n = arguments[i]; this.appendChild(n !== null && typeof n === 'object' ? n : documentObj.createTextNode(String(n))); } }
    prepend() {
      var first = this.firstChild;
      for (var i = 0; i < arguments.length; i++) { var n = arguments[i]; this.insertBefore(n !== null && typeof n === 'object' ? n : documentObj.createTextNode(String(n)), first); }
    }
    before() { var p = this.parentNode; if (!p) { return; } for (var i = 0; i < arguments.length; i++) { var n = arguments[i]; p.insertBefore(n !== null && typeof n === 'object' ? n : documentObj.createTextNode(String(n)), this); } }
    after() { var p = this.parentNode; if (!p) { return; } var nx = this.nextSibling; for (var i = 0; i < arguments.length; i++) { var n = arguments[i]; p.insertBefore(n !== null && typeof n === 'object' ? n : documentObj.createTextNode(String(n)), nx); } }
    replaceWith(n) { var p = this.parentNode; if (p) { p.insertBefore(n, this); p.removeChild(this); } }
    insertAdjacentHTML(pos, html) {
      var tmp = documentObj.createElement('div');
      tmp.innerHTML = html;
      var kids = tmp.childNodes;
      for (var i = 0; i < kids.length; i++) {
        if (pos === 'beforeend') { this.appendChild(kids[i]); }
        else if (pos === 'afterbegin') { this.insertBefore(kids[i], this.firstChild); }
        else if (pos === 'beforebegin' && this.parentNode) { this.parentNode.insertBefore(kids[i], this); }
        else if (pos === 'afterend' && this.parentNode) { this.parentNode.insertBefore(kids[i], this.nextSibling); }
      }
    }
    insertAdjacentElement(pos, el) { this.insertAdjacentHTML(pos, ''); if (pos === 'beforeend') { this.appendChild(el); } else if (pos === 'afterbegin') { this.insertBefore(el, this.firstChild); } return el; }
    get value() {
      var tag = D.tag(this[ID]);
      if (tag === 'textarea') { return D.textContent(this[ID]); }
      if (tag === 'select') {
        var sel = this.querySelector('option[selected]') || this.querySelector('option');
        return sel ? (sel.getAttribute('value') !== null ? sel.getAttribute('value') : sel.textContent) : '';
      }
      var v = D.getAttr(this[ID], 'value');
      return v === null ? '' : v;
    }
    set value(v) {
      if (D.tag(this[ID]) === 'textarea') { D.setTextContent(this[ID], String(v)); }
      else { D.setAttr(this[ID], 'value', String(v)); }
    }
    get checked() { return D.getAttr(this[ID], 'checked') !== null; }
    set checked(b) { if (b) { D.setAttr(this[ID], 'checked', ''); } else { D.removeAttr(this[ID], 'checked'); } }
    get disabled() { return D.getAttr(this[ID], 'disabled') !== null; }
    set disabled(b) { if (b) { D.setAttr(this[ID], 'disabled', ''); } else { D.removeAttr(this[ID], 'disabled'); } }
    get hidden() { return D.getAttr(this[ID], 'hidden') !== null; }
    set hidden(b) { if (b) { D.setAttr(this[ID], 'hidden', ''); } else { D.removeAttr(this[ID], 'hidden'); } }
    get selected() { return D.getAttr(this[ID], 'selected') !== null; }
    set selected(b) { if (b) { D.setAttr(this[ID], 'selected', ''); } else { D.removeAttr(this[ID], 'selected'); } }
    get form() { return this.closest('form'); }
    get options() { return this.querySelectorAll('option'); }
    get tabIndex() { return Number(D.getAttr(this[ID], 'tabindex') || 0); }
    set tabIndex(v) { D.setAttr(this[ID], 'tabindex', String(v)); }
    click() { var e = new MouseEvent('click', { bubbles: true, cancelable: true }); e.target = this; this.dispatchEvent(e); }
    focus() {}
    blur() {}
    select() {}
    scrollIntoView() {}
    scrollTo() {}
    animate() { return { cancel: function () {}, finished: Promise.resolve() }; }
    attachShadow() { return this; }
    getContext(kind) {
      if (kind !== '2d') { return null; }
      var self3 = this;
      var fns = {
        measureText: function (t) { return { width: String(t).length * 6 }; },
        createLinearGradient: function () { return { addColorStop: function () {} }; },
        createRadialGradient: function () { return { addColorStop: function () {} }; },
        createPattern: function () { return {}; },
        getImageData: function (x, y, w, h) { return { width: w, height: h, data: new Uint8ClampedArray(Math.max(0, w * h * 4)) }; },
        createImageData: function (w, h) { return { width: w, height: h, data: new Uint8ClampedArray(Math.max(0, w * h * 4)) }; }
      };
      return new Proxy({ canvas: self3 }, {
        get: function (t, k) {
          if (k in t) { return t[k]; }
          if (typeof k === 'string' && fns[k]) { return fns[k]; }
          return function () {};
        },
        set: function (t, k, v) { t[k] = v; return true; }
      });
    }
    toDataURL() { return 'data:,'; }
    toBlob(cb) { if (typeof cb === 'function') { setTimeout(function () { cb(null); }, 0); } }
    getBoundingClientRect() { return { x: 0, y: 0, top: 0, left: 0, right: 0, bottom: 0, width: 0, height: 0 }; }
    getClientRects() { return []; }
    get offsetWidth() { return 0; }
    get offsetHeight() { return 0; }
    get offsetTop() { return 0; }
    get offsetLeft() { return 0; }
    get clientWidth() { return 0; }
    get clientHeight() { return 0; }
    get scrollWidth() { return 0; }
    get scrollHeight() { return 0; }
    get scrollTop() { return 0; }
    set scrollTop(v) {}
    get scrollLeft() { return 0; }
    set scrollLeft(v) {}
    get offsetParent() { return this.parentElement; }
    get shadowRoot() { return null; }
    get content() { return this; }
  }
  ['href', 'src', 'alt', 'title', 'type', 'name', 'placeholder', 'action', 'method', 'target', 'rel', 'lang', 'dir', 'role', 'htmlFor:for', 'max', 'min', 'step', 'pattern', 'accept', 'autocomplete'].forEach(function (p) {
    var parts = p.split(':');
    reflect(Element.prototype, parts[0], parts[1] || parts[0]);
  });

  class Document extends Node {
    get documentElement() { return this.querySelector('html'); }
    get body() { return this.querySelector('body'); }
    get head() { return this.querySelector('head'); }
    get title() { var t = this.querySelector('title'); return t ? t.textContent : ''; }
    set title(v) {
      var t = this.querySelector('title');
      if (!t) { t = this.createElement('title'); var h = this.head; if (h) { h.appendChild(t); } }
      t.textContent = String(v);
    }
    get readyState() { return this['@@rs'] || 'loading'; }
    get cookie() { return __host.getCookie(); }
    set cookie(v) { __host.setCookie(String(v)); }
    get fonts() { return { ready: Promise.resolve(), load: function () { return Promise.resolve([]); }, check: function () { return true; }, addEventListener: function () {}, removeEventListener: function () {}, forEach: function () {}, size: 0 }; }
    get URL() { return location.href; }
    get documentURI() { return location.href; }
    get baseURI() { return location.href; }
    get referrer() { return ''; }
    get domain() { return location.hostname; }
    get defaultView() { return window; }
    get activeElement() { return this.body; }
    get scrollingElement() { return this.documentElement; }
    get characterSet() { return 'UTF-8'; }
    get contentType() { return 'text/html'; }
    get visibilityState() { return 'visible'; }
    get hidden() { return false; }
    get forms() { return this.querySelectorAll('form'); }
    get images() { return this.querySelectorAll('img'); }
    get links() { return this.querySelectorAll('a[href]'); }
    get scripts() { return this.querySelectorAll('script'); }
    createElement(t) { return wrap(D.create(String(t))); }
    createElementNS(ns, t) { return wrap(D.create(String(t))); }
    createTextNode(s) { return wrap(D.text(String(s))); }
    createComment() { return wrap(D.text('')); }
    createDocumentFragment() { return wrap(D.create('#fragment')); }
    createEvent(t) {
      var n = String(t).toLowerCase();
      if (n.indexOf('custom') === 0) { return new CustomEvent(''); }
      if (n.indexOf('mouse') === 0) { return new MouseEvent(''); }
      if (n.indexOf('key') === 0) { return new KeyboardEvent(''); }
      return new Event('');
    }
    createRange() { return { setStart: function () {}, setEnd: function () {}, selectNodeContents: function () {}, collapse: function () {}, getBoundingClientRect: function () { return { top: 0, left: 0, width: 0, height: 0 }; } }; }
    createTreeWalker() { return { nextNode: function () { return null; } }; }
    getElementById(i) { var n = D.byId(String(i)); return n < 0 ? null : wrap(n); }
    querySelector(s) { var r = D.query(0, String(s), false); return r.length ? wrap(r[0]) : null; }
    querySelectorAll(s) { return D.query(0, String(s), true).map(wrap); }
    getElementsByTagName(t) { return this.querySelectorAll(String(t)); }
    getElementsByClassName(c) {
      var parts = String(c).trim().split(/\s+/).filter(function (x) { return x; });
      return parts.length ? this.querySelectorAll('.' + parts.join('.')) : [];
    }
    getElementsByName(n) { return this.querySelectorAll('[name="' + n + '"]'); }
    hasFocus() { return true; }
    open() {}
    close() {}
    write() {}
    writeln() {}
    execCommand() { return false; }
    elementFromPoint() { return null; }
    getSelection() { return { rangeCount: 0, toString: function () { return ''; }, removeAllRanges: function () {}, addRange: function () {} }; }
  }

  documentObj = new Document(0);
  wrappers.set(0, documentObj);
  globalThis.document = documentObj;
  globalThis.Node = Node;
  globalThis.Text = Text;
  globalThis.Element = Element;
  globalThis.Document = Document;
  globalThis.DocumentFragment = DocumentFragment;
  globalThis.DOMTokenList = DOMTokenList;
  ['HTMLElement', 'HTMLDivElement', 'HTMLSpanElement', 'HTMLInputElement', 'HTMLButtonElement', 'HTMLFormElement', 'HTMLAnchorElement', 'HTMLImageElement', 'HTMLScriptElement', 'HTMLStyleElement', 'HTMLLinkElement', 'HTMLTextAreaElement', 'HTMLSelectElement', 'HTMLCanvasElement', 'HTMLBodyElement', 'HTMLHtmlElement', 'SVGElement', 'HTMLIFrameElement', 'HTMLVideoElement', 'HTMLAudioElement'].forEach(function (n) { globalThis[n] = Element; });
  globalThis.__wrap = wrap;
  Node.ELEMENT_NODE = 1;
  Node.ATTRIBUTE_NODE = 2;
  Node.TEXT_NODE = 3;
  Node.COMMENT_NODE = 8;
  Node.DOCUMENT_NODE = 9;
  Node.DOCUMENT_FRAGMENT_NODE = 11;
  globalThis.NodeFilter = { SHOW_ALL: 4294967295, SHOW_ELEMENT: 1, SHOW_TEXT: 4, FILTER_ACCEPT: 1, FILTER_REJECT: 2, FILTER_SKIP: 3 };
  globalThis.Image = function Image() { return documentObj.createElement('img'); };
  globalThis.Audio = function Audio() { return documentObj.createElement('audio'); };
  globalThis.Option = function Option(text, value) {
    var o = documentObj.createElement('option');
    if (text !== undefined) { o.textContent = String(text); }
    if (value !== undefined) { o.setAttribute('value', String(value)); }
    return o;
  };
  globalThis.DOMParser = class DOMParser {
    parseFromString(str) {
      var div = documentObj.createElement('div');
      div.innerHTML = String(str);
      return {
        body: div, documentElement: div, head: div,
        querySelector: function (q) { return div.querySelector(q); },
        querySelectorAll: function (q) { return div.querySelectorAll(q); },
        getElementById: function (i) { return div.querySelector('#' + i); }
      };
    }
  };
  globalThis.__fire_load = function (id, ok) {
    var el = wrap(id);
    if (!el) { return; }
    var e = new Event(ok ? 'load' : 'error');
    e.target = el;
    el.dispatchEvent(e);
  };

  globalThis.__click = function (id) {
    var el = wrap(id);
    if (!el) { return false; }
    var e = new MouseEvent('click', { bubbles: true, cancelable: true });
    e.target = el;
    el.dispatchEvent(e);
    return e.defaultPrevented;
  };
  globalThis.__changed = function (id) {
    var el = wrap(id);
    if (!el) { return; }
    var e = new InputEvent('input', { bubbles: true });
    e.target = el;
    el.dispatchEvent(e);
    var c = new Event('change', { bubbles: true });
    c.target = el;
    el.dispatchEvent(c);
  };
  globalThis.__submit = function (id) {
    var el = wrap(id);
    if (!el) { return false; }
    var e = new Event('submit', { bubbles: true, cancelable: true });
    e.target = el;
    el.dispatchEvent(e);
    return e.defaultPrevented;
  };
  globalThis.__key = function (id, key) {
    var el = wrap(id) || documentObj.body;
    if (!el) { return false; }
    var e = new KeyboardEvent('keydown', { bubbles: true, cancelable: true, key: key });
    e.target = el;
    el.dispatchEvent(e);
    var u = new KeyboardEvent('keyup', { bubbles: true, cancelable: true, key: key });
    u.target = el;
    el.dispatchEvent(u);
    return e.defaultPrevented;
  };
  globalThis.__fire_ready = function () {
    documentObj['@@rs'] = 'interactive';
    var rs = new Event('readystatechange', { bubbles: false });
    rs.target = documentObj;
    documentObj.dispatchEvent(rs);
    var dc = new Event('DOMContentLoaded', { bubbles: true });
    dc.target = documentObj;
    documentObj.dispatchEvent(dc);
    documentObj['@@rs'] = 'complete';
    var rs2 = new Event('readystatechange', { bubbles: false });
    rs2.target = documentObj;
    documentObj.dispatchEvent(rs2);
    var ld = new Event('load', { bubbles: false });
    ld.target = window;
    window.dispatchEvent(ld);
  };
});

__section('location', function () {
  var H = __host;
  function parts() {
    var m = /^([a-zA-Z][a-zA-Z0-9+.-]*:)\/\/([^\/?#:]*)(?::(\d+))?([^?#]*)(\?[^#]*)?(#.*)?$/.exec(H.location());
    if (!m) { return { protocol: 'about:', hostname: '', port: '', pathname: 'blank', search: '', hash: '' }; }
    return { protocol: m[1], hostname: m[2], port: m[3] || '', pathname: m[4] || '/', search: m[5] || '', hash: m[6] || '' };
  }
  class Location {
    get href() { return H.location(); }
    set href(v) { H.navigate(String(v)); }
    get protocol() { return parts().protocol; }
    get hostname() { return parts().hostname; }
    get port() { return parts().port; }
    get host() { var p = parts(); return p.port ? p.hostname + ':' + p.port : p.hostname; }
    get origin() { var p = parts(); return p.protocol + '//' + this.host; }
    get pathname() { return parts().pathname; }
    set pathname(v) { H.navigate(this.origin + (String(v).charAt(0) === '/' ? '' : '/') + v); }
    get search() { return parts().search; }
    set search(v) { H.navigate(this.origin + this.pathname + (String(v) && String(v).charAt(0) !== '?' ? '?' : '') + v); }
    get hash() { return parts().hash; }
    set hash(v) {}
    assign(u) { H.navigate(String(u)); }
    replace(u) { H.navigate(String(u)); }
    reload() { H.navigate(H.location()); }
    toString() { return H.location(); }
  }
  globalThis.Location = Location;
  globalThis.location = new Location();
  document.location = globalThis.location;
});

__section('url', function () {
  function enc(s) { return encodeURIComponent(String(s)).replace(/%20/g, '+'); }
  function dec(s) { return decodeURIComponent(String(s).replace(/\+/g, ' ')); }
  class URLSearchParams {
    constructor(init) {
      this.list = [];
      if (typeof init === 'string') {
        var s = init.charAt(0) === '?' ? init.slice(1) : init;
        if (s) {
          var pairs = s.split('&');
          for (var i = 0; i < pairs.length; i++) {
            var kv = pairs[i].split('=');
            this.list.push([dec(kv[0]), dec(kv.slice(1).join('='))]);
          }
        }
      } else if (init && typeof init === 'object') {
        var keys = Object.keys(init);
        for (var j = 0; j < keys.length; j++) { this.list.push([keys[j], String(init[keys[j]])]); }
      }
    }
    append(k, v) { this.list.push([String(k), String(v)]); }
    delete(k) { this.list = this.list.filter(function (p) { return p[0] !== k; }); }
    get(k) { for (var i = 0; i < this.list.length; i++) { if (this.list[i][0] === k) { return this.list[i][1]; } } return null; }
    getAll(k) { return this.list.filter(function (p) { return p[0] === k; }).map(function (p) { return p[1]; }); }
    has(k) { return this.get(k) !== null; }
    set(k, v) {
      var found = false;
      var out = [];
      for (var i = 0; i < this.list.length; i++) {
        if (this.list[i][0] === k) { if (!found) { out.push([String(k), String(v)]); found = true; } }
        else { out.push(this.list[i]); }
      }
      if (!found) { out.push([String(k), String(v)]); }
      this.list = out;
    }
    forEach(fn) { for (var i = 0; i < this.list.length; i++) { fn(this.list[i][1], this.list[i][0], this); } }
    keys() { return this.list.map(function (p) { return p[0]; }); }
    values() { return this.list.map(function (p) { return p[1]; }); }
    entries() { return this.list.map(function (p) { return [p[0], p[1]]; }); }
    sort() { this.list.sort(function (a, b) { return a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0; }); }
    toString() { return this.list.map(function (p) { return enc(p[0]) + '=' + enc(p[1]); }).join('&'); }
  }
  URLSearchParams.prototype['@@iterator'] = function () {
    var i = 0;
    var l = this.list;
    return { next: function () { if (i < l.length) { var p = l[i]; i++; return { value: [p[0], p[1]], done: false }; } return { value: undefined, done: true }; } };
  };

  function resolve(href, base) {
    href = String(href);
    if (/^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(href)) { return href; }
    var b = /^([a-zA-Z][a-zA-Z0-9+.-]*:)\/\/([^\/?#]*)([^?#]*)/.exec(String(base || location.href));
    if (!b) { throw new TypeError('Invalid URL'); }
    if (href.slice(0, 2) === '//') { return b[1] + href; }
    if (href.charAt(0) === '/') { return b[1] + '//' + b[2] + href; }
    if (href.charAt(0) === '?' || href.charAt(0) === '#') { return b[1] + '//' + b[2] + b[3] + href; }
    var dir = b[3].slice(0, b[3].lastIndexOf('/') + 1) || '/';
    var segs = (dir + href).split('/');
    var out = [];
    for (var i = 0; i < segs.length; i++) {
      if (segs[i] === '..') { if (out.length > 1) { out.pop(); } }
      else if (segs[i] !== '.') { out.push(segs[i]); }
    }
    return b[1] + '//' + b[2] + out.join('/');
  }
  class URL {
    constructor(u, base) {
      var full = resolve(u, base);
      var m = /^([a-zA-Z][a-zA-Z0-9+.-]*:)\/\/([^\/?#:]*)(?::(\d+))?([^?#]*)(\?[^#]*)?(#.*)?$/.exec(full);
      if (!m) { throw new TypeError('Invalid URL'); }
      this.protocol = m[1];
      this.hostname = m[2];
      this.port = m[3] || '';
      this.pathname = m[4] || '/';
      this.search = m[5] || '';
      this.hash = m[6] || '';
      this.searchParams = new URLSearchParams(this.search);
    }
    get host() { return this.port ? this.hostname + ':' + this.port : this.hostname; }
    get origin() { return this.protocol + '//' + this.host; }
    get href() {
      var q = this.searchParams.toString();
      return this.origin + this.pathname + (q ? '?' + q : '') + this.hash;
    }
    toString() { return this.href; }
    toJSON() { return this.href; }
  }
  URL.createObjectURL = function () { return 'blob:netizencore'; };
  URL.revokeObjectURL = function () {};
  globalThis.URL = URL;
  globalThis.URLSearchParams = URLSearchParams;
  globalThis.__resolve_url = resolve;
});

__section('storage', function () {
  function makeStorage() {
    var data = new Map();
    var api = {
      getItem: function (k) { k = String(k); return data.has(k) ? data.get(k) : null; },
      setItem: function (k, v) { data.set(String(k), String(v)); },
      removeItem: function (k) { data.delete(String(k)); },
      clear: function () { data.clear(); },
      key: function (i) { return Array.from(data.keys())[i] || null; }
    };
    return new Proxy({}, {
      get: function (t, k) {
        if (k === 'length') { return data.size; }
        if (typeof k === 'string' && Object.prototype.hasOwnProperty.call(api, k)) { return api[k]; }
        return typeof k === 'string' && data.has(k) ? data.get(k) : undefined;
      },
      set: function (t, k, v) { data.set(String(k), String(v)); return true; },
      has: function (t, k) { return data.has(String(k)); }
    });
  }
  globalThis.localStorage = makeStorage();
  globalThis.sessionStorage = makeStorage();
});

__section('bom', function () {
  globalThis.navigator = {
    userAgent: __host.userAgent(),
    appVersion: '5.0 (X11)',
    appName: 'Netscape',
    product: 'Gecko',
    vendor: '',
    platform: 'Linux x86_64',
    language: 'tr-TR',
    languages: ['tr-TR', 'tr', 'en'],
    onLine: true,
    cookieEnabled: true,
    webdriver: false,
    hardwareConcurrency: 2,
    maxTouchPoints: 0,
    deviceMemory: 2,
    doNotTrack: null,
    sendBeacon: function () { return true; },
    clipboard: { writeText: function () { return Promise.resolve(); }, readText: function () { return Promise.resolve(''); } },
    serviceWorker: undefined,
    mediaDevices: undefined,
    permissions: { query: function () { return Promise.resolve({ state: 'denied' }); } }
  };
  globalThis.screen = { width: 1280, height: 720, availWidth: 1280, availHeight: 720, colorDepth: 24, pixelDepth: 24, orientation: { type: 'landscape-primary', angle: 0 } };
  globalThis.innerWidth = 1280;
  globalThis.innerHeight = 720;
  globalThis.outerWidth = 1280;
  globalThis.outerHeight = 720;
  globalThis.devicePixelRatio = 1;
  globalThis.pageXOffset = 0;
  globalThis.pageYOffset = 0;
  globalThis.scrollX = 0;
  globalThis.scrollY = 0;
  globalThis.performance = {
    now: function () { return __host.time(); },
    timeOrigin: 0,
    timing: {},
    mark: function () {},
    measure: function () {},
    getEntriesByType: function () { return []; },
    getEntriesByName: function () { return []; }
  };
  globalThis.history = { length: 1, state: null, pushState: function () {}, replaceState: function () {}, back: function () {}, forward: function () {}, go: function () {}, scrollRestoration: 'auto' };
  globalThis.getComputedStyle = function (el) {
    return new Proxy({}, {
      get: function (t, k) {
        if (k === 'getPropertyValue') { return function (n) { return el.style.getPropertyValue(n); }; }
        return typeof k === 'string' ? el.style[k] : undefined;
      }
    });
  };
  globalThis.matchMedia = function (q) {
    return { matches: false, media: String(q), onchange: null, addListener: function () {}, removeListener: function () {}, addEventListener: function () {}, removeEventListener: function () {}, dispatchEvent: function () { return false; } };
  };
  globalThis.scrollTo = function () {};
  globalThis.scroll = function () {};
  globalThis.scrollBy = function () {};
  globalThis.alert = function (m) { console.log('alert: ' + m); };
  globalThis.confirm = function () { return false; };
  globalThis.prompt = function () { return null; };
  globalThis.open = function () { return null; };
  globalThis.close = function () {};
  globalThis.focus = function () {};
  globalThis.blur = function () {};
  globalThis.postMessage = function (data) {
    setTimeout(function () { var e = new MessageEvent('message', { data: data, origin: location.origin }); e.target = window; window.dispatchEvent(e); }, 0);
  };
  globalThis.requestIdleCallback = function (fn) {
    return setTimeout(function () { fn({ didTimeout: false, timeRemaining: function () { return 10; } }); }, 1);
  };
  globalThis.cancelIdleCallback = function (id) { clearTimeout(id); };
  globalThis.CSS = { supports: function () { return true; }, escape: function (s) { return String(s); } };
  globalThis.visualViewport = { width: 1280, height: 720, scale: 1, offsetLeft: 0, offsetTop: 0, pageLeft: 0, pageTop: 0, addEventListener: function () {}, removeEventListener: function () {} };
  globalThis.Notification = { permission: 'denied', requestPermission: function () { return Promise.resolve('denied'); } };
  globalThis.WebSocket = class WebSocket extends EventTarget {
    constructor(url) {
      super();
      this.url = String(url);
      this.readyState = 0;
      this.onopen = null; this.onmessage = null; this.onerror = null; this.onclose = null;
      var me = this;
      setTimeout(function () {
        me.readyState = 3;
        var e = new Event('error'); e.target = me; me.dispatchEvent(e);
        var c = new Event('close'); c.target = me; me.dispatchEvent(c);
      }, 0);
    }
    send() {}
    close() { this.readyState = 3; }
  };
  globalThis.WebSocket.CONNECTING = 0; globalThis.WebSocket.OPEN = 1; globalThis.WebSocket.CLOSING = 2; globalThis.WebSocket.CLOSED = 3;
  globalThis.EventSource = class EventSource extends EventTarget {
    constructor(url) { super(); this.url = String(url); this.readyState = 2; }
    close() {}
  };
  globalThis.customElements = { define: function () {}, get: function () { return undefined; }, whenDefined: function () { return Promise.resolve(); } };
  var Stub = function () {
    return class {
      constructor(cb) { this.cb = cb; }
      observe() {}
      unobserve() {}
      disconnect() {}
      takeRecords() { return []; }
    };
  };
  globalThis.MutationObserver = Stub();
  globalThis.IntersectionObserver = Stub();
  globalThis.ResizeObserver = Stub();
  globalThis.PerformanceObserver = Stub();
  globalThis.WeakMap = class WeakMap extends Map {};
  globalThis.WeakSet = class WeakSet extends Set {};
  globalThis.WeakRef = class WeakRef { constructor(t) { this.t = t; } deref() { return this.t; } };
  globalThis.FinalizationRegistry = class FinalizationRegistry { register() {} unregister() {} };
  globalThis.structuredClone = function (v) { return v === undefined ? v : JSON.parse(JSON.stringify(v)); };
  globalThis.AbortController = class AbortController {
    constructor() {
      var s = new EventTarget();
      s.aborted = false;
      s.reason = undefined;
      this.signal = s;
    }
    abort(reason) {
      if (this.signal.aborted) { return; }
      this.signal.aborted = true;
      this.signal.reason = reason;
      var e = new Event('abort');
      e.target = this.signal;
      this.signal.dispatchEvent(e);
    }
  };
  var b64 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
  globalThis.btoa = function (s) {
    s = String(s);
    var out = '';
    for (var i = 0; i < s.length; i += 3) {
      var a = s.charCodeAt(i);
      var b = i + 1 < s.length ? s.charCodeAt(i + 1) : NaN;
      var c = i + 2 < s.length ? s.charCodeAt(i + 2) : NaN;
      out += b64.charAt(a >> 2);
      out += b64.charAt(((a & 3) << 4) | ((isNaN(b) ? 0 : b) >> 4));
      out += isNaN(b) ? '=' : b64.charAt(((b & 15) << 2) | ((isNaN(c) ? 0 : c) >> 6));
      out += isNaN(c) ? '=' : b64.charAt(c & 63);
    }
    return out;
  };
  globalThis.atob = function (s) {
    s = String(s).replace(/=+$/, '');
    var out = '';
    var bits = 0;
    var acc = 0;
    for (var i = 0; i < s.length; i++) {
      var v = b64.indexOf(s.charAt(i));
      if (v < 0) { continue; }
      acc = (acc << 6) | v;
      bits += 6;
      if (bits >= 8) { bits -= 8; out += String.fromCharCode((acc >> bits) & 255); }
    }
    return out;
  };
});

__section('reflect', function () {
  globalThis.Reflect = {
    get: function (t, k) { return t[k]; },
    set: function (t, k, v) { t[k] = v; return true; },
    has: function (t, k) { return k in t; },
    ownKeys: function (t) { return Object.keys(t); },
    deleteProperty: function (t, k) { delete t[k]; return true; },
    getPrototypeOf: function (t) { return Object.getPrototypeOf(t); },
    setPrototypeOf: function (t, p) { Object.setPrototypeOf(t, p); return true; },
    defineProperty: function (t, k, d) { Object.defineProperty(t, k, d); return true; },
    apply: function (f, th, args) { return f.apply(th, args); },
    construct: function (F, args) { return new F(...args); }
  };
  if (!Promise.allSettled) {
    Promise.allSettled = function (list) {
      return Promise.all(Array.from(list).map(function (p) {
        return Promise.resolve(p).then(function (value) { return { status: 'fulfilled', value: value }; }, function (reason) { return { status: 'rejected', reason: reason }; });
      }));
    };
  }
  if (!Promise.any) {
    Promise.any = function (list) {
      return new Promise(function (resolve, reject) {
        var items = Array.from(list);
        var left = items.length;
        if (!left) { reject(new Error('All promises were rejected')); return; }
        items.forEach(function (p) {
          Promise.resolve(p).then(resolve, function () { left--; if (left === 0) { reject(new Error('All promises were rejected')); } });
        });
      });
    };
  }
  if (!Array.prototype.findLast) {
    Array.prototype.findLast = function (fn) { for (var i = this.length - 1; i >= 0; i--) { if (fn(this[i], i, this)) { return this[i]; } } return undefined; };
    Array.prototype.findLastIndex = function (fn) { for (var i = this.length - 1; i >= 0; i--) { if (fn(this[i], i, this)) { return i; } } return -1; };
  }
  globalThis.Intl = {
    NumberFormat: class NumberFormat { constructor() {} format(n) { return String(n); } resolvedOptions() { return { locale: 'tr-TR' }; } },
    DateTimeFormat: class DateTimeFormat { constructor() {} format(d) { return d && d.toISOString ? d.toISOString() : String(d); } resolvedOptions() { return { locale: 'tr-TR', timeZone: 'UTC' }; } },
    PluralRules: class PluralRules { select(n) { return n === 1 ? 'one' : 'other'; } },
    Collator: class Collator { compare(a, b) { return a < b ? -1 : a > b ? 1 : 0; } },
    RelativeTimeFormat: class RelativeTimeFormat { format(v, u) { return v + ' ' + u; } },
    ListFormat: class ListFormat { format(l) { return Array.from(l).join(', '); } }
  };
});

__section('net', function () {
  var H = __host;
  var pending = new Map();
  function parseHeaderText(t) {
    var h = {};
    var lines = String(t || '').split('\n');
    for (var i = 0; i < lines.length; i++) {
      var idx = lines[i].indexOf(':');
      if (idx > 0) { h[lines[i].slice(0, idx).trim().toLowerCase()] = lines[i].slice(idx + 1).trim(); }
    }
    return h;
  }
  globalThis.__net_done = function (id, ok, status, body, url, headerText) {
    var p = pending.get(id);
    if (!p) { return; }
    pending.delete(id);
    if (!ok) { p.reject(new TypeError('Failed to fetch')); return; }
    p.resolve({ status: status, body: body, url: url, headers: parseHeaderText(headerText) });
  };
  function rawFetch(url, method, headers, body) {
    return new Promise(function (resolve, reject) {
      var full;
      try { full = __resolve_url(url, location.href); } catch (e) { reject(new TypeError('Invalid URL')); return; }
      var lines = '';
      for (var k in headers) { lines += k + ': ' + headers[k] + '\n'; }
      var id = H.net(full, method || 'GET', lines, body === undefined || body === null ? '' : String(body));
      pending.set(id, { resolve: resolve, reject: reject });
    });
  }
  class Headers {
    constructor(init) {
      this.m = {};
      if (init instanceof Headers) { init.forEach(function (v, k) { this.m[k] = v; }, this); }
      else if (Array.isArray(init)) { for (var i = 0; i < init.length; i++) { this.m[String(init[i][0]).toLowerCase()] = String(init[i][1]); } }
      else if (init && typeof init === 'object') { for (var k in init) { this.m[String(k).toLowerCase()] = String(init[k]); } }
    }
    get(k) { var v = this.m[String(k).toLowerCase()]; return v === undefined ? null : v; }
    set(k, v) { this.m[String(k).toLowerCase()] = String(v); }
    has(k) { return this.m[String(k).toLowerCase()] !== undefined; }
    append(k, v) { this.set(k, v); }
    delete(k) { delete this.m[String(k).toLowerCase()]; }
    forEach(fn, thisArg) { for (var k in this.m) { fn.call(thisArg, this.m[k], k, this); } }
  }
  class Response {
    constructor(body, init) {
      this.body_ = body === null || body === undefined ? '' : String(body);
      this.status = (init && init.status) || 200;
      this.ok = this.status >= 200 && this.status < 300;
      this.statusText = (init && init.statusText) || '';
      this.url = (init && init.url) || '';
      this.headers = new Headers(init && init.headers);
      this.redirected = false;
      this.type = 'basic';
      this.bodyUsed = false;
    }
    text() { return Promise.resolve(this.body_); }
    json() { try { return Promise.resolve(JSON.parse(this.body_)); } catch (e) { return Promise.reject(e); } }
    clone() { return new Response(this.body_, { status: this.status, url: this.url, headers: this.headers }); }
    arrayBuffer() { return Promise.reject(new Error('arrayBuffer is not supported')); }
    blob() { return Promise.resolve(new Blob([this.body_])); }
  }
  class Request {
    constructor(input, init) {
      this.url = typeof input === 'string' ? input : input.url;
      this.method = (init && init.method) || (input && input.method) || 'GET';
      this.headers = new Headers((init && init.headers) || (input && input.headers));
      this.body = init && init.body;
    }
  }
  function prepare(method, headersIn, bodyIn) {
    var h = {};
    new Headers(headersIn).forEach(function (v, k) { h[k] = v; });
    var body = '';
    if (bodyIn !== undefined && bodyIn !== null) {
      body = String(bodyIn);
      if (!h['content-type']) {
        if (bodyIn instanceof URLSearchParams || bodyIn instanceof FormData) { h['content-type'] = 'application/x-www-form-urlencoded;charset=UTF-8'; }
        else if (typeof bodyIn === 'string') { h['content-type'] = 'text/plain;charset=UTF-8'; }
      }
    }
    if (method !== 'GET' && method !== 'HEAD') {
      if (!h['origin']) { h['origin'] = location.origin; }
      if (!h['referer']) { h['referer'] = location.href; }
    }
    return { headers: h, body: body };
  }
  globalThis.Headers = Headers;
  globalThis.Response = Response;
  globalThis.Request = Request;
  globalThis.fetch = function (input, init) {
    var url = typeof input === 'string' ? input : (input && input.url) || String(input);
    var method = String((init && init.method) || (input && input.method) || 'GET').toUpperCase();
    var p = prepare(method, (init && init.headers) || (input && input.headers), (init && init.body !== undefined) ? init.body : (input && input.body));
    return rawFetch(url, method, p.headers, p.body).then(function (r) {
      return new Response(r.body, { status: r.status, url: r.url, headers: r.headers });
    });
  };
  class XMLHttpRequest extends EventTarget {
    constructor() {
      super();
      this.readyState = 0;
      this.status = 0;
      this.statusText = '';
      this.responseText = '';
      this.response = '';
      this.responseURL = '';
      this.responseType = '';
      this.timeout = 0;
      this.withCredentials = false;
      this.onreadystatechange = null;
      this.onload = null;
      this.onerror = null;
      this.upload = new EventTarget();
      this.reqHeaders_ = {};
      this.resHeaders_ = {};
    }
    open(method, url) { this.method_ = String(method).toUpperCase(); this.url_ = String(url); this.readyState = 1; }
    setRequestHeader(k, v) { this.reqHeaders_[String(k).toLowerCase()] = String(v); }
    getResponseHeader(k) { var v = this.resHeaders_[String(k).toLowerCase()]; return v === undefined ? null : v; }
    getAllResponseHeaders() { var o = ''; for (var k in this.resHeaders_) { o += k + ': ' + this.resHeaders_[k] + '\r\n'; } return o; }
    overrideMimeType() {}
    abort() { this.aborted_ = true; }
    fire(type) {
      var e = new Event(type);
      e.target = this;
      this.dispatchEvent(e);
    }
    send(body) {
      var self2 = this;
      var p = prepare(this.method_, this.reqHeaders_, body);
      rawFetch(this.url_, this.method_, p.headers, p.body).then(function (r) {
        if (self2.aborted_) { return; }
        self2.status = r.status;
        self2.statusText = r.status === 200 ? 'OK' : '';
        self2.responseText = r.body;
        self2.responseURL = r.url;
        self2.resHeaders_ = r.headers || {};
        var parsed = r.body;
        if (self2.responseType === 'json') { try { parsed = JSON.parse(r.body); } catch (x) { parsed = null; } }
        self2.response = parsed;
        self2.readyState = 4;
        self2.fire('readystatechange');
        self2.fire('load');
        self2.fire('loadend');
      }, function () {
        self2.readyState = 4;
        self2.status = 0;
        self2.fire('readystatechange');
        self2.fire('error');
        self2.fire('loadend');
      });
    }
  }
  XMLHttpRequest.UNSENT = 0;
  XMLHttpRequest.OPENED = 1;
  XMLHttpRequest.HEADERS_RECEIVED = 2;
  XMLHttpRequest.LOADING = 3;
  XMLHttpRequest.DONE = 4;
  globalThis.XMLHttpRequest = XMLHttpRequest;
});

__section('typed', function () {
  class ArrayBuffer {
    constructor(n) { this.byteLength = n | 0; }
    slice(a, b) { return new ArrayBuffer(Math.max(0, (b === undefined ? this.byteLength : b) - (a || 0))); }
    static isView(v) { return !!(v && v['@@a'] !== undefined); }
  }
  globalThis.ArrayBuffer = ArrayBuffer;
  globalThis.SharedArrayBuffer = ArrayBuffer;

  function isIndex(k) {
    if (typeof k !== 'string' || k.length === 0) { return false; }
    var c = k.charCodeAt(0);
    return c >= 48 && c <= 57;
  }

  function makeTyped(name, bytes, coerce) {
    var C = class {
      constructor(a, off, len) {
        var arr = [];
        var i;
        if (a === undefined || a === null) { a = 0; }
        if (typeof a === 'number') {
          for (i = 0; i < a; i++) { arr.push(0); }
        } else if (a instanceof ArrayBuffer) {
          var n = len !== undefined ? len : Math.floor((a.byteLength - (off || 0)) / bytes);
          for (i = 0; i < n; i++) { arr.push(0); }
        } else {
          var src = Array.from(a);
          for (i = 0; i < src.length; i++) { arr.push(coerce(src[i])); }
        }
        this['@@a'] = arr;
        this['@@c'] = coerce;
        this['@@n'] = name;
        this.BYTES_PER_ELEMENT = bytes;
        this.byteLength = arr.length * bytes;
        this.byteOffset = off || 0;
        this.buffer = a instanceof ArrayBuffer ? a : new ArrayBuffer(arr.length * bytes);
        return new Proxy(this, {
          get: function (t, k) {
            if (isIndex(k)) { return t['@@a'][+k]; }
            if (k === 'length') { return t['@@a'].length; }
            return t[k];
          },
          set: function (t, k, v) {
            if (isIndex(k)) { var ix = +k; if (ix < t['@@a'].length) { t['@@a'][ix] = coerce(v); } return true; }
            t[k] = v;
            return true;
          },
          has: function (t, k) { return isIndex(k) ? (+k) < t['@@a'].length : k in t; }
        });
      }
      set(src, offset) {
        var s2 = Array.from(src);
        var o = offset || 0;
        for (var i = 0; i < s2.length; i++) { if (o + i < this['@@a'].length) { this['@@a'][o + i] = this['@@c'](s2[i]); } }
      }
      subarray(b, e) { return new C(this['@@a'].slice(b, e)); }
      slice(b, e) { return new C(this['@@a'].slice(b, e)); }
      fill(v, b, e) { var c = this['@@c']; var a = this['@@a']; for (var i = b || 0; i < (e === undefined ? a.length : e); i++) { a[i] = c(v); } return this; }
      map(f) { var self4 = this; return new C(this['@@a'].map(function (x, i) { return f(x, i, self4); })); }
      filter(f) { return new C(this['@@a'].filter(f)); }
      forEach(f) { var self4 = this; this['@@a'].forEach(function (x, i) { f(x, i, self4); }); }
      reduce(f, init) { return arguments.length > 1 ? this['@@a'].reduce(f, init) : this['@@a'].reduce(f); }
      indexOf(v) { return this['@@a'].indexOf(v); }
      includes(v) { return this['@@a'].indexOf(v) >= 0; }
      join(sep) { return this['@@a'].join(sep); }
      find(f) { return this['@@a'].find(f); }
      findIndex(f) { return this['@@a'].findIndex(f); }
      some(f) { return this['@@a'].some(f); }
      every(f) { return this['@@a'].every(f); }
      reverse() { this['@@a'].reverse(); return this; }
      sort(f) { this['@@a'].sort(f || function (a, b) { return a - b; }); return this; }
      toString() { return this['@@a'].join(','); }
    }
    C.prototype['@@iterator'] = function () {
      var i = 0;
      var a = this['@@a'];
      return { next: function () { if (i < a.length) { var v = a[i]; i++; return { value: v, done: false }; } return { value: undefined, done: true }; } };
    };
    C.BYTES_PER_ELEMENT = bytes;
    C.from = function (src, f) { var arr = Array.from(src); return new C(f ? arr.map(f) : arr); };
    C.of = function () { return new C(Array.prototype.slice.call(arguments)); };
    globalThis[name] = C;
  }
  makeTyped('Uint8Array', 1, function (v) { return (v | 0) & 255; });
  makeTyped('Int8Array', 1, function (v) { return ((v | 0) << 24) >> 24; });
  makeTyped('Uint8ClampedArray', 1, function (v) { v = Number(v); return v !== v ? 0 : v < 0 ? 0 : v > 255 ? 255 : Math.round(v); });
  makeTyped('Uint16Array', 2, function (v) { return (v | 0) & 65535; });
  makeTyped('Int16Array', 2, function (v) { return ((v | 0) << 16) >> 16; });
  makeTyped('Uint32Array', 4, function (v) { return v >>> 0; });
  makeTyped('Int32Array', 4, function (v) { return v | 0; });
  makeTyped('Float32Array', 4, function (v) { return +v; });
  makeTyped('Float64Array', 8, function (v) { return +v; });

  class DataView {
    constructor(buf) { this.buffer = buf; this.bytes = []; for (var i = 0; i < buf.byteLength; i++) { this.bytes.push(0); } this.byteLength = buf.byteLength; }
    getUint8(i) { return this.bytes[i] || 0; }
    setUint8(i, v) { this.bytes[i] = v & 255; }
    getInt8(i) { return (this.getUint8(i) << 24) >> 24; }
    getUint16(i, le) { var a = this.getUint8(i), b = this.getUint8(i + 1); return le ? (b << 8) | a : (a << 8) | b; }
    setUint16(i, v, le) { if (le) { this.setUint8(i, v); this.setUint8(i + 1, v >> 8); } else { this.setUint8(i, v >> 8); this.setUint8(i + 1, v); } }
    getUint32(i, le) { var a = this.getUint16(i, le), b = this.getUint16(i + 2, le); return (le ? (b * 65536 + a) : (a * 65536 + b)) >>> 0; }
    setUint32(i, v, le) { if (le) { this.setUint16(i, v & 65535, true); this.setUint16(i + 2, v >>> 16, true); } else { this.setUint16(i, v >>> 16, false); this.setUint16(i + 2, v & 65535, false); } }
    getInt32(i, le) { return this.getUint32(i, le) | 0; }
  }
  globalThis.DataView = DataView;

  globalThis.TextEncoder = class TextEncoder {
    get encoding() { return 'utf-8'; }
    encode(str) {
      var e = encodeURIComponent(String(str === undefined ? '' : str));
      var out = [];
      for (var i = 0; i < e.length; i++) {
        if (e.charAt(i) === '%') { out.push(parseInt(e.substr(i + 1, 2), 16)); i += 2; } else { out.push(e.charCodeAt(i)); }
      }
      return new Uint8Array(out);
    }
  };
  globalThis.TextDecoder = class TextDecoder {
    get encoding() { return 'utf-8'; }
    decode(buf) {
      if (!buf) { return ''; }
      var a = Array.from(buf);
      var s2 = '';
      for (var i = 0; i < a.length; i++) { s2 += '%' + (a[i] < 16 ? '0' : '') + a[i].toString(16); }
      try { return decodeURIComponent(s2); } catch (e) { return String.fromCharCode.apply(null, a); }
    }
  };
  globalThis.crypto = {
    getRandomValues: function (arr) {
      var big = arr.BYTES_PER_ELEMENT >= 4;
      for (var i = 0; i < arr.length; i++) { arr[i] = Math.floor(Math.random() * (big ? 4294967296 : 256)); }
      return arr;
    },
    randomUUID: function () {
      return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, function (c) {
        var r = Math.floor(Math.random() * 16);
        return (c === 'x' ? r : (r & 3) | 8).toString(16);
      });
    },
    subtle: undefined
  };
  globalThis.Blob = class Blob {
    constructor(parts, opts) {
      this.parts = parts || [];
      this.type = (opts && opts.type) || '';
      var n = 0;
      for (var i = 0; i < this.parts.length; i++) { n += String(this.parts[i]).length; }
      this.size = n;
    }
    text() { return Promise.resolve(this.parts.map(String).join('')); }
    slice() { return this; }
  };
  globalThis.File = class File extends Blob {
    constructor(parts, name, opts) { super(parts, opts); this.name = String(name); this.lastModified = 0; }
  };
  globalThis.FileReader = class FileReader extends EventTarget {
    readAsText(b) {
      var me = this;
      b.text().then(function (t) { me.result = t; var e = new Event('load'); e.target = me; me.dispatchEvent(e); if (typeof me.onload === 'function') { me.onload(e); } });
    }
  };
  globalThis.FormData = class FormData {
    constructor() { this.list = []; }
    append(k, v) { this.list.push([String(k), String(v)]); }
    set(k, v) { this.delete(k); this.append(k, v); }
    delete(k) { this.list = this.list.filter(function (p) { return p[0] !== k; }); }
    get(k) { for (var i = 0; i < this.list.length; i++) { if (this.list[i][0] === k) { return this.list[i][1]; } } return null; }
    has(k) { return this.get(k) !== null; }
    forEach(fn) { for (var i = 0; i < this.list.length; i++) { fn(this.list[i][1], this.list[i][0], this); } }
    toString() { return new URLSearchParams(this.list.reduce(function (o, p) { o[p[0]] = p[1]; return o; }, {})).toString(); }
  };
});
