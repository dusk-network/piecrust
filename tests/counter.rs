use hatchery::{module, Error};
use rend::LittleEndian;

#[test]
pub fn counter_trivial() -> Result<(), Error> {
    println!("runt trivial counter test yo");

    let module = module!("counter")?;

    println!("env created!");

    module.snap();

    let value: i32 = module.query("read_value", ())?;

    assert_eq!(value, 0xfc);

    Ok(())
}

#[test]
pub fn counter_increment() -> Result<(), Error> {
    let mut module = module!("counter")?;

    module.transact("increment", ())?;

    let value: LittleEndian<i32> = module.query("read_value", ())?;
    assert_eq!(value, 0xfd);

    module.transact("increment", ())?;

    let value: LittleEndian<i32> = module.query("read_value", ())?;
    assert_eq!(value, 0xfe);

    Ok(())
}
