# nc-app-stopwatch/src

## lib.rs

StopwatchApp: running, started_ms, banked_ms ve hover tutar. Sure ctx.uptime_ms'ten hesaplanir (wrapping_sub). Calisirken
update her karede Update::REDRAW doner; durunca sadece hover veya tik degisince doner.
