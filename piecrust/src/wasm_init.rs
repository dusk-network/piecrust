// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Prepares contract bytecode for upstream Wasmtime while preserving Piecrust's
//! persistent-memory semantics. The old Wasmtime fork exposed a
//! `LinearMemory::needs_init` hook. Upstream Wasmtime intentionally does not
//! expose that control: custom memories are an allocation hook, while
//! instantiation must still follow the WebAssembly semantics.
//!
//! Here, active data segments are converted to passive segments and replayed by
//! a reserved VM-only exported initializer that the VM calls only for newly
//! created memories.
//!
//! Piecrust needs active data to be applied once, when a contract's persistent
//! memory is first created, and skipped whenever that memory is reopened.
//!
//! The WebAssembly-native way used here to make memory initialization explicit
//! is to use passive data segments plus the bulk-memory instructions
//! `memory.init` and `data.drop`. During normal instantiation, the spec expands
//! every active data segment into its offset expression, a source offset of
//! zero, the byte length, `memory.init`, and `data.drop`. This module performs
//! the same transformation ahead of compilation, but moves that instruction
//! sequence into a generated initializer function. The VM invokes that function
//! only when the contract memory is new.
//!
//! In Piecrust, "new memory" means a fresh backing store for a contract, not
//! just a newly created Wasmtime instance. For that fresh backing store, the VM
//! runs the generated initializer immediately after `Instance::new` and before
//! calling contract exports such as `init` or later user entrypoints. Later
//! instantiations reopen persisted memory with `is_new = false`, skip the
//! generated initializer, and keep the bytes changed by earlier executions.
//!
//! A fully spec-aligned persistent-memory ABI would make contract memory an
//! import supplied by the host and expose an explicit initialization function.
//! Piecrust currently keeps its existing contract shape, where the module
//! exports a memory that the VM backs through a custom memory creator, so this
//! rewriter applies the explicit-initializer part without changing the contract
//! ABI.
//!
//! Modules with both active data and a start function are rejected. WebAssembly
//! copies active data before running the start function, but a generated export
//! can only be called after `Instance::new`, which is after the start function.
//! Rewriting such a module would let the start function observe uninitialized
//! memory, so Piecrust treats that combination as unsupported.

use std::borrow::Cow;
use std::convert::Infallible;
use std::io;

use wasm_encoder::reencode::{Error as ReencodeError, Reencode};
use wasm_encoder::{
    CodeSection, DataCountSection, DataSection, ExportKind, ExportSection,
    Function, FunctionSection, Module, SectionId, TypeSection,
};
use wasmparser::{
    Data, DataKind, Operator, Parser, Payload, TypeRef, Validator,
};

/// Export name reserved for Piecrust's generated memory initializer.
pub(crate) const MEMORY_INIT_EXPORT: &str = "__piecrust_init_memory";

/// Original active data segment metadata needed to replay initialization.
#[derive(Clone)]
struct ActiveDataSegment<'a> {
    index: u32,
    memory_index: u32,
    offset_expr: wasmparser::ConstExpr<'a>,
    len: usize,
}

/// Module-level facts collected before deciding whether to re-encode.
#[derive(Default)]
struct RewritePlan<'a> {
    type_count: u32,
    imported_func_count: u32,
    defined_func_count: u32,
    data_count: u32,
    has_type_section: bool,
    has_function_section: bool,
    has_export_section: bool,
    has_code_section: bool,
    has_data_count: bool,
    has_start: bool,
    has_reserved_export: bool,
    active_segments: Vec<ActiveDataSegment<'a>>,
}

impl RewritePlan<'_> {
    /// Whether upstream Wasmtime would otherwise initialize contract memory.
    fn needs_rewrite(&self) -> bool {
        !self.active_segments.is_empty()
    }

    /// Function index assigned to the generated initializer.
    fn init_func_index(&self) -> u32 {
        self.imported_func_count + self.defined_func_count
    }
}

/// Re-encodes a module while moving active data initialization into a function.
struct MemoryInitReencoder<'a> {
    plan: &'a RewritePlan<'a>,
    emitted_type: bool,
    emitted_function: bool,
    emitted_export: bool,
    emitted_code: bool,
    emitted_data_count: bool,
}

impl<'a> MemoryInitReencoder<'a> {
    /// Create a reencoder with section-presence state from the analysis pass.
    fn new(plan: &'a RewritePlan<'a>) -> Self {
        Self {
            plan,
            emitted_type: plan.has_type_section,
            emitted_function: plan.has_function_section,
            emitted_export: plan.has_export_section,
            emitted_code: plan.has_code_section,
            emitted_data_count: plan.has_data_count,
        }
    }

    /// Insert a generated type section when the source module has none.
    fn emit_type(&mut self, module: &mut Module) {
        if !self.emitted_type {
            let mut types = TypeSection::new();
            types.ty().function([], []);
            module.section(&types);
            self.emitted_type = true;
        }
    }

    /// Insert a function section that declares the generated initializer.
    fn emit_function(&mut self, module: &mut Module) {
        if !self.emitted_function {
            let mut functions = FunctionSection::new();
            functions.function(self.plan.type_count);
            module.section(&functions);
            self.emitted_function = true;
        }
    }

    /// Export the generated initializer under Piecrust's reserved name.
    fn emit_export(&mut self, module: &mut Module) {
        if !self.emitted_export {
            let mut exports = ExportSection::new();
            exports.export(
                MEMORY_INIT_EXPORT,
                ExportKind::Func,
                self.plan.init_func_index(),
            );
            module.section(&exports);
            self.emitted_export = true;
        }
    }

    /// Insert a data count section required by bulk-memory instructions.
    fn emit_data_count(&mut self, module: &mut Module) {
        if !self.emitted_data_count {
            module.section(&DataCountSection {
                count: self.plan.data_count,
            });
            self.emitted_data_count = true;
        }
    }

    /// Insert a code section containing only the generated initializer.
    fn emit_code(
        &mut self,
        module: &mut Module,
    ) -> Result<(), ReencodeError<Infallible>> {
        if !self.emitted_code {
            let mut code = CodeSection::new();
            code.function(&self.memory_init_function()?);
            module.section(&code);
            self.emitted_code = true;
        }

        Ok(())
    }

    /// Build the initializer that copies each former active segment once.
    fn memory_init_function(
        &mut self,
    ) -> Result<Function, ReencodeError<Infallible>> {
        let mut function = Function::new([]);

        for segment in &self.plan.active_segments {
            let mut reader = segment.offset_expr.get_operators_reader();
            loop {
                match reader.read()? {
                    Operator::End => break,
                    operator => {
                        function.instruction(&self.instruction(operator)?);
                    }
                }
            }

            function
                .instructions()
                .i32_const(0)
                .i32_const(segment.len as i32)
                .memory_init(segment.memory_index, segment.index)
                .data_drop(segment.index);
        }

        function.instructions().end();
        Ok(function)
    }
}

/// Reencode hooks that append generated sections in canonical Wasm order.
impl Reencode for MemoryInitReencoder<'_> {
    type Error = Infallible;

    fn parse_type_section(
        &mut self,
        types: &mut TypeSection,
        section: wasmparser::TypeSectionReader<'_>,
    ) -> Result<(), ReencodeError<Self::Error>> {
        wasm_encoder::reencode::utils::parse_type_section(
            self, types, section,
        )?;
        types.ty().function([], []);
        self.emitted_type = true;
        Ok(())
    }

    fn parse_function_section(
        &mut self,
        functions: &mut FunctionSection,
        section: wasmparser::FunctionSectionReader<'_>,
    ) -> Result<(), ReencodeError<Self::Error>> {
        wasm_encoder::reencode::utils::parse_function_section(
            self, functions, section,
        )?;
        functions.function(self.plan.type_count);
        self.emitted_function = true;
        Ok(())
    }

    fn parse_export_section(
        &mut self,
        exports: &mut ExportSection,
        section: wasmparser::ExportSectionReader<'_>,
    ) -> Result<(), ReencodeError<Self::Error>> {
        wasm_encoder::reencode::utils::parse_export_section(
            self, exports, section,
        )?;
        exports.export(
            MEMORY_INIT_EXPORT,
            ExportKind::Func,
            self.plan.init_func_index(),
        );
        self.emitted_export = true;
        Ok(())
    }

    fn data_count(
        &mut self,
        count: u32,
    ) -> Result<u32, ReencodeError<Self::Error>> {
        self.emitted_data_count = true;
        Ok(count)
    }

    fn intersperse_section_hook(
        &mut self,
        module: &mut Module,
        _after: Option<SectionId>,
        before: Option<SectionId>,
    ) -> Result<(), ReencodeError<Self::Error>> {
        if matches!(
            before,
            Some(
                SectionId::Import
                    | SectionId::Function
                    | SectionId::Table
                    | SectionId::Memory
                    | SectionId::Tag
                    | SectionId::Global
                    | SectionId::Export
                    | SectionId::Start
                    | SectionId::Element
                    | SectionId::DataCount
                    | SectionId::Code
                    | SectionId::Data
            ) | None
        ) {
            self.emit_type(module);
        }

        if matches!(
            before,
            Some(
                SectionId::Table
                    | SectionId::Memory
                    | SectionId::Tag
                    | SectionId::Global
                    | SectionId::Export
                    | SectionId::Start
                    | SectionId::Element
                    | SectionId::DataCount
                    | SectionId::Code
                    | SectionId::Data
            ) | None
        ) {
            self.emit_function(module);
        }

        if matches!(
            before,
            Some(
                SectionId::Start
                    | SectionId::Element
                    | SectionId::DataCount
                    | SectionId::Code
                    | SectionId::Data
            ) | None
        ) {
            self.emit_export(module);
        }

        if matches!(before, Some(SectionId::Code | SectionId::Data) | None) {
            self.emit_data_count(module);
        }

        if matches!(before, Some(SectionId::Data) | None) {
            self.emit_code(module)?;
        }

        Ok(())
    }

    fn parse_code_section(
        &mut self,
        code: &mut CodeSection,
        section: wasmparser::CodeSectionReader<'_>,
    ) -> Result<(), ReencodeError<Self::Error>> {
        wasm_encoder::reencode::utils::parse_code_section(self, code, section)?;
        code.function(&self.memory_init_function()?);
        self.emitted_code = true;
        Ok(())
    }

    fn parse_data(
        &mut self,
        data: &mut DataSection,
        datum: Data<'_>,
    ) -> Result<(), ReencodeError<Self::Error>> {
        match datum.kind {
            DataKind::Active { .. } | DataKind::Passive => {
                data.passive(datum.data.iter().copied());
            }
        }
        Ok(())
    }
}

/// Return bytecode suitable for compilation with upstream Wasmtime.
pub(crate) fn prepare_contract_bytecode(
    bytecode: &[u8],
) -> io::Result<Cow<'_, [u8]>> {
    let plan = analyze(bytecode)?;

    if plan.has_reserved_export {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "contract exports reserved function `{MEMORY_INIT_EXPORT}`"
            ),
        ));
    }

    if !plan.needs_rewrite() {
        return Ok(Cow::Borrowed(bytecode));
    }

    if plan.has_start {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "contracts with start functions and active data segments cannot be rewritten safely",
        ));
    }

    validate_original_module(bytecode)?;

    let mut module = Module::new();
    let mut reencoder = MemoryInitReencoder::new(&plan);
    reencoder
        .parse_core_module(&mut module, Parser::new(0), bytecode)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;

    Ok(Cow::Owned(module.finish()))
}

/// Validate the source module before moving constant expressions into code.
fn validate_original_module(bytecode: &[u8]) -> io::Result<()> {
    Validator::new()
        .validate_all(bytecode)
        .map(|_| ())
        .map_err(invalid_wasm)
}

/// Scan a module for imports, functions, data segments, and reserved exports.
fn analyze(bytecode: &[u8]) -> io::Result<RewritePlan<'_>> {
    let mut plan = RewritePlan::default();

    for payload in Parser::new(0).parse_all(bytecode) {
        match payload.map_err(invalid_wasm)? {
            Payload::Version { .. } => {}
            Payload::TypeSection(section) => {
                plan.has_type_section = true;
                for group in section {
                    let group = group.map_err(invalid_wasm)?;
                    let group_type_count = u32::try_from(group.types().len())
                        .map_err(|_| too_many_types())?;
                    plan.type_count = plan
                        .type_count
                        .checked_add(group_type_count)
                        .ok_or_else(too_many_types)?;
                }
            }
            Payload::ImportSection(section) => {
                for import in section.into_imports() {
                    if matches!(
                        import.map_err(invalid_wasm)?.ty,
                        TypeRef::Func(_) | TypeRef::FuncExact(_)
                    ) {
                        plan.imported_func_count += 1;
                    }
                }
            }
            Payload::FunctionSection(section) => {
                plan.has_function_section = true;
                plan.defined_func_count = section.count();
            }
            Payload::ExportSection(section) => {
                plan.has_export_section = true;
                for export in section {
                    let export = export.map_err(invalid_wasm)?;
                    if export.name == MEMORY_INIT_EXPORT {
                        plan.has_reserved_export = true;
                    }
                }
            }
            Payload::StartSection { .. } => {
                plan.has_start = true;
            }
            Payload::DataCountSection { count, .. } => {
                plan.has_data_count = true;
                plan.data_count = count;
            }
            Payload::CodeSectionStart { .. } => {
                plan.has_code_section = true;
            }
            Payload::DataSection(section) => {
                plan.data_count = section.count();
                for (index, datum) in section.into_iter().enumerate() {
                    let datum = datum.map_err(invalid_wasm)?;
                    if let DataKind::Active {
                        memory_index,
                        offset_expr,
                    } = datum.kind
                    {
                        plan.active_segments.push(ActiveDataSegment {
                            index: index as u32,
                            memory_index,
                            offset_expr,
                            len: datum.data.len(),
                        });
                    }
                }
            }
            Payload::End(_) => {}
            other => {
                if let Some((id, _)) = other.as_section()
                    && id == u8::from(wasm_encoder::SectionId::Data)
                {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "unsupported data section encoding",
                    ));
                }
            }
        }
    }

    Ok(plan)
}

fn invalid_wasm(err: wasmparser::BinaryReaderError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, err)
}

fn too_many_types() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "too many type definitions")
}

#[cfg(test)]
mod tests {
    use dusk_wasmtime::{Engine, Module as WasmtimeModule};
    use wasm_encoder::{
        ConstExpr, DataSection, ExportKind, ExportSection, GlobalSection,
        GlobalType, MemorySection, MemoryType, Module, ValType,
    };

    use super::{MEMORY_INIT_EXPORT, prepare_contract_bytecode};

    #[test]
    fn rewrites_active_data_without_existing_function_sections() {
        let mut memory = MemorySection::new();
        memory.memory(MemoryType {
            minimum: 1,
            maximum: None,
            memory64: false,
            shared: false,
            page_size_log2: None,
        });

        let mut global = GlobalSection::new();
        global.global(
            GlobalType {
                val_type: ValType::I32,
                mutable: false,
                shared: false,
            },
            &ConstExpr::i32_const(0),
        );

        let mut exports = ExportSection::new();
        exports.export("memory", ExportKind::Memory, 0);
        exports.export("A", ExportKind::Global, 0);

        let mut data = DataSection::new();
        data.active(0, &ConstExpr::i32_const(0), [1, 2, 3]);

        let mut module = Module::new();
        module
            .section(&memory)
            .section(&global)
            .section(&exports)
            .section(&data);
        let bytecode = module.finish();

        let prepared = prepare_contract_bytecode(&bytecode).unwrap();
        let wasmtime_module =
            WasmtimeModule::new(&Engine::default(), prepared.as_ref()).unwrap();

        assert!(
            wasmtime_module
                .exports()
                .any(|export| export.name() == MEMORY_INIT_EXPORT)
        );
    }

    #[test]
    fn rejects_invalid_active_data_offset_expression() {
        let mut module = Module::new();
        empty_function_declaration(&mut module);

        let memory = memory_section(false);
        let code = empty_code_section();

        let mut data = DataSection::new();
        data.active(0, &ConstExpr::raw([0x10, 0x00, 0x0b]), [1, 2, 3]);

        module.section(&memory).section(&code).section(&data);
        let bytecode = module.finish();

        assert!(WasmtimeModule::new(&Engine::default(), &bytecode).is_err());

        let err = prepare_contract_bytecode(&bytecode).unwrap_err();

        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }
}
