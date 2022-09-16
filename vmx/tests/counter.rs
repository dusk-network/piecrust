use vmx::{module_bytecode, Error, VM};

#[test]
fn counter_read_simple() -> Result<(), Error> {
    let mut vm = VM::new();
    let id = vm.deploy(module_bytecode!("counter"))?;

    assert_eq!(vm.query::<(), i64>(id, "read_value", ())?, 0xfc);

    Ok(())
}

#[test]
fn counter_read_write_simple() -> Result<(), Error> {
    let mut vm = VM::new();
    let id = vm.deploy(module_bytecode!("counter"))?;

    let mut session = vm.session();

    assert_eq!(session.query::<(), i64>(id, "read_value", ())?, 0xfc);

    session.transact::<(), ()>(id, "increment", ())?;

    assert_eq!(session.query::<(), i64>(id, "read_value", ())?, 0xfd);

    Ok(())
}

#[test]
fn counter_read_write_session() -> Result<(), Error> {
    let mut vm = VM::new();
    let id = vm.deploy(module_bytecode!("counter"))?;

    {
        let mut session = vm.session();

        assert_eq!(session.query::<(), i64>(id, "read_value", ())?, 0xfc);

        session.transact::<(), ()>(id, "increment", ())?;

        assert_eq!(session.query::<(), i64>(id, "read_value", ())?, 0xfd);
    }

    // mutable session dropped without commiting.
    // old counter value still accessible.

    assert_eq!(vm.query::<(), i64>(id, "read_value", ())?, 0xfc);

    let mut other_session = vm.session();

    other_session.transact::<(), ()>(id, "increment", ())?;

    let _commit_id = other_session.commit();

    // session committed, new value accessible

    assert_eq!(vm.query::<(), i64>(id, "read_value", ())?, 0xfd);

    Ok(())
}
