#![no_std]

#[allow(unused_imports)]
use piecrust_uplink as uplink;

#[no_mangle]
fn bad_function(bad_arg: i64) -> i64 {
    bad_arg
}
