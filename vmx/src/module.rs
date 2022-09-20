// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use crate::error::Error::{self, ParsingError};
use wasmparser::{DataKind, Operator, Parser, Payload};

use std::fmt;

impl fmt::Debug for VolatileMem {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "VolatileMem {{ offset: {:x}, length: {} }}",
            self.offset, self.length
        )
    }
}

// Workaround to save/restore re-initialized data
#[derive(Clone, Copy)]
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
            if let Payload::DataSection(datas) =
                payload.map_err(|e| ParsingError(Box::from(e)))?
            {
                for data in datas {
                    let data = data.map_err(|e| ParsingError(Box::from(e)))?;
                    if let DataKind::Active { offset_expr, .. } = data.kind {
                        let length = data.data.len();
                        let mut offset_expr_reader =
                            offset_expr.get_binary_reader();
                        let op =
                            offset_expr_reader.read_operator().expect("op");

                        if let Operator::I32Const { value } = op {
                            volatile.push(VolatileMem {
                                offset: value as usize,
                                length,
                            });
                        }
                    }
                }
            }
        }

        let module = wasmer::Module::new(&wasmer::Store::default(), bytecode)?;
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
