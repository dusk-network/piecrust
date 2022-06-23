use rkyv::ser::serializers::BufferSerializerError;

#[derive(Debug)]
pub enum Error {
    InstantiationError(wasmer::InstantiationError),
    CompileError(wasmer::CompileError),
    ExportError(wasmer::ExportError),
    RuntimeError(wasmer::RuntimeError),
    MissingModuleExport,
    BufferSerializerError(BufferSerializerError),
}

impl From<wasmer::InstantiationError> for Error {
    fn from(e: wasmer::InstantiationError) -> Self {
        Error::InstantiationError(e)
    }
}

impl From<wasmer::CompileError> for Error {
    fn from(e: wasmer::CompileError) -> Self {
        Error::CompileError(e)
    }
}

impl From<wasmer::ExportError> for Error {
    fn from(e: wasmer::ExportError) -> Self {
        Error::ExportError(e)
    }
}

impl From<wasmer::RuntimeError> for Error {
    fn from(e: wasmer::RuntimeError) -> Self {
        Error::RuntimeError(e)
    }
}

impl From<BufferSerializerError> for Error {
    fn from(e: BufferSerializerError) -> Self {
        Error::BufferSerializerError(e)
    }
}
