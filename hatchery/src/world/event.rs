use dallo::ModuleId;

/// An event emitted by a module.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Event {
    module_id: ModuleId,
    data: Vec<u8>,
}

impl Event {
    pub(crate) fn new(module_id: ModuleId, data: Vec<u8>) -> Self {
        Self { module_id, data }
    }

    /// The id of the module that emitted this event.
    pub fn module_id(&self) -> &ModuleId {
        &self.module_id
    }

    /// Data contained with the event
    pub fn data(&self) -> &[u8] {
        &self.data
    }
}
