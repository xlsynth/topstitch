// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result, bail, ensure};
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};
use xlsynth_pir::ir::{
    Block, BlockPort, InstantiationKind, NodePayload, NodeRef, Package, PackageMember, Type,
};
use xlsynth_pir::ir_parser::Parser;

use super::{ModDef, ModuleOrigin};
use crate::{IO, Usage};

/// Options for importing modules from an XLS Block IR package.
#[derive(Clone, Copy, Debug, Default)]
pub struct BlockIrParserConfig<'a> {
    /// Package-form XLS IR.
    pub ir: &'a str,
    /// Include nested block instances in imported module definitions.
    pub include_hierarchy: bool,
}

/// An extern instantiation declared in an XLS block hierarchy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockIrExternCall<'a> {
    /// Dot-separated instance path starting with the queried block.
    pub path: String,
    /// Name of the referenced IR function.
    pub function: &'a str,
    /// Decoded template bytes, if the function supplies an FFI code template.
    pub sv_template: Option<&'a [u8]>,
}

/// One verified XLS package shared by module and hierarchy queries.
pub struct BlockIrLibrary {
    package: Package,
    blocks: IndexMap<String, usize>,
    include_hierarchy: bool,
}

impl BlockIrLibrary {
    /// Parses and verifies a package once with the supplied import options.
    pub fn from_config(config: &BlockIrParserConfig<'_>) -> Result<Self> {
        let package = Parser::new(config.ir)
            .parse_and_verify_package()
            .context("could not import XLS Block IR package")?;
        let blocks = package
            .members
            .iter()
            .enumerate()
            .filter_map(|(index, member)| match member {
                PackageMember::Block(block) => Some((block.name.clone(), index)),
                _ => None,
            })
            .collect();
        Ok(Self {
            package,
            blocks,
            include_hierarchy: config.include_hierarchy,
        })
    }

    /// Parses a package with hierarchy extraction disabled.
    pub fn from_ir(ir: &str) -> Result<Self> {
        Self::from_config(&BlockIrParserConfig {
            ir,
            ..Default::default()
        })
    }

    /// Lists all blocks in package declaration order.
    pub fn module_names(&self) -> Vec<String> {
        self.blocks.keys().cloned().collect()
    }

    /// Imports the named block's interface and optional instance hierarchy.
    ///
    /// Nested definitions contain names only, like SV hierarchy import.
    /// Use [`Self::extern_calls`] to inspect references to foreign functions.
    pub fn module(&self, name: &str) -> Result<ModDef> {
        self.import_module(name, &mut HashMap::new())
    }

    /// Imports every block in declaration order, with a separate interface for each root.
    /// Within this result, occurrences of the same nested block share a definition,
    /// including changes to its definition-level metadata.
    pub fn modules(&self) -> Result<Vec<ModDef>> {
        let mut definitions = HashMap::new();
        self.blocks
            .keys()
            .map(|name| self.import_module(name, &mut definitions))
            .collect()
    }

    /// Reports one extern call per instance path under the named block, in depth-first
    /// declaration order. This query works with either setting of `include_hierarchy`.
    pub fn extern_calls(&self, name: &str) -> Result<Vec<BlockIrExternCall<'_>>> {
        let block = self.block(name)?;
        let mut templates = HashMap::new();
        for member in &self.package.members {
            if let PackageMember::Function(function) = member
                && let Some(template) = function
                    .ffi_proto()
                    .and_then(|ffi| ffi.code_template.as_deref())
            {
                templates.insert(function.name.as_str(), template);
            }
        }
        let mut calls = Vec::new();
        let mut path = vec![block.name.as_str()];
        self.collect_extern_calls(
            block,
            &templates,
            &mut path,
            &mut HashSet::new(),
            &mut calls,
        )?;
        Ok(calls)
    }

    /// Imports a root interface using the supplied cache for name-only descendants.
    fn import_module(
        &self,
        name: &str,
        definitions: &mut HashMap<String, ModDef>,
    ) -> Result<ModDef> {
        let block = self.block(name)?;
        let module = ModDef::import_block_interface(block)?;
        if self.include_hierarchy {
            self.populate_hierarchy(&module, block, definitions)?;
        }
        freeze_import(&module);
        Ok(module)
    }

    /// Looks up a block by its package member index.
    fn block(&self, name: &str) -> Result<&Block> {
        let index = self.blocks.get(name).with_context(|| {
            format!(
                "no block named '{name}' in XLS package '{}'",
                self.package.name
            )
        })?;
        let PackageMember::Block(block) = &self.package.members[*index] else {
            unreachable!()
        };
        Ok(block)
    }

    /// Copies instance names and references while sharing repeated block definitions.
    fn populate_hierarchy(
        &self,
        parent: &ModDef,
        block: &Block,
        definitions: &mut HashMap<String, ModDef>,
    ) -> Result<()> {
        for inst in &block.instantiations {
            if inst.kind == InstantiationKind::Extern {
                continue;
            }
            let child = if let Some(definition) = definitions.get(&inst.block) {
                definition
            } else {
                let definition = ModDef::new(&inst.block);
                self.populate_hierarchy(&definition, self.block(&inst.block)?, definitions)?;
                freeze_import(&definition);
                definitions.entry(inst.block.clone()).or_insert(definition)
            };
            parent.instantiate(child, Some(&inst.name), None);
        }
        Ok(())
    }

    /// Walks each call's instance path, skipping known call-free subtrees on later visits.
    fn collect_extern_calls<'a>(
        &'a self,
        block: &'a Block,
        templates: &HashMap<&str, &'a [u8]>,
        path: &mut Vec<&'a str>,
        extern_free: &mut HashSet<&'a str>,
        calls: &mut Vec<BlockIrExternCall<'a>>,
    ) -> Result<()> {
        if extern_free.contains(block.name.as_str()) {
            return Ok(());
        }
        let start = calls.len();
        for inst in &block.instantiations {
            path.push(&inst.name);
            match inst.kind {
                InstantiationKind::Block => {
                    self.collect_extern_calls(
                        self.block(&inst.block)?,
                        templates,
                        path,
                        extern_free,
                        calls,
                    )?;
                }
                InstantiationKind::Extern => {
                    calls.push(BlockIrExternCall {
                        path: path.join("."),
                        function: &inst.block,
                        sv_template: templates.get(inst.block.as_str()).copied(),
                    });
                }
            }
            path.pop();
        }
        if calls.len() == start {
            extern_free.insert(&block.name);
        }
        Ok(())
    }
}

impl ModDef {
    /// Imports a named XLS block as an external module from package-form Block IR.
    ///
    /// The block and port names must be simple identifiers matching the separately supplied SystemVerilog
    /// implementation. The parent emits ordinary instances of that module.
    /// A clock becomes a one-bit input. Data ports must be nonempty bit vectors
    /// without an SV type override. Ports retain their declaration order.
    ///
    /// Requires the `block-ir` Cargo feature.
    pub fn from_block_ir(name: &str, ir: &str) -> Result<Self> {
        BlockIrLibrary::from_ir(ir)?.module(name)
    }

    /// Creates an unfrozen module with the block's directly wireable interface.
    fn import_block_interface(block: &Block) -> Result<Self> {
        let name = &block.name;
        check_sv_name(name).with_context(|| format!("cannot instantiate XLS block '{name}'"))?;
        let module = Self::new(name);
        for port in &block.ports {
            let (port_name, io) = match port {
                BlockPort::Clock(clock) => (clock.as_str(), IO::Input(1)),
                BlockPort::Input(n) => (block.port_name(*n), IO::Input(port_width(block, *n)?)),
                BlockPort::Output(n) => (block.port_name(*n), IO::Output(port_width(block, *n)?)),
            };
            check_sv_name(port_name)
                .with_context(|| format!("cannot connect XLS port '{name}.{port_name}'"))?;
            module.add_port(port_name, io);
        }

        Ok(module)
    }
}

/// Marks a definition as an externally supplied, immutable module.
fn freeze_import(module: &ModDef) {
    let mut core = module.core.write();
    core.origin = ModuleOrigin::BlockIr;
    core.usage = Usage::EmitNothingAndStop;
}

/// Returns the vector width that can be wired directly in the SV wrapper.
fn port_width(block: &Block, node: NodeRef) -> Result<usize> {
    let name = block.port_name(node);
    let ty = block.port_type(node);
    let Type::Bits(width) = ty else {
        bail!(
            "{}.{name}: unsupported XLS port type {ty}; expected bits[N]",
            block.name
        );
    };
    ensure!(*width > 0, "{}.{name}: zero-width XLS port", block.name);
    if let NodePayload::InputPort {
        sv_type: Some(ty), ..
    }
    | NodePayload::OutputPort {
        sv_type: Some(ty), ..
    } = &block.get_node(node).payload
    {
        bail!("{}.{name}: unsupported SV type override '{ty}'", block.name);
    }
    Ok(*width)
}

/// TopStitch references external identifiers verbatim when emitting instances.
fn check_sv_name(name: &str) -> Result<()> {
    let mut bytes = name.bytes();
    ensure!(
        bytes
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == b'_')
            && bytes.all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'$'))
            && !xlsynth_vast::is_system_verilog_keyword(name),
        "'{name}' is not a simple SystemVerilog identifier"
    );
    Ok(())
}
