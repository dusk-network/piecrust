use hatchery::{module, Error};

#[test]
pub fn counter_trivial() -> Result<(), Error> {
    println!("runt trivial counter test yo");

    let module = module!("counter")?;

    println!("env created!");

    let value: i32 = module.query("read_value", ())?;

    assert_eq!(value, 0xfc);

    Ok(())
}

#[test]
pub fn counter_increment() -> Result<(), Error> {
    let mut module = module!("counter")?;

    module.transact("increment", ())?;

    let value: i32 = module.query("read_value", ())?;
    assert_eq!(value, 0xfd);

    module.transact("increment", ())?;

    let value: i32 = module.query("read_value", ())?;
    assert_eq!(value, 0xfe);

    Ok(())
}

#[test]
pub fn counter_mogrify() -> Result<(), Error> {
    println!("runt trivial counter test yo");

    let mut module = module!("counter")?;

    println!("env created!");

    let value: i32 = module.transact("mogrify", 32)?;

    assert_eq!(value, 0xfc);

    let value: i32 = module.query("read_value", ())?;
    assert_eq!(value, 0xfc - 32);

    Ok(())
}
