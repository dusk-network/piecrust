use crate::{Env, Error};
use dallo::{ModuleId, Ser};
use rkyv::{Archive, Deserialize, Infallible, Serialize};
use std::collections::BTreeMap;

#[derive(Default)]
pub struct World(BTreeMap<ModuleId, Env>);

impl World {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn deploy(&mut self, env: Env) -> ModuleId {
        let id = env.id();
        self.0.insert(id, env);

        println!("deployed id {:?}", id);

        id
    }

    pub fn query<Arg, Ret>(&self, m_id: ModuleId, name: &str, arg: Arg) -> Result<Ret, Error>
    where
        Arg: for<'a> Serialize<Ser<'a>>,
        Ret: Archive + core::fmt::Debug,
        Ret::Archived: Deserialize<Ret, Infallible> + core::fmt::Debug,
    {
        self.0
            .get(&m_id)
            .expect("invalid module id")
            .query(name, arg)
    }

    pub fn transact<Arg, Ret>(&mut self, m_id: ModuleId, name: &str, arg: Arg) -> Result<Ret, Error>
    where
        Arg: for<'a> Serialize<Ser<'a>>,
        Ret: Archive + core::fmt::Debug,
        Ret::Archived: Deserialize<Ret, Infallible> + core::fmt::Debug,
    {
        self.0
            .get_mut(&m_id)
            .expect("invalid module id")
            .transact(name, arg)
    }
}
