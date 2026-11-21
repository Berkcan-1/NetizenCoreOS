// compat_probe.html'deki betigi Node'da prelude.js + dom_mock.js uzerinde calistirir.
// Amac: (1) probe'un kendisinin hatasiz calistigini dogrulamak, (2) prelude'un DOM/Web API eksiklerini listelemek.
// ES yerlesikleri burada Node'un oldugu icin hep "var" gorunur: o kisim yalnizca nc-js icinde (about:compat) anlamlidir.
const fs = require('fs'), path = require('path'), vm = require('vm');
const root = path.resolve(__dirname, '../../..');
const html = fs.readFileSync(path.join(root, 'nc-app-browser/src/compat_probe.html'), 'utf8');
const script = html.slice(html.indexOf('<script>') + 8, html.lastIndexOf('</script>'));
const timers = [];
const ctx = vm.createContext({ console: { log() {}, error() {}, warn() {} }, queueMicrotask, setTimeout: (f) => timers.push(f), clearTimeout() {}, setInterval: () => 0, clearInterval() {} });
vm.runInContext(fs.readFileSync(path.join(root, 'nc-js/src/dom_mock.js'), 'utf8'), ctx);
vm.runInContext(fs.readFileSync(path.join(root, 'nc-app-browser/src/prelude.js'), 'utf8'), ctx);
const errs = vm.runInContext('__prelude_errors.join("; ")', ctx);
if (errs) console.log('PRELUDE SECTION ERRORS:', errs);
vm.runInContext(script, ctx);
const r = vm.runInContext('window.__compat', ctx);
if (!r) { console.log('probe produced no result'); process.exit(1); }
console.log(`PASS ${r.total - r.missing - (r.untested || 0)} / ${r.total} · MISSING ${r.missing} · NOT TESTED ${r.untested || 0}`);
for (const c of r.categories) { if (c.missing.length) console.log(`[${c.name}] MISSING: ${c.missing.join(', ')}`); if (c.untested && c.untested.length) console.log(`[${c.name}] NOT TESTED: ${c.untested.join(', ')}`); }

// Function kurucusunun bulunmadigi nc-js ortaminda syntax testleri false-negative olmamali.
const noFunction = vm.createContext({
  console: { log() {}, error() {}, warn() {} },
  queueMicrotask, setTimeout: (f) => timers.push(f), clearTimeout() {},
  setInterval: () => 0, clearInterval() {}, Function: undefined
});
vm.runInContext(fs.readFileSync(path.join(root, 'nc-js/src/dom_mock.js'), 'utf8'), noFunction);
vm.runInContext(fs.readFileSync(path.join(root, 'nc-app-browser/src/prelude.js'), 'utf8'), noFunction);
vm.runInContext(script, noFunction);
const noFunctionResult = vm.runInContext('window.__compat', noFunction);
const syntax = noFunctionResult && noFunctionResult.categories.find(c => c.name === 'JS syntax');
if (!syntax || syntax.untested.length === 0 || syntax.missing.length !== 0) {
  console.error('FAIL syntax probe must report dynamic-compiler cases as NOT TESTED when Function is unavailable');
  process.exit(1);
}
console.log(`PASS syntax unavailable-state probe · ${syntax.untested.length} NOT TESTED · ${syntax.missing.length} incorrectly MISSING`);
