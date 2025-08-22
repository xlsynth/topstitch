// SPDX-License-Identifier: Apache-2.0

/// Represents how a module definition should be used when validating and/or
/// emitting Verilog.
#[derive(PartialEq, Default, Clone, Debug)]
pub enum Usage {
    /// When validating, validate the module definition and descend into its
    /// instances. When emitting Verilog, emit its definition and descend into
    /// its instances.
    #[default]
    EmitDefinitionAndDescend,

    /// When validating, do not validate the module definition and do not
    /// descend into its instances. When emitting Verilog, do not emit its
    /// definition and do not descend into its instances.
    EmitNothingAndStop,

    /// When validating, do not validate the module definition and do not
    /// descend into its instances. When emitting Verilog, emit a stub
    /// (interface only) and do not descend into its instances.
    EmitStubAndStop,

    /// When validating, do not validate the module definition and do not
    /// descend into its instances. When emitting Verilog, emit its definition
    /// but do not descend into its instances.
    EmitDefinitionAndStop,
}
