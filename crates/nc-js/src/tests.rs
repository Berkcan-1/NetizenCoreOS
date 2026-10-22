use crate::Interp;

fn ev(src: &str) -> String {
    let mut it = Interp::new();
    match it.run(src) {
        Ok(v) => match it.to_str(&v) {
            Ok(s) => s,
            Err(_) => String::from("<tostring error>"),
        },
        Err(e) => format!("ERR {}", e),
    }
}

fn big<F: FnOnce() + Send + 'static>(f: F) {
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn conformance() {
    big(|| {
        let cases: Vec<(&str, &str)> = vec![
            ("1 + 2 * 3", "7"),
            ("(1 + 2) * 3 % 5", "4"),
            ("2 ** 10", "1024"),
            ("'a' + 1 + 2", "a12"),
            ("1 + 2 + 'a'", "3a"),
            ("var x = 5; x += 3; x", "8"),
            ("let s = 0; for (let i = 0; i < 10; i++) { s += i } s", "45"),
            ("var f = []; for (let i = 0; i < 3; i++) { f.push(() => i) } f.map(g => g()).join()", "0,1,2"),
            ("var f = []; for (var i = 0; i < 3; i++) { f.push(() => i) } f.map(g => g()).join()", "3,3,3"),
            ("function fib(n) { return n < 2 ? n : fib(n-1) + fib(n-2) } fib(15)", "610"),
            ("const add = (a, b = 10) => a + b; add(1) + add(1, 2)", "14"),
            ("function f(...r) { return r.length } f(1,2,3,4)", "4"),
            ("[1,2,3].map(x => x * 2).join('-')", "2-4-6"),
            ("[3,1,2].sort().join()", "1,2,3"),
            ("[10,9,1].sort().join()", "1,10,9"),
            ("[10,9,1].sort((a,b) => a - b).join()", "1,9,10"),
            ("[1,2,3,4].filter(x => x % 2 == 0).reduce((a,b) => a + b, 0)", "6"),
            ("var o = {a: 1, b: {c: [1,2,3]}}; o.b.c[1] + o.a", "3"),
            ("var {a, b: {c}} = {a: 1, b: {c: 7}}; a + c", "8"),
            ("var [x, , y, ...z] = [1,2,3,4,5]; x + y + z.length", "6"),
            ("`a${1+1}b${'c'}`", "a2bc"),
            ("typeof undefinedVar", "undefined"),
            ("typeof null + typeof 1 + typeof 'a' + typeof {} + typeof (() => 1)", "objectnumberstringobjectfunction"),
            ("null ?? 'd'", "d"),
            ("0 ?? 'd'", "0"),
            ("var o = null; o?.x?.y", "undefined"),
            ("var o = {f() { return 5 }}; o.f?.() + o.g?.()", "NaN"),
            ("'abc'.toUpperCase() + 'ABC'.toLowerCase()", "ABCabc"),
            ("'a,b,c'.split(',').length", "3"),
            ("'hello world'.indexOf('o') + 'hello'.slice(1, 3)", "4el"),
            ("'  x '.trim() + '5'.padStart(3, '0')", "x005"),
            ("'abcabc'.replace('b', 'X') + 'abcabc'.replaceAll('b', 'Y')", "aXcabcaYcaYc"),
            ("parseInt('42px') + parseFloat('3.5e1x')", "77"),
            ("(255).toString(16) + (3.14159).toFixed(2)", "ff3.14"),
            ("Math.max(1, 5, 3) + Math.min(4, 2) + Math.floor(2.7) + Math.round(2.5)", "12"),
            ("JSON.stringify({a: [1, 2, {b: null}], c: 'x', d: undefined})", r#"{"a":[1,2,{"b":null}],"c":"x"}"#),
            ("JSON.stringify({a: 1, b: [1, 2]}, null, 2)", r#"{
      "a": 1,
      "b": [
        1,
        2
      ]
    }"#),
            (r#"JSON.parse('{"a": [1, 2, {"b": true}], "c": "s"}').a[2].b"#, "true"),
            ("try { null.x } catch (e) { e instanceof TypeError }", "true"),
            ("try { throw new Error('boom') } catch (e) { e.message } finally { 1 }", "boom"),
            ("function f() { try { return 1 } finally { 2 } } f()", "1"),
            ("var r = ''; outer: for (var i = 0; i < 3; i++) { for (var j = 0; j < 3; j++) { if (j == 1) continue outer; if (i == 2) break outer; r += i + '' + j + ' ' } } r", "00 10 "),
            ("var r = ''; switch (2) { case 1: r += 'a'; case 2: r += 'b'; case 3: r += 'c'; break; default: r += 'd' } r", "bc"),
            ("var r = []; for (var k in {x: 1, y: 2}) r.push(k); r.join()", "x,y"),
            ("var r = 0; for (var v of [1, 2, 3]) r += v; r", "6"),
            ("var m = new Map(); m.set('a', 1).set('b', 2); m.get('b') + m.size", "4"),
            ("var s = new Set([1, 2, 2, 3]); s.size + (s.has(2) ? 10 : 0)", "13"),
            ("function Person(n) { this.n = n } Person.prototype.hi = function() { return 'hi ' + this.n }; new Person('bob').hi()", "hi bob"),
            ("var o = {x: 1}; var p = Object.create(o); p.x", "1"),
            ("Object.keys({a: 1, b: 2}).join() + Object.values({a: 1, b: 2}).join()", "a,b1,2"),
            ("Object.entries({a: 1}).map(([k, v]) => k + v).join()", "a1"),
            ("var a = {...{x: 1}, y: 2}; a.x + a.y", "3"),
            ("Math.max(...[1, 9, 3])", "9"),
            ("[1, [2, [3]]].flat(2).length", "3"),
            ("(function() { return arguments.length })(1, 2, 3)", "3"),
            ("var c = (function() { var n = 0; return function() { return ++n } })(); c(); c(); c()", "3"),
            ("function f() { return this.v } f.call({v: 7}) + f.apply({v: 1}, []) + f.bind({v: 2})()", "10"),
            ("var x = 10; { let x = 20; } x", "10"),
            ("hoisted(); function hoisted() { return 'ok' }", "ok"),
            ("var i = 0; do { i++ } while (i < 5); i", "5"),
            ("[1,2,3].indexOf(2) + [1,2,3].includes(3)", "2"),
            ("var a = [1,2,3,4,5]; a.splice(1, 2); a.join()", "1,4,5"),
            ("var a = [1,2,3]; a.length = 1; a.join() + a.length", "11"),
            ("var a = []; a[3] = 1; a.length", "4"),
            ("1 / 0 + ' ' + -1 / 0 + ' ' + 0 / 0", "Infinity -Infinity NaN"),
            ("5 & 3 | 8 ^ 1", "9"),
            ("1 << 3 | -16 >> 2", "-4"),
            ("-1 >>> 28", "15"),
            ("'5' * '2' + ('3' - 1)", "12"),
            ("1 == '1' && null == undefined && null !== undefined", "true"),
            ("[] + [] + ({} + '')", "[object Object]"),
            ("var t = 0; var id = setTimeout(function() { t = 1 }, 10); typeof id", "object"),
            ("Number('12') + Number('') + Number('x')", "NaN"),
            ("isNaN('abc') + ' ' + isFinite('12')", "true true"),
            ("encodeURIComponent('a b&c/d') + decodeURIComponent('%41%20')", "a%20b%26c%2FdA "),
            ("new Date(0).toISOString()", "1970-01-01T00:00:00.000Z"),
            ("new Date(86400000 * 365).getFullYear()", "1971"),
            ("Array.from({length: 3}, (_, i) => i * 2).join()", "0,2,4"),
            ("Array.from('abc').join('-')", "a-b-c"),
            ("Array.isArray([]) + ' ' + Array.isArray({})", "true false"),
            ("var o = {a: 1}; delete o.a; 'a' in o", "false"),
            ("'b' in {b: 1}", "true"),
            ("var s = 0; var i = 10; while (i --> 0) s += i; s", "45"),
            ("String(123) + String(true) + String(null) + String([1, [2, 3]])", "123truenull1,2,3"),
            ("(() => { try { throw 1 } catch { return 'c' } })()", "c"),
            ("var x = 1; x ||= 5; x &&= 7; var y = null; y ??= 3; x + y", "10"),
            ("[...'ab', ...[1, 2]].join('')", "ab12"),
            ("var o = {['k' + 1]: 5}; o.k1", "5"),
            ("var f = function fact(n) { return n <= 1 ? 1 : n * fact(n - 1) }; f(6)", "720"),
            ("Number.isInteger(5) + ' ' + Number.isInteger(5.5)", "true false"),
            ("(0.1 + 0.2).toFixed(2) + ' ' + 0.1 * 3", "0.30 0.30000000000000004"),
            ("1e3 + 0x10 + 0b11 + 0o7", "1026"),
            ("'x'.repeat(3) + 'abc'.at(-1) + 'abc'.charAt(1) + 'abc'.charCodeAt(0)", "xxxcb97"),
            ("[5, 1, 4].reverse().join() + [1, 2, 3].slice(-2).join()", "4,1,52,3"),
            ("var a = [1, 2, 3]; a.some(x => x > 2) + ' ' + a.every(x => x > 2) + ' ' + a.find(x => x > 1) + ' ' + a.findIndex(x => x > 1)", "true false 2 1"),
            ("class A { constructor(x) { this.x = x } get double() { return this.x * 2 } static make() { return new A(4) } } A.make().double", "8"),
            ("class A { a = 1; #p = 5; getP() { return this.#p } } new A().a + new A().getP()", "6"),
            ("class A { hi() { return 'A' } } class B extends A { hi() { return super.hi() + 'B' } } new B().hi()", "AB"),
            ("class A { constructor() { this.v = 1 } } class B extends A { constructor() { super(); this.v += 1 } } new B().v", "2"),
            ("class A {} class B extends A {} new B() instanceof A", "true"),
            ("class E extends Error { constructor(m) { super(m); this.name = 'E' } } try { throw new E('bad') } catch (e) { e.name + ':' + e.message + ':' + (e instanceof Error) }", "E:bad:true"),
            ("class S { static n = 3; static inc() { return ++S.n } } S.inc() + S.n", "8"),
            ("var o = { _v: 1, get v() { return this._v + 1 }, set v(x) { this._v = x } }; o.v = 10; o.v", "11"),
            ("Object.defineProperty({}, 'x', { get() { return 42 } }).x", "42"),
            ("function* g() { yield 1; yield 2; return 3 } [...g()].join()", "1,2"),
            ("function* g() { yield* [1,2]; yield 3 } var r = []; for (var v of g()) r.push(v); r.join()", "1,2,3"),
            ("function* g() { yield 1; return 9 } var it = g(); it.next().value + it.next().value + ' ' + it.next().done", "10 true"),
            ("function tag(s, a, b) { return s.join('|') + a + b } tag`x${1}y${2}z`", "x|y|z12"),
            (r#"'abc123def'.match(/\d+/)[0]"#, "123"),
            (r#"/(\d+)-(\d+)/.exec('10-20')[2]"#, "20"),
            (r#"'a1b22c333'.match(/\d+/g).join()"#, "1,22,333"),
            (r#"'John Smith'.replace(/(\w+)\s(\w+)/, '$2, $1')"#, "Smith, John"),
            ("'aaa'.replace(/a/g, 'b')", "bbb"),
            (r#"'a, b;c'.split(/[,;]\s*/).join('|')"#, "a|b|c"),
            (r#"/^[\w.+-]+@[\w-]+\.[\w.]+$/.test('foo.bar@example.com')"#, "true"),
            ("'Hello World'.replace(/o/g, (m, off) => off)", "Hell4 W7rld"),
            (r#"/(?<y>\d{4})-(?<m>\d{2})/.exec('2024-05').groups.m"#, "05"),
            ("'x'.replace(/(?=x)/, '>')", ">x"),
            (r#"/a.c/s.test('a\nc') + ' ' + /a.c/.test('a\nc')"#, "true false"),
            ("'AbC'.replace(/b/i, 'X')", "AXC"),
            ("var r = /o/g; var out = []; var m; while ((m = r.exec('foo boo')) !== null) out.push(m.index); out.join()", "1,2,5,6"),
            (r#"'one  two three'.split(/\s+/).length"#, "3"),
            ("'abcabc'.search(/c/)", "2"),
            (r#"[...'a1b2'.matchAll(/\d/g)].map(m => m[0] + m.index).join()"#, "11,23"),
            (r#"'2020-01-02'.replace(/(\d+)-(\d+)-(\d+)/, '$3/$2/$1')"#, "02/01/2020"),
            (r#"/\bfoo\b/.test('a foo b') + ' ' + /\bfoo\b/.test('afoob')"#, "true false"),
            ("'aBc'.replace(/[a-z]/g, c => c.toUpperCase())", "ABC"),
            (r#"/^(?:\+|-)?\d+$/.test('-12')"#, "true"),
            (r#"/(\w)\1/.test('hello') + ' ' + /(\w)\1/.test('helo')"#, "true false"),
            (r#"'a-b_c'.replace(/[-_](\w)/g, (_, c) => c.toUpperCase())"#, "aBC"),
            ("/[^abc]/.test('abc') + ' ' + /[^abc]/.test('abcd')", "false true"),
            ("/x*/.exec('')[0].length", "0"),
            ("'abc'.split(/(b)/).join('|')", "a|b|c"),
            (r#"/(?<=\$)\d+/.exec('cost $45')[0]"#, "45"),
            (r#"/\d{2,3}/.exec('1 2345')[0]"#, "234"),
            ("/a{2}/.test('caab')", "true"),
            ("'The Quick'.replace(/quick/i, m => m.toLowerCase())", "The quick"),
            ("class A { get x() { return 1 } } Object.keys(new A()).length", "0"),
            ("class A { static #c = 0; static inc() { return ++A.#c } } A.inc()", "1"),
            ("var re = /a(b)?/; var m = re.exec('ac'); m[0] + ':' + m[1] + ':' + m.length", "a:undefined:2"),
            ("'x-y-z'.split('-', 2).join()", "x,y"),
            (r#"'path/to/file.txt'.replace(/^.*\//, '')"#, "file.txt"),
            (r#"'  a  b  '.replace(/\s+/g, ' ').trim()"#, "a b"),
            ("String(/ab+c/gi)", "/ab+c/gi"),
            ("var p = new Proxy({}, { get: (t, k) => k + '!' }); p.foo", "foo!"),
            ("var log = []; var p = new Proxy({}, { set(t, k, v) { log.push(k + v); t[k] = v; return true } }); p.a = 1; log.join() + p.a", "a11"),
            ("var p = new Proxy({x: 1}, {}); p.y = 2; p.x + p.y", "3"),
            ("'k' in new Proxy({}, { has: () => true })", "true"),
            ("var a, b; [a, b] = [1, 2]; a + ',' + b", "1,2"),
            ("var a = 1, b = 2; [a, b] = [b, a]; a + ',' + b", "2,1"),
            ("var x, y; ({ a: x, b: y = 5 } = { a: 1 }); x + ',' + y", "1,5"),
            ("var o; (function(){ var p, q; ({ p, q } = { p: 'P', q: 'Q' }); o = p + q; })(); o", "PQ"),
            ("var r; var k = ({ z: r } = { z: 9 }); r + ',' + typeof k", "9,object"),
            ("var arr = [1, 2, 3]; var ob = { a: 1 }; arr.length + ob.a", "4"),
            ("var a2 = []; a2[0] = 1; a2.length", "1"),
            ("var f = a => { ({ data: a } = a); return a; }; f({ data: 4 })", "4"),
            ("var a3; [a3 = 3] = []; a3", "3"),
            ("var v; [...v] = [1, 2]; v.length", "2"),
            ("function F(){ this.nt = (new.target === F); } new F().nt + ',' + (function(){ return new.target; })()", "true,undefined"),
            ("class A { constructor(){ this.n = new.target.name; } } class B extends A {} new A().n + ',' + new B().n", "A,B"),
            ("function G(){ var f = () => new.target; this.ok = f() === G; } new G().ok", "true"),
            ("function H(){ this.v = (function(){ return new.target; })(); } String(new H().v)", "undefined"),
        ];
        let mut failed = 0;
        for (src, expected) in cases {
            let got = ev(src);
            if got != expected {
                failed += 1;
                println!("FAIL: {}\n  expected: {}\n  got:      {}", src, expected, got);
            }
        }
        assert_eq!(failed, 0, "{} conformance case(s) failed", failed);
    });
}

#[test]
fn promises_and_timers() {
    big(|| {
        let mut it = Interp::new();
        it.run("var log = []; Promise.resolve(1).then(v => log.push('p' + v)); setTimeout(() => log.push('t'), 5); log.push('sync');").unwrap();
        it.run_timers(10.0, 10);
        let v = it.run("log.join()").unwrap();
        assert_eq!(it.to_str(&v).ok().unwrap(), "sync,p1,t");
    });
}

#[test]
fn async_await() {
    big(|| {
        let mut it = Interp::new();
        let v = it.run("var out = 0; async function f(x) { var a = await Promise.resolve(x); var b = await new Promise(r => setTimeout(() => r(10), 5)); return a + b } f(5).then(v => { out = v }); out").unwrap();
        let _ = v;
        it.run_timers(100.0, 10);
        let v = it.run("out").unwrap();
        assert_eq!(it.to_str(&v).ok().unwrap(), "15");
    });
}

#[test]
fn console_log_collects() {
    big(|| {
        let mut it = Interp::new();
        it.run("console.log('a', 1, [1, 2], {x: 1})").unwrap();
        assert_eq!(it.logs[0], "a 1 [ 1, 2 ] { x: 1 }");
    });
}

#[test]
fn syntax_error_reported() {
    big(|| {
        let mut it = Interp::new();
        assert!(it.run("var = ;").is_err());
        assert!(it.run("class {").is_err());
    });
}

#[test]
fn runaway_recursion_is_caught() {
    big(|| {
        let mut it = Interp::new();
        let r = it.run("function f() { return f() } try { f() } catch (e) { e instanceof RangeError }").unwrap();
        assert_eq!(it.to_str(&r).ok().unwrap(), "true");
    });
}

#[test]
fn dom_prelude() {
    big(|| {
        let mock = include_str!("dom_mock.js");
        let prelude = include_str!("../../nc-app-browser/src/prelude.js");
        let cases: Vec<(&str, &str, &str)> = vec![
            (r##"(function(){ var d = document.createElement('div'); d.id = 'x'; document.body.appendChild(d); return (document.getElementById('x') === d) + d.tagName; })()"##, r##""##, r##"trueDIV"##),
            (r##"(function(){ var d = document.createElement('p'); d.classList.add('a', 'b'); d.classList.toggle('a'); d.classList.add('c'); return d.className + '|' + d.classList.contains('b'); })()"##, r##""##, r##"b c|true"##),
            (r##"(function(){ var d = document.createElement('p'); d.style.backgroundColor = 'red'; d.style.display = 'none'; var a = d.getAttribute('style'); d.style.display = ''; return a + ' / ' + d.getAttribute('style') + ' / ' + d.style.backgroundColor; })()"##, r##""##, r##"background-color: red; display: none; / background-color: red; / red"##),
            (r##"(function(){ var d = document.createElement('p'); d.dataset.fooBar = '1'; return d.getAttribute('data-foo-bar') + d.dataset.fooBar; })()"##, r##""##, r##"11"##),
            (r##"(function(){ var a = document.createElement('div'), b = document.createElement('span'); document.body.appendChild(a); a.appendChild(b); var log = []; a.addEventListener('click', function (e) { log.push('a:' + e.target.tagName + (e.currentTarget === a)); }); document.addEventListener('click', function () { log.push('doc'); }); window.addEventListener('click', function () { log.push('win'); }); b.click(); return log.join(','); })()"##, r##""##, r##"a:SPANtrue,doc,win"##),
            (r##"(function(){ var a = document.createElement('div'), b = document.createElement('i'); a.appendChild(b); var log = []; a.addEventListener('click', function () { log.push('a'); }); b.addEventListener('click', function (e) { log.push('b'); e.stopPropagation(); }); b.click(); return log.join(','); })()"##, r##""##, r##"b"##),
            (r##"(function(){ var a = document.createElement('b'); var n = 0; a.onclick = function () { n++; }; a.addEventListener('click', function () { n += 10; }, { once: true }); a.click(); a.click(); return n; })()"##, r##""##, r##"12"##),
            (r##"(function(){ var a = document.createElement('a'); document.body.appendChild(a); a.addEventListener('click', function (e) { e.preventDefault(); }); return __click(a['@@id']); })()"##, r##""##, r##"true"##),
            (r##"(function(){ var d = document.createElement('div'); d.className = 'q'; document.body.appendChild(d); return document.querySelectorAll('.q').length + ':' + (document.querySelector('.q') === d); })()"##, r##""##, r##"1:true"##),
            (r##"(function(){ var a = document.createElement('div'); a.className = 'k'; var b = document.createElement('span'); a.appendChild(b); return b.closest('.k') === a; })()"##, r##""##, r##"true"##),
            (r##"(function(){ var d = document.createElement('div'); d.textContent = 'hi'; d.appendChild(document.createTextNode('!')); return d.textContent + d.childNodes.length; })()"##, r##""##, r##"hi!2"##),
            (r##"(function(){ var f = document.createDocumentFragment(); f.appendChild(document.createElement('i')); f.appendChild(document.createElement('b')); var d = document.createElement('div'); d.appendChild(f); return d.children.length + ':' + f.childNodes.length; })()"##, r##""##, r##"2:0"##),
            (r##"(function(){ var p = document.createElement('div'), a = document.createElement('i'), b = document.createElement('b'); p.appendChild(a); p.appendChild(b); return (a.nextSibling === b) + ',' + (b.previousSibling === a) + ',' + (p.firstChild === a) + ',' + (p.lastElementChild === b); })()"##, r##""##, r##"true,true,true,true"##),
            (r##"(function(){ var i = document.createElement('input'); i.value = 'abc'; i.checked = true; return i.value + i.checked + i.getAttribute('checked'); })()"##, r##""##, r##"abctrue"##),
            (r##"location.pathname + '|' + location.search + '|' + location.hostname + '|' + location.origin + '|' + location.hash"##, r##""##, r##"/a/b|?x=1|example.com|https://example.com|#h"##),
            (r##"(function(){ location.href = 'https://z.com/'; return location.href; })()"##, r##""##, r##"https://z.com/"##),
            (r##"(function(){ var u = new URL('../p?a=1&b=2#f', 'https://h.com/x/y/z'); u.searchParams.set('c', '3'); return u.href + ' ' + u.searchParams.get('b'); })()"##, r##""##, r##"https://h.com/x/p?a=1&b=2&c=3#f 2"##),
            (r##"(function(){ localStorage.setItem('a', 1); localStorage.b = 2; return localStorage.getItem('a') + localStorage.b + localStorage.length + localStorage.getItem('zz'); })()"##, r##""##, r##"122null"##),
            (r##"btoa('Hello, World!') + ' ' + atob(btoa('round trip'))"##, r##""##, r##"SGVsbG8sIFdvcmxkIQ== round trip"##),
            (r##"(function(){ var ti = document.createElement('title'); document.head.appendChild(ti); document.title = 'T'; return document.title; })()"##, r##""##, r##"T"##),
            (r##"(function(){ document.cookie = 'a=1; path=/'; document.cookie = 'b=2'; document.cookie = 'a=3'; return document.cookie; })()"##, r##""##, r##"b=2; a=3"##),
            (r##"(function(){ var c = new AbortController(); var f = false; c.signal.addEventListener('abort', function () { f = true; }); c.abort(); return f && c.signal.aborted; })()"##, r##""##, r##"true"##),
            (r##"var done = []; fetch('/data.json').then(function (r) { return r.json(); }).then(function (j) { done.push('fetch json ' + j.v); }); __net_done(1, true, 200, '{"v":42}', 'https://z.com/data.json'); 0"##, r##"done.join()"##, r##"fetch json 42"##),
            (r##"var done = []; var x = new XMLHttpRequest(); x.open('GET', '/x'); x.onload = function () { done.push('xhr ' + x.status + x.responseText); }; x.send(); __net_done(1, true, 200, 'ok', 'u'); 0"##, r##"done.join()"##, r##"xhr 200ok"##),
            (r##"var done = []; document.addEventListener('DOMContentLoaded', function () { done.push('dcl'); }); window.addEventListener('load', function () { done.push('load'); }); __fire_ready(); 0"##, r##"done.join() + ' ' + document.readyState"##, r##"dcl,load complete"##),
            (r##"(function(){ var u = new Uint8Array([1, 2, 300]); u[0] = 257; var f = new Float64Array(2); f[1] = 1.5; return u[0] + ',' + u[2] + ',' + u.length + ',' + f[1] + ',' + Array.from(u).join('|') + ',' + (u instanceof Uint8Array) + ',' + Uint32Array.BYTES_PER_ELEMENT; })()"##, r##""##, r##"1,44,3,1.5,1|2|44,true,4"##),
            (r##"(function(){ var e = new TextEncoder().encode('a\u00e9\u20ac'); return e.length + ':' + new TextDecoder().decode(e); })()"##, r##""##, r##"6:aé€"##),
            (r##"(function(){ var c = document.createElement('canvas'); var x = c.getContext('2d'); x.fillRect(0, 0, 1, 1); return (c.getContext('webgl') === null) + ',' + x.measureText('abc').width + ',' + (x.canvas === c); })()"##, r##""##, r##"true,18,true"##),
            (r##"(function(){ var e = document.createEvent('CustomEvent'); e.initCustomEvent('x', true, false, 5); return e.type + e.detail + e.bubbles; })()"##, r##""##, r##"x5true"##),
            (r##"NodeFilter.SHOW_ELEMENT + ',' + Node.TEXT_NODE + ',' + (typeof CSS.supports('a', 'b'))"##, r##""##, r##"1,3,boolean"##),
            (r##"(function(){ document.cookie = 'sid=abc; path=/'; document.cookie = 'k=v'; return document.cookie; })()"##, r##""##, r##"sid=abc; k=v"##),
            (r##"(function(){ var f = new FormData(); f.append('a', 1); f.append('b', 'x y'); return f.toString() + '|' + new Blob(['abc', 'de']).size; })()"##, r##""##, r##"a=1&b=x+y|5"##),
            (r##"new Image().tagName + new Option('t', 'v').getAttribute('value')"##, r##""##, r##"IMGv"##),
            (r##"var done = []; fetch('/post', { method: 'POST', headers: { 'X-Test': '1' }, body: new URLSearchParams({ a: '1', b: '2' }) }).then(function (r) { done.push('post ' + r.status + ' ' + r.headers.get('content-type')); }); __net_done(1, true, 201, 'ok', 'https://z.com/post', 'Content-Type: application/json\nX-A: b\n'); 0"##, r##"done.join() + ' ' + __queue[0][1] + ' ' + __queue[0][3]"##, r##"post 201 application/json POST a=1&b=2"##),
            (r##"(function(){ var u = new Int16Array(3); u.fill(70000); return u.join() + '/' + u.subarray(1).length + '/' + [...new Uint8Array([5, 6])].join(); })() /*subarray*/"##, r##""##, r##"4464,4464,4464/2/5,6"##),
            (r##"var out = []; for (const [k, v] of new URLSearchParams('a=1&b=%20x')) out.push(k + v); out.join('|')"##, r##""##, r##"a1|b x"##),
            (r##"var log=[]; var d=document.createElement('div'); document.body.appendChild(d); var mo=new MutationObserver(function(recs){ log.push(recs.length+':'+recs[0].type+':'+recs[0].addedNodes.length+':'+recs[0].target.tagName); }); mo.observe(d,{childList:true}); d.appendChild(document.createElement('span')); log.push('sync'); 0"##, r##"log.join('|')"##, r##"sync|1:childList:1:DIV"##),
            (r##"var log=[]; var d=document.createElement('p'); d.setAttribute('title','a'); var mo=new MutationObserver(function(r){ r.forEach(function(x){ log.push(x.attributeName+'='+x.oldValue); }); }); mo.observe(d,{attributes:true,attributeOldValue:true,attributeFilter:['title']}); d.setAttribute('title','b'); d.setAttribute('id','z'); d.classList.add('c'); 0"##, r##"log.join('|')"##, r##"title=a"##),
            (r##"var log=[]; var a=document.createElement('div'),b=document.createElement('div'); a.appendChild(b); var mo=new MutationObserver(function(r){ log.push(r.length+r[0].target.tagName+(r[0].target===b)); }); mo.observe(a,{childList:true,subtree:true}); b.appendChild(document.createElement('i')); 0"##, r##"log.join('|')"##, r##"1DIVtrue"##),
            (r##"var log=[]; var a=document.createElement('div'),b=document.createElement('div'); a.appendChild(b); var mo=new MutationObserver(function(r){ log.push('fired'); }); mo.observe(a,{childList:true}); b.appendChild(document.createElement('i')); 0"##, r##"log.join('|') + '#'"##, r##"#"##),
            (r##"var log=[]; var t=document.createTextNode('x'); var mo=new MutationObserver(function(r){ log.push(r[0].type+':'+r[0].oldValue); }); mo.observe(t,{characterData:true,characterDataOldValue:true}); t.nodeValue='y'; 0"##, r##"log.join('|')"##, r##"characterData:x"##),
            (r##"var n=0; var d=document.createElement('div'); var mo=new MutationObserver(function(){ n++; }); mo.observe(d,{attributes:true}); d.id='a'; var taken=mo.takeRecords().length; mo.disconnect(); d.id='b'; 0"##, r##"n + ',' + taken"##, r##"0,1"##),
            (r##"(function(){ try { new MutationObserver(function(){}).observe(document.body, {}); return 'no'; } catch (e) { return e instanceof TypeError ? 'TypeError' : 'other'; } })()"##, r##""##, r##"TypeError"##),
            (r##"(function(){ history.pushState({a:1}, '', '/next?x=2#h'); return location.pathname + location.search + location.hash + '|' + history.state.a + '|' + history.length; })()"##, r##""##, r##"/next?x=2#h|1|2"##),
            (r##"history.pushState({n:1}, '', '/p1'); history.pushState({n:2}, '', '/p2'); history.back(); 0"##, r##"location.pathname + '|' + JSON.stringify(history.state) + '|' + history.length"##, r##"/p1|{"n":1}|3"##),
            (r##"history.replaceState(null, '', '/r'); 0"##, r##"location.pathname + history.length"##, r##"/r1"##),
            (r##"(function(){ try { history.pushState(null, '', 'https://evil.example/x'); return 'no'; } catch (e) { return e.name; } })()"##, r##""##, r##"SecurityError"##),
            (r##"location.hash = 'top'; 0"##, r##"location.href + '|' + history.length"##, r##"https://example.com/a/b?x=1#top|2"##),
            (r##"(function(){ try { document.createElement('div').attachShadow({ mode: 'open' }); return 'no'; } catch (e) { return e.name; } })()"##, r##""##, r##"NotSupportedError"##),
            (r##"(function(){ class A {} customElements.define('x-a', A); var dup = ''; try { customElements.define('x-a', A); } catch (e) { dup = e.name; } var bad = ''; try { customElements.define('nodash', A); } catch (e) { bad = e.name; } return (customElements.get('x-a') === A) + ',' + dup + ',' + bad + ',' + (customElements.get('x-b') === undefined); })()"##, r##""##, r##"true,NotSupportedError,SyntaxError,true"##),
            (r##"typeof IntersectionObserver + ',' + typeof ResizeObserver + ',' + ('IntersectionObserver' in window) + ',' + PerformanceObserver.supportedEntryTypes.length"##, r##""##, r##"undefined,undefined,false,0"##),
            (r##"(function(){ var d = document.implementation.createHTMLDocument('T'); var p = d.createElement('p'); d.body.appendChild(p); return d.title + ',' + (d.body.firstChild === p) + ',' + document.implementation.hasFeature(); })()"##, r##""##, r##"T,true,true"##),
            (r##"CSS.supports('position', 'absolute') + ',' + CSS.supports('position', 'sticky') + ',' + CSS.supports('font-weight', '700') + ',' + CSS.supports('transform', 'none') + ',' + CSS.supports('border-bottom', '1px solid red')"##, r##""##, r##"true,false,true,false,true"##),
            (r##"(function(){ var e = new DOMException('boom', 'NotFoundError'); return (e instanceof Error) + ',' + e.name + ',' + e.code + ',' + e.message + ',' + DOMException.NOT_FOUND_ERR; })()"##, r##""##, r##"true,NotFoundError,8,boom,8"##),
            (r##"(function(){ NodeList.prototype.extra = 1; HTMLCollection.prototype.item2 = 2; var a = [1, 2]; return typeof NodeList + ',' + typeof HTMLCollection + ',' + (typeof NodeList.prototype.item) + ',' + (NodeList.prototype.item.call(a, 1)) + ',' + (NodeList.prototype.item.call(a, 5) === null); })()"##, r##""##, r##"function,function,function,2,true"##),
            (r##"(function(){ var d = document.createElement('div'); return (d instanceof HTMLElement) + ',' + (d instanceof Element) + ',' + typeof HTMLInputElement + ',' + typeof ShadowRoot; })()"##, r##""##, r##"true,true,function,function"##),
            (r##"(function(){ var r = new DOMRect(10, 20, 30, 40); return r.right + ',' + r.bottom + ',' + JSON.stringify(DOMRect.fromRect({ width: 5 }).toJSON()); })()"##, r##""##, r##"40,60,{"x":0,"y":0,"width":5,"height":0,"top":0,"right":5,"bottom":0,"left":0}"##),
            (r##"var h = new Headers(); h.append('B', '1'); h.append('b', '2'); h.set('a', 'x'); 0"##, r##"h.entries().map(function (p) { return p.join('='); }).join('|')"##, r##"a=x|b=1, 2"##),
            (r##"var f = new FormData(); f.append('a', '1'); f.append('a', '2'); 0"##, r##"f.getAll('a').join() + ',' + f.values().join() + ',' + f.keys().length"##, r##"1,2,1,2,2"##),
            (r##"new Intl.NumberFormat('en-US').format(1234567.891) + '|' + new Intl.NumberFormat('en-US', { style: 'currency', currency: 'USD' }).format(1234.5) + '|' + new Intl.NumberFormat('tr-TR', { style: 'currency', currency: 'TRY' }).format(1234.5) + '|' + new Intl.NumberFormat('en-US', { style: 'percent' }).format(0.256) + '|' + (1234.5).toLocaleString('de-DE')"##, r##""##, r##"1,234,567.891|$1,234.50|₺1.234,50|26%|1.234,5"##),
            (r##"new Intl.PluralRules('en').select(1) + ',' + new Intl.PluralRules('en', { type: 'ordinal' }).select(22) + ',' + new Intl.ListFormat('en').format(['a', 'b', 'c']) + ',' + new Intl.RelativeTimeFormat('en').format(-3, 'day')"##, r##""##, r##"one,two,a, b, and c,3 days ago"##),
            (r##"Reflect.getOwnPropertyDescriptor({ a: 1 }, 'a').value + ',' + Reflect.isExtensible({}) + ',' + Reflect.ownKeys({ x: 1, y: 2 }).join()"##, r##""##, r##"1,true,x,y"##),
            (r##"var c = new AbortController(); var n = 0; c.signal.addEventListener('abort', function () { n++; }); c.abort(); 0"##, r##"AbortSignal.abort().aborted + ',' + c.signal.aborted + ',' + n + ',' + c.signal.reason.name"##, r##"true,true,1,AbortError"##),
            (r##"new WheelEvent('wheel', { deltaY: 5 }).deltaY + ',' + (new ErrorEvent('error', { message: 'm' }) instanceof Event) + ',' + new CloseEvent('close', { code: 1000 }).code"##, r##""##, r##"5,true,1000"##),
            (r##"var d = document.createElement('div'); d.appendChild(document.createElement('i')); d.replaceChildren('a', document.createElement('b')); d.insertAdjacentText('beforeend', 'z'); 0"##, r##"d.childNodes.length + ',' + d.textContent"##, r##"3,az"##),
            (r##"var a = document.createElement('a'); a.setAttribute('href', 'https://x.org:8080/p/q?z=1#h'); 0"##, r##"a.hostname + '|' + a.pathname + '|' + a.search + '|' + a.hash + '|' + a.port"##, r##"x.org|/p/q|?z=1|#h|8080"##),
            (r##"var st = document.createElement('style'); document.head.appendChild(st); st.sheet.insertRule('a { color: red }', 0); st.sheet.insertRule('b { color: blue }', 1); 0"##, r##"st.sheet.cssRules.length + ',' + st.sheet.cssRules[1].selectorText + ',' + (st.textContent.indexOf('color: blue') >= 0) + ',' + (document.styleSheets.length >= 1)"##, r##"2,b,true,true"##),
            (r##"var st2 = document.createElement('style'); document.head.appendChild(st2); st2.sheet.insertRule('a{}', 0); st2.sheet.insertRule('b{}', 1); st2.sheet.deleteRule(0); 0"##, r##"st2.sheet.cssRules.length + ',' + st2.sheet.cssRules[0].selectorText + ',' + (function(){ try { st2.sheet.insertRule('x{}', 9); return 'no'; } catch (e) { return e.name; } })()"##, r##"1,b,IndexSizeError"##),
            (r##"var st3 = document.createElement('style'); st3.textContent = '@import url(x.css); p { margin: 0 } @media (min-width: 1px) { q { color: red } }'; 0"##, r##"st3.sheet.cssRules.length + ',' + st3.sheet.cssRules[1].selectorText + ',' + st3.sheet.cssRules[2].type + ',' + document.createElement('div').sheet"##, r##"3,p,4,null"##),
            (r##"CSS.supports('display', 'flex') + ',' + CSS.supports('position', 'sticky') + ',' + CSS.supports('(display: grid) and (not (position: sticky))') + ',' + CSS.supports('selector(:has(a))') + ',' + CSS.supports('--x', '1')"##, r##""##, r##"true,false,true,false,true"##),
        ];
        let mut failed = 0;
        for (setup, check, expected) in cases {
            let mut it = Interp::new();
            if let Err(e) = it.run(mock) {
                println!("MOCK ERROR: {}", e);
                failed += 1;
                continue;
            }
            if let Err(e) = it.run(prelude) {
                println!("PRELUDE ERROR: {}", e);
                failed += 1;
                continue;
            }
            if let Ok(v) = it.run("__prelude_errors.join('; ')") {
                let s = it.to_str(&v).ok().unwrap_or_default();
                if !s.is_empty() {
                    println!("PRELUDE SECTION ERRORS: {}", s);
                    failed += 1;
                    continue;
                }
            }
            let first = it.run(setup);
            let mut got = match first {
                Ok(v) => it.to_str(&v).ok().unwrap_or_default(),
                Err(e) => format!("ERR {}", e),
            };
            if !check.is_empty() {
                got = match it.run(check) {
                    Ok(v) => it.to_str(&v).ok().unwrap_or_default(),
                    Err(e) => format!("ERR {}", e),
                };
            }
            if got != expected {
                failed += 1;
                println!("FAIL: {}\n  expected: {}\n  got:      {}", setup, expected, got);
            }
        }
        assert_eq!(failed, 0, "{} DOM prelude case(s) failed", failed);
    });
}

