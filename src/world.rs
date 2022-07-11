#[derive(Default)]
pub struct World;

use crate::Env;
use dallo::ModuleId;

impl World {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn deploy(&mut self, env: Env) -> ModuleId {
        todo!()
    }
}
