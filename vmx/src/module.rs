// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use crate::types::Error;
use wasmparser::{DataKind, Operator, Parser, Payload};

// Workaround to save/restore re-initialized data
#[derive(Debug, Clone, Copy)]
pub struct VolatileMem {
    pub offset: usize,
    pub length: usize,
}

pub struct WrappedModule {
    serialized: Vec<u8>,
    volatile: Vec<VolatileMem>,
}

impl WrappedModule {
    pub fn new(bytecode: &[u8]) -> Result<Self, Error> {
        let mut volatile = vec![];

        for payload in Parser::new(0).parse_all(bytecode) {
            match payload? {
                Payload::DataSection(datas) => {
                    for data in datas {
                        let data = data?;
                        if let DataKind::Active { offset_expr, .. } = data.kind
                        {
                            let length = data.data.len();
                            let mut offset_expr_reader =
                                offset_expr.get_binary_reader();
                            let op =
                                offset_expr_reader.read_operator().expect("op");

                            if let Operator::I32Const { value } = op {
                                let vol = VolatileMem {
                                    offset: value as usize,
                                    length,
                                };

                                println!("volatile {:?}", vol);

                                volatile.push(vol);
                            }
                        }
                    }
                }
                _ => (),
            }
        }

        let module =
            wasmer::Module::new(&mut wasmer::Store::default(), bytecode)?;
        let serialized = module.serialize()?;

        Ok(WrappedModule {
            serialized,
            volatile,
        })
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.serialized
    }

    pub fn volatile(&self) -> &Vec<VolatileMem> {
        &self.volatile
    }
}
