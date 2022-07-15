use hatchery::{module_bytecode, Error, World};

#[test]
pub fn world_center_counter_read() -> Result<(), Error> {
    let mut world = World::new();

    let counter_id = world.deploy(module_bytecode!("counter"))?;
    assert_eq!(&counter_id, include_bytes!("../modules/counter/id"));

    let value: i64 = world.query(counter_id, "read_value", ())?;
    assert_eq!(value, 0xfc);

    let center_id = world.deploy(module_bytecode!("callcenter"))?;
    assert_eq!(&center_id, include_bytes!("../modules/callcenter/id"));

    // read value through callcenter

    let value: i64 = world.query(center_id, "query_counter", ())?;
    assert_eq!(value, 0xfc);

    Ok(())
}

#[test]
pub fn world_center_counter() -> Result<(), Error> {
    let mut world = World::new();

    let counter_id = world.deploy(module_bytecode!("counter"))?;
    assert_eq!(&counter_id, include_bytes!("../modules/counter/id"));

    let value: i64 = world.query(counter_id, "read_value", ())?;
    assert_eq!(value, 0xfc);

    let center_id = world.deploy(module_bytecode!("callcenter"))?;
    assert_eq!(&center_id, include_bytes!("../modules/callcenter/id"));

    // read value through callcenter

    let value: i64 = world.query(center_id, "query_counter", ())?;
    assert_eq!(value, 0xfc);

    world.transact(center_id, "increment_counter", ())?;

    // read back without proxy.

    let value: i64 = world.query(counter_id, "read_value", ())?;
    assert_eq!(value, 0xfd);

    // read back with proxy.

    let value: i64 = world.query(center_id, "query_counter", ())?;
    assert_eq!(value, 0xfd);

    Ok(())
}
