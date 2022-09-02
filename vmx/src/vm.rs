use std::collections::BTreeMap;

use bytecheck::CheckBytes;
use rkyv::{
    validation::validators::DefaultValidator, Archive, Deserialize, Infallible,
    Serialize,
};

use crate::module::WrappedModule;
use crate::query_session::QuerySession;
use crate::types::{Error, StandardBufSerializer};

#[derive(Clone, Copy, PartialOrd, Ord, PartialEq, Eq)]
pub struct ModuleId(usize);

pub struct VM {
    modules: BTreeMap<ModuleId, WrappedModule>,
}

impl VM {
    pub fn new() -> Self {
        VM {
            modules: BTreeMap::new(),
        }
    }

    pub fn deploy(&mut self, bytecode: &[u8]) -> Result<ModuleId, Error> {
        let id = ModuleId(self.modules.len());
        let module = WrappedModule::new(bytecode)?;
        self.modules.insert(id, module);
        Ok(id)
    }

    pub fn module(&self, id: ModuleId) -> &WrappedModule {
        self.modules.get(&id).expect("Invalid ModuleId")
    }

    pub fn query<Arg, Ret>(
        &self,
        id: ModuleId,
        method_name: &str,
        arg: Arg,
    ) -> Result<Ret, Error>
    where
        Arg: for<'b> Serialize<StandardBufSerializer<'b>>,
        Ret: Archive,
        Ret::Archived: Deserialize<Ret, Infallible>
            + for<'b> CheckBytes<DefaultValidator<'b>>,
    {
        let mut session = QuerySession::new(self);
        session.query(id, method_name, arg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() -> Result<(), Error> {
        let mut vm = VM::new();
        let id = vm.deploy(module_bytecode!("counter"))?;

        assert_eq!(vm.query::<(), i64>(id, "read_value", ())?, 0xfc);

        Ok(())
    }
}
