use hatchery::{module, Error, World};

#[test]
pub fn world_call_counter() -> Result<(), Error> {
    let mut world = World::default();

    let c_id = world.deploy(module!("counter")?);

    let value: i32 = world.query(c_id, "read_value", ())?;

    assert_eq!(value, 0xfc);

    Ok(())
}
