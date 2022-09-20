use vmx::{module_bytecode, Error, VM};

#[test]
pub fn box_set_get() -> Result<(), Error> {
    let mut vm = VM::ephemeral()?;

    let id = vm.deploy(module_bytecode!("box"))?;

    let value: Option<i16> = vm.query(id, "get", ())?;

    assert_eq!(value, None);

    let mut session = vm.session();

    session.transact::<i16, ()>(id, "set", 0x11)?;

    let value = session.query::<_, Option<i16>>(id, "get", ())?;

    assert_eq!(value, Some(0x11));

    Ok(())
}
