use std::collections::BTreeMap;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use dallo::{ModuleId, Ser};
use parking_lot::RwLock;
use rkyv::{archived_value, Archive, Deserialize, Infallible, Serialize};
use wasmer::{imports, Exports, Function, Val};

use crate::{Env, Error, Instance, MemHandler};

#[derive(Debug)]
pub struct WorldInner(BTreeMap<ModuleId, Env>);

impl Deref for WorldInner {
    type Target = BTreeMap<ModuleId, Env>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for WorldInner {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Debug, Clone)]
pub struct World(Arc<RwLock<WorldInner>>);

impl World {
    pub fn new() -> Self {
        World(Arc::new(RwLock::new(WorldInner(BTreeMap::new()))))
    }

    pub fn deploy(&mut self, bytecode: &[u8]) -> Result<ModuleId, Error> {
        let store = wasmer::Store::default();
        let module = wasmer::Module::new(&store, bytecode)?;

        let mut env = Env::uninitialized();

        #[rustfmt::skip]
        let imports = imports! {
            "env" => {
                "alloc" => Function::new_native_with_env(&store, env.clone(), host_alloc),
		"dealloc" => Function::new_native_with_env(&store, env.clone(), host_dealloc),

                "snap" => Function::new_native_with_env(&store, env.clone(), host_snapshot),
		
                "q" => Function::new_native_with_env(&store, env.clone(), host_query),
		"t" => Function::new_native_with_env(&store, env.clone(), host_transact),
            }
        };

        let instance = wasmer::Instance::new(&module, &imports)?;

        let arg_buf_ofs = global_i32(&instance.exports, "A")?;
        let arg_buf_len_pos = global_i32(&instance.exports, "AL")?;
        let heap_base = global_i32(&instance.exports, "__heap_base")?;

        // We need to read the actual value of AL from the offset into memory

        let mem = instance.exports.get_memory("memory")?;
        let data = &unsafe { mem.data_unchecked() }[arg_buf_len_pos as usize..][..4];

        let arg_buf_len: i32 = unsafe { archived_value::<i32>(data, 0) }
            .deserialize(&mut Infallible)
            .expect("infallible");

        let id = blake3::hash(bytecode).into();

        let instance = Instance {
            id,
            instance,
            world: self.clone(),
            mem_handler: MemHandler::new(heap_base as usize),
            arg_buf_ofs,
            arg_buf_len,
            heap_base,
        };

	env.initialize(instance);

        self.0.write().insert(id, env);

        Ok(id)
    }

    pub fn query<Arg, Ret>(&self, m_id: ModuleId, name: &str, arg: Arg) -> Result<Ret, Error>
    where
        Arg: for<'a> Serialize<Ser<'a>>,
        Ret: Archive,
        Ret::Archived: Deserialize<Ret, Infallible>,
    {
        self.0
            .read()
            .get(&m_id)
            .expect("invalid module id")
            .inner()
            .query(name, arg)
    }

    pub fn transact<Arg, Ret>(&mut self, m_id: ModuleId, name: &str, arg: Arg) -> Result<Ret, Error>
    where
        Arg: for<'a> Serialize<Ser<'a>>,
        Ret: Archive,
        Ret::Archived: Deserialize<Ret, Infallible>,
    {
        self.0
            .write()
            .get_mut(&m_id)
            .expect("invalid module id")
            .inner_mut()
            .transact(name, arg)
    }
}

fn global_i32(exports: &Exports, name: &str) -> Result<i32, Error> {
    if let Val::I32(i) = exports.get_global(name)?.get() {
        Ok(i)
    } else {
        Err(Error::MissingModuleExport)
    }
}

fn host_alloc(env: &Env, amount: i32, align: i32) -> i32 {
    env.inner_mut()
        .alloc(amount as usize, align as usize)
        .try_into()
        .expect("i32 overflow")
}

fn host_dealloc(env: &Env, addr: i32) {
    env.inner_mut().dealloc(addr as usize)
}

// Debug helper to take a snapshot of the memory of the running process.
fn host_snapshot(env: &Env) {
    env.inner().snap()
}

fn host_query(
    env: &Env,
    module_id_adr: i32,
    method_name_adr: i32,
    method_name_len: i32,
    arg_ofs: i32,
) -> i32 {
    let module_id_adr = module_id_adr as usize;
    let method_name_adr = method_name_adr as usize;
    let method_name_len = method_name_len as usize;
    let _arg_ofs = arg_ofs as usize;

    let i = env.inner();
    let mut mod_id = ModuleId::default();
    // performance: use a dedicated buffer here?
    let mut name = String::new();

    i.with_memory(|buf| {
        mod_id[..].copy_from_slice(&buf[module_id_adr..][..core::mem::size_of::<ModuleId>()]);
        let utf = core::str::from_utf8(&buf[method_name_adr..][..method_name_len])
            .expect("TODO, error out cleaner");
        name.push_str(utf)
    });

    let _ = env.inner().world;
    
    // At this point we have the mod_id, and the name of the method.
    // The caller has written the arguments to its own arg buffer.

    todo!()
}

fn host_transact(
    _env: &Env,
    _module_id_adr: i32,
    _method_name_adr: i32,
    _method_name_len: i32,
    _buffer_arg_ofs: i32,
) -> i32 {
    todo!()
}
