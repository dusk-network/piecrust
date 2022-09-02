use bytecheck::CheckBytes;
use rkyv::{
    validation::validators::DefaultValidator, Archive, Deserialize, Infallible,
    Serialize,
};
use std::collections::BTreeMap;

use crate::instance::WrappedInstance;
use crate::types::{Error, StandardBufSerializer};
use crate::vm::{ModuleId, VM};

pub struct QuerySession<'a> {
    vm: &'a mut VM,
    instances: BTreeMap<ModuleId, WrappedInstance>,
}

impl<'a> QuerySession<'a> {
    pub fn new(vm: &'a mut VM) -> Self {
        QuerySession {
            vm,
            instances: BTreeMap::new(),
        }
    }

    fn initialize_module(&mut self, id: ModuleId) -> Result<(), Error> {
        if self.instances.get(&id).is_some() {
            return Ok(());
        }
        let module = self.vm.module(id);
        let wrapped = WrappedInstance::new(module)?;
        self.instances.insert(id, wrapped);
        Ok(())
    }

    fn get_instance(
        &mut self,
        id: ModuleId,
    ) -> Result<&mut WrappedInstance, Error> {
        self.initialize_module(id)?;
        Ok(self.instances.get_mut(&id).expect("initialized above"))
    }

    pub fn query<Arg, Ret>(
        &mut self,
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
        let i = self.get_instance(id)?;
        i.query(method_name, arg)
    }
}
