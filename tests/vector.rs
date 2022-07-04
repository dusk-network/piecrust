use hatchery::{module, Error};

#[test]
pub fn vector_push_pop() -> Result<(), Error> {
    let mut module = module!("vector")?;

    const N: usize = 128;

    for i in 0..N {
        module.transact("push", i)?;
    }

    for i in 0..N {
        let popped: Option<i16> = module.transact("pop", ())?;

        assert_eq!(popped, Some((N - i - 1) as i16));
    }

    let popped: Option<i16> = module.transact("pop", ())?;

    assert_eq!(popped, None);

    Ok(())
}
