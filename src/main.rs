#![no_std]
#![no_main]
#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(static_mut_refs)]
#![allow(unused_comparisons)]
#![allow(asm_sub_register)]
#![allow(suspicious_runtime_symbol_definitions)]

mod app;
mod arch;
mod bench;
mod elfself;
mod env;
mod fmt;
mod json;
mod meta;
mod output;
mod rand;
mod rt;
mod runner;
mod sha256;
mod stats;
mod sys;
mod time;
mod util;
