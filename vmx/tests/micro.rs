use vmx::{module_bytecode, Error, VM};

#[test]
fn micro() -> Result<(), Error> {
    let mut vm = VM::new();

    println!("a");

    let id = vm.deploy(module_bytecode!("counter"))?;

    println!("b");

    assert_eq!(vm.query::<(), i64>(id, "read", ())?, 42);

    println!("c");

    Ok(())
}
