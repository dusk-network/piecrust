use pando::ModuleStore;
use std::{env, io};

fn main() -> io::Result<()> {
    let mut args = env::args();

    let _ = args.next().unwrap();
    let store = ModuleStore::new(args.next().unwrap())?;

    let mut session = store.genesis_session();

    const BYTECODE_1: &[u8] = b"some really cool bytecode";

    let module_1_id = session.deploy(BYTECODE_1)?;
    let (bytecode_1, _) = session
        .module(module_1_id)?
        .expect("module was just deployed, so it should be present");

    assert_eq!(BYTECODE_1, bytecode_1.as_ref());

    let root = session.commit()?;
    println!("{}", hex::encode(root));

    let mut session = store.session(root)?;

    const BYTECODE_2: &[u8] = b"some much cooler bytecode";

    let module_2_id = session.deploy(BYTECODE_2)?;

    let (bytecode_1, _) = session.module(module_1_id)?.expect(
        "module should be present, since it was already deployed before",
    );
    let (bytecode_2, _) = session
        .module(module_2_id)?
        .expect("module was just deployed, so it should be present");

    assert_eq!(BYTECODE_1, bytecode_1.as_ref());
    assert_eq!(BYTECODE_2, bytecode_2.as_ref());

    let root = session.commit()?;
    println!("{}", hex::encode(root));

    let mut session = store.session(root)?;

    const BYTECODE_3: &[u8] = b"even more coolerer bytecode";

    let _ = session.deploy(BYTECODE_3)?;

    let (bytecode_1, _) = session.module(module_1_id)?.expect(
        "module should be present, since it was already deployed before",
    );
    let (bytecode_2, _) = session.module(module_2_id)?.expect(
        "module should be present, since it was already deployed before",
    );

    assert_eq!(BYTECODE_1, bytecode_1.as_ref());
    assert_eq!(BYTECODE_2, bytecode_2.as_ref());

    let root = session.commit()?;
    println!("{}", hex::encode(root));

    Ok(())
}
