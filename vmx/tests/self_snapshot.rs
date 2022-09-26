use vmx::{module_bytecode, Error, RawTransaction, VM};

#[test]
fn self_snapshot() -> Result<(), Error> {
    let mut vm = VM::ephemeral()?;

    let contract_id = vm.deploy(module_bytecode!("self_snapshot"))?;

    assert_eq!(7, vm.query::<_, i32>(contract_id, "crossover", ())?);

    // returns old value

    let mut session = vm.session();

    let res = session.transact::<_, i32>(contract_id, "set_crossover", 9)?;

    assert_eq!(res, 7);

    assert_eq!(9, session.query::<_, i32>(contract_id, "crossover", ())?);

    session.transact(contract_id, "self_call_test_a", 10)?;

    assert_eq!(10, session.query::<_, i32>(contract_id, "crossover", ())?);

    let result = session.transact::<_, ()>(contract_id, "update_and_panic", 11);

    assert!(result.is_err());

    assert_eq!(10, session.query::<_, i32>(contract_id, "crossover", ())?);

    let raw = RawTransaction::new("set_crossover", 12);

    session.transact::<_, ()>(
        contract_id,
        "self_call_test_b",
        (contract_id, raw),
    )?;

    assert_eq!(12, session.query::<_, i32>(contract_id, "crossover", ())?);

    Ok(())
}
