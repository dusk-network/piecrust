#[macro_export]
macro_rules! module_bytecode {
    ($name:literal) => {
        include_bytes!(concat!(
            "../../modules/target/stripped/",
            $name,
            ".wasm"
        ))
    };
}
