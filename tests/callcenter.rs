use hatchery::{module, Error};

#[test]
pub fn call_counter() -> Result<(), Error> {
    let world = World::new();

    world.deploy(module!("counter")?);

    Ok(())
}
