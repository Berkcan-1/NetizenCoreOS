// Node ile prelude.js testleri: nc-js/src/tests.rs icindeki `dom_prelude` vakalarini AYNEN okur,
// dom_mock.js + prelude.js uzerinde calistirir. Cargo gerektirmez; gercek V8 uzerinde mantigi dogrular.
// (nc-js yorumlayicisinin kendisini test etmez: onun icin `cd nc-js && cargo test`.)
// Kullanim:  node run_prelude_tests.js   (nc-app-browser/host-tests/js icinden)
const fs = require('fs');
const path = require('path');
const vm = require('vm');

const root = path.resolve(__dirname, '../../..');
const mock = fs.readFileSync(path.join(root, 'nc-js/src/dom_mock.js'), 'utf8');
const prelude = fs.readFileSync(path.join(root, 'nc-app-browser/src/prelude.js'), 'utf8');
const rust = fs.readFileSync(path.join(root, 'nc-js/src/tests.rs'), 'utf8');

function parseCases(src) {
  const start = src.indexOf('fn dom_prelude()');
  const body = src.slice(start);
  const re = /\(r##"([\s\S]*?)"##, r##"([\s\S]*?)"##, r##"([\s\S]*?)"##\),/g;
  const out = [];
  let m;
  while ((m = re.exec(body))) out.push([m[1], m[2], m[3]]);
  return out;
}

// Zamanlayicilar elle surulur: Rust tarafinda Interp::run_timers'in yaptigi isin karsiligi.
function makeContext() {
  const timers = [];
  const ctx = vm.createContext({
    console: { log() {}, error() {}, warn() {} },
    queueMicrotask,
    setTimeout: (fn, ms) => { timers.push(fn); return timers.length; },
    clearTimeout: () => {},
    setInterval: () => 0,
    clearInterval: () => {},
  });
  ctx.__flushTimers = () => { while (timers.length) timers.shift()(); };
  vm.runInContext(mock, ctx);
  vm.runInContext(prelude, ctx);
  return ctx;
}
const tick = () => new Promise((r) => setImmediate(r));

async function runCase(setup, check) {
  const ctx = makeContext();
  const errs = vm.runInContext('__prelude_errors.join("; ")', ctx);
  if (errs) return 'PRELUDE SECTION ERRORS: ' + errs;
  let got;
  try { got = String(vm.runInContext(setup, ctx)); } catch (e) { got = 'ERR ' + e.message; }
  await tick(); await tick();
  if (check) {
    try { got = String(vm.runInContext(check, ctx)); } catch (e) { got = 'ERR ' + e.message; }
  }
  return got;
}

// Zamanlayici gerektiren (asenkron olay) vakalar: yalnizca burada, cunku Rust vaka tablosu run_timers cagirmaz.
const extra = [
  ['var log=[]; window.addEventListener("popstate", function(e){ log.push("pop:" + JSON.stringify(e.state)); }); history.pushState({n:1},"","/p1"); history.pushState({n:2},"","/p2"); history.back(); log.push("sync"); __flushTimers(); 0',
   'log.join("|")', 'sync|pop:{"n":1}'],
  ['var log=[]; window.addEventListener("hashchange", function(e){ log.push(e.oldURL.slice(-2) + ">" + e.newURL.slice(-4)); }); location.hash="top"; __flushTimers(); 0',
   'log.join("|")', '#h>#top'],
  ['var log=[]; window.addEventListener("popstate", function(e){ log.push("pop"); }); history.back(); __flushTimers(); 0',
   'log.length + ":" + location.href', '0:https://example.com/a/b?x=1#h'],
  ['var log=[]; var c=new MessageChannel(); c.port2.onmessage=function(e){ log.push(e.data.v); }; c.port1.postMessage({v:7}); log.push("sync"); __flushTimers(); 0',
   'log.join()', 'sync,7'],
  ['var n=0; var s=AbortSignal.timeout(5); s.addEventListener("abort", function(){ n++; }); __flushTimers(); 0',
   'n + "," + s.aborted + "," + s.reason.name', '1,true,TimeoutError'],
  ['var n=0; var c=new AbortController(); c.signal.onabort=function(){ n++; }; c.abort(); c.abort(); 0', 'n', '1'],
  // go(0)/forward() disinda kalan hedefler (bu belgenin disi) sessizce yok sayilir, URL degismez.
];

// nc-js'e ozgu yineleyici protokolune dayanan vakalar: V8'de calismaz (ORIJINAL prelude'de de ayni),
// bunlari yalnizca `cd nc-js && cargo test` dogrular. Burada atlanir ve sayilir.
const NC_JS_ONLY = ['new Int16Array(3)', 'new URLSearchParams(\'a=1&b=%20x\')'];

(async () => {
  const cases = parseCases(rust);
  let failed = 0;
  let skipped = 0;
  for (const [setup, check, expected] of cases) {
    if (NC_JS_ONLY.some((k) => setup.includes(k))) { skipped++; continue; }
    const got = await runCase(setup, check);
    if (got !== expected) { failed++; console.log('FAIL:', setup.slice(0, 110), '\n  beklenen:', expected, '\n  gelen:   ', got); }
  }
  for (const [setup, check, expected] of extra) {
    const got = await runCase(setup, check);
    if (got !== expected) { failed++; console.log('FAIL(extra):', setup.slice(0, 110), '\n  beklenen:', expected, '\n  gelen:   ', got); }
  }
  console.log(`prelude.js (Node/V8): ${cases.length + extra.length - skipped} vaka calisti, ${failed} basarisiz, ${skipped} nc-js'e ozgu vaka atlandi`);
  process.exit(failed ? 1 : 0);
})();
