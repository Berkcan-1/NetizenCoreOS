// Mock of the native __dom / __host primitives, used only by the tests.
var __nodes = [{ type: 9, tag: 'document', attrs: {}, kids: [], parent: -1, text: '' }];
function __mk(type, tag, text) { __nodes.push({ type: type, tag: tag, attrs: {}, kids: [], parent: -1, text: text || '' }); return __nodes.length - 1; }
function __detach(id) {
  var p = __nodes[id].parent;
  if (p >= 0) { __nodes[p].kids = __nodes[p].kids.filter(function (k) { return k !== id; }); }
  __nodes[id].parent = -1;
}
var __html = __mk(1, 'html');
var __body = __mk(1, 'body');
var __head = __mk(1, 'head');
__nodes[0].kids.push(__html); __nodes[__html].parent = 0;
__nodes[__html].kids.push(__head); __nodes[__head].parent = __html;
__nodes[__html].kids.push(__body); __nodes[__body].parent = __html;
function __descendants(id, out) {
  var kids = __nodes[id].kids;
  for (var i = 0; i < kids.length; i++) { out.push(kids[i]); __descendants(kids[i], out); }
  return out;
}
function __textOf(id) {
  if (__nodes[id].type === 3) { return __nodes[id].text; }
  return __nodes[id].kids.map(__textOf).join('');
}
function __hasClass(id, c) { return (__nodes[id].attrs['class'] || '').split(/\s+/).indexOf(c) >= 0; }
var __queue = [];
var __href = 'https://example.com/a/b?x=1#h';
var __dom = {
  type: function (id) { return __nodes[id].type; },
  tag: function (id) { return __nodes[id].tag; },
  create: function (t) { return __mk(1, t); },
  text: function (s) { return __mk(3, '', s); },
  value: function (id) { return __nodes[id].text; },
  setValue: function (id, s) { __nodes[id].text = s; },
  parent: function (id) { return __nodes[id].parent; },
  children: function (id) { return __nodes[id].kids.slice(); },
  append: function (p, c) { __detach(c); __nodes[p].kids.push(c); __nodes[c].parent = p; return true; },
  insertBefore: function (p, c, r) {
    __detach(c);
    var i = __nodes[p].kids.indexOf(r);
    if (i < 0) { __nodes[p].kids.push(c); } else { __nodes[p].kids.splice(i, 0, c); }
    __nodes[c].parent = p;
    return true;
  },
  remove: function (id) { __detach(id); },
  getAttr: function (id, n) { return n in __nodes[id].attrs ? __nodes[id].attrs[n] : null; },
  setAttr: function (id, n, v) { __nodes[id].attrs[n] = v; },
  removeAttr: function (id, n) { delete __nodes[id].attrs[n]; },
  attrNames: function (id) { return Object.keys(__nodes[id].attrs); },
  textContent: __textOf,
  setTextContent: function (id, s) {
    __nodes[id].kids.forEach(function (k) { __nodes[k].parent = -1; });
    __nodes[id].kids = [];
    if (s) { var t = __mk(3, '', s); __nodes[id].kids.push(t); __nodes[t].parent = id; }
  },
  innerHTML: function () { return '<mock>'; },
  setInnerHTML: function () {},
  outerHTML: function () { return '<mock/>'; },
  clone: function (id) { return __mk(__nodes[id].type, __nodes[id].tag, __nodes[id].text); },
  query: function (root, sel, all) {
    var m = __descendants(root, []).filter(function (i) {
      if (__nodes[i].type !== 1) { return false; }
      if (sel.charAt(0) === '.') { return __hasClass(i, sel.slice(1)); }
      if (sel.charAt(0) === '#') { return __nodes[i].attrs.id === sel.slice(1); }
      return __nodes[i].tag === sel;
    });
    return all ? m : m.slice(0, 1);
  },
  matches: function (id, sel) { return sel.charAt(0) === '.' ? __hasClass(id, sel.slice(1)) : __nodes[id].tag === sel; },
  byId: function (s) {
    var m = __descendants(0, []).filter(function (i) { return __nodes[i].attrs.id === s; });
    return m.length ? m[0] : -1;
  },
  connected: function (id) {
    var c = id;
    while (c >= 0) { if (c === 0) { return true; } c = __nodes[c].parent; }
    return false;
  }
};
var __host = {
  time: function () { return 123; },
  error: function (e) {},
  location: function () { return __href; },
  navigate: function (u) { __href = u; },
  userAgent: function () { return 'UA'; },
  net: function (u, m, h, b) { __queue.push([u, m, h, b]); return __queue.length; },
  getCookie: function () { return __jar.join('; '); },
  setCookie: function (v) {
    var s = String(v).split(';')[0];
    var n = s.split('=')[0];
    for (var i = __jar.length - 1; i >= 0; i--) { if (__jar[i].split('=')[0] === n) { __jar.splice(i, 1); } }
    __jar.push(s);
  }
};
var __jar = [];
