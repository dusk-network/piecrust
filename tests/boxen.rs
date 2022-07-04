use hatchery::{module, Error};

#[test]
pub fn box_set_get() -> Result<(), Error> {
    let mut module = module!("box")?;

    let value: Option<i32> = module.query("get", ())?;

    assert_eq!(value, None);

    println!("setting");

    module.transact("set", 0x11)?;

    let value: Option<i16> = module.query("get", ())?;

    println!("fin");

    assert_eq!(value, Some(0x11));

    Ok(())
}
