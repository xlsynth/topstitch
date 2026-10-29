// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::ops::Index;

use slang_rs::{ConstantValue, Error as SlangError, Type};

impl From<slang_rs::Package> for Package {
    fn from(pkg: slang_rs::Package) -> Self {
        Self {
            name: pkg.name,
            parameters: pkg.parameters,
            types: pkg.types,
        }
    }
}

/// Evaluated constants and typedefs from one SystemVerilog package.
///
/// Checked integer conversions respect signedness and reject X/Z bits and
/// values outside the requested type's range.
#[derive(Debug, PartialEq)]
pub struct Package {
    pub name: String,
    /// Evaluated parameters and localparams, keyed by name. Typedefs are in
    /// `types`. Integer values support checked `TryFrom` conversions.
    pub parameters: HashMap<String, ConstantValue>,
    /// Resolved typedefs. An unsupported type has its own error so it does not
    /// prevent access to the package's other members.
    pub types: HashMap<String, Result<Type, SlangError>>,
}

impl Index<&str> for Package {
    type Output = ConstantValue;

    fn index(&self, key: &str) -> &Self::Output {
        &self.parameters[key]
    }
}

impl Package {
    /// Get a parameter's value. Returns `None` for a missing name or typedef.
    pub fn get(&self, key: &str) -> Option<&ConstantValue> {
        self.parameters.get(key)
    }
}
