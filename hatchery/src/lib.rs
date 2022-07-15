mod error;
mod memory;
mod world;
mod env;
mod instance;

pub use world::World;
pub use error::Error;

#[macro_export]
macro_rules! module_bytecode {
    ($name:literal) => {
        include_bytes!(concat!(
            "../../target/wasm32-unknown-unknown/release/",
            $name,
            ".wasm"
        ))
    };
}
