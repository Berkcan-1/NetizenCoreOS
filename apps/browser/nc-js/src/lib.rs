#![cfg_attr(not(test), no_std)]

extern crate alloc;

mod builtins;
mod interp;
mod lexer;
mod parser;
mod regex;

pub use interp::{num_to_str, parse_index, Ctl, Interp, Kind, NativeDataFn, NativeFn, Obj, ObjRef, Res, Value};

#[cfg(test)]
mod tests;
