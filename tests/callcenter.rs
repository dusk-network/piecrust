use hatchery::{module, Error, World};

#[test]
pub fn call_counter() -> Result<(), Error> {
    let mut world = World::default();

    //    world.deploy(module!("counter")?);

    Ok(())
}
