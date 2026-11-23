// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use anyhow::{Context, Result, ensure};
use indexmap::IndexMap;
use parking_lot::RwLock;
use xlsynth_pir::FnBuilder;
use xlsynth_pir::ir::{Package, PackageMember, Type};
use xlsynth_pir::ir_value_utils::zero_ir_value_for_type;

use crate::port::PortType;
use crate::{IO, ModDef, ModDefCore};

pub(super) type DefinitionId = *const RwLock<ModDefCore>;

/// The concrete function and its mapping from SV port names to PIR extern ports.
pub(super) struct ExternalBinding {
    pub function: usize,
    pub ports: HashMap<String, String>,
}

/// Adds one FFI function per specialization and indexes it by each referring definition.
pub(super) fn bind_externals(
    package: &mut Package,
    blocks: &IndexMap<String, ModDef>,
    leaves: &IndexMap<DefinitionId, ModDef>,
) -> Result<HashMap<DefinitionId, Rc<ExternalBinding>>> {
    let mut bindings = HashMap::new();
    if leaves.is_empty() {
        return Ok(bindings);
    }
    let mut variants = HashMap::new();
    let mut reserved: HashSet<_> = blocks.keys().cloned().collect();
    for leaf in leaves.values() {
        let name = leaf.get_name();
        ensure!(
            !blocks.contains_key(&name),
            "{name} has both an external implementation and a generated block"
        );
        reserved.insert(name);
    }
    let mut names = (0..)
        .map(|i| format!("ts_external_{i}"))
        .filter(|name| !reserved.contains(name));
    for (&id, leaf) in leaves {
        let core = leaf.core.read();
        let sig = Signature::from_module(&core)
            .with_context(|| format!("binding external module {}", core.name))?;
        let binding = match variants.entry(sig) {
            std::collections::hash_map::Entry::Occupied(entry) => Rc::clone(entry.get()),
            std::collections::hash_map::Entry::Vacant(entry) => {
                let binding = Rc::new(entry.key().add_function(&names.next().unwrap(), package)?);
                entry.insert(binding.clone());
                binding
            }
        };
        bindings.insert(id, binding);
    }
    Ok(bindings)
}

#[derive(Hash, PartialEq, Eq)]
struct Signature {
    module: String,
    parameters: Vec<(String, String)>,
    inputs: Vec<ExternalPort>,
    outputs: Vec<ExternalPort>,
}

#[derive(Hash, PartialEq, Eq)]
struct ExternalPort {
    name: String,
    width: usize,
    enum_type: Option<String>,
}

impl Signature {
    /// Captures the elaborated interface and every override, including behavior-only parameters.
    fn from_module(core: &ModDefCore) -> Result<Self> {
        check_identifier(&core.name)?;
        let parameters = core.sv_parameter_bindings().map_err(anyhow::Error::msg)?;
        for (name, _) in &parameters {
            check_identifier(name)?;
        }
        let mut inputs = Vec::new();
        let mut outputs = Vec::new();
        for (name, decl) in &core.ports {
            let io = &decl.io;
            check_identifier(name)?;
            ensure!(
                io.width() > 0,
                "zero-width external port {}.{name}",
                core.name
            );
            let enum_type = match &decl.ty {
                PortType::EnumInputCast(name) => Some(name.clone()),
                _ => None,
            };
            if let Some(ty) = &enum_type {
                for identifier in ty.split("::") {
                    check_identifier(identifier)?;
                }
            }
            let port = ExternalPort {
                name: name.clone(),
                width: io.width(),
                enum_type,
            };
            match io {
                IO::Input(_) => inputs.push(port),
                IO::Output(_) => outputs.push(port),
                IO::InOut(_) => anyhow::bail!(
                    "inout port {}.{name} cannot be represented in Block IR",
                    core.name
                ),
            }
        }
        inputs.sort_by(|a, b| a.name.cmp(&b.name));
        outputs.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Self {
            module: core.name.clone(),
            parameters,
            inputs,
            outputs,
        })
    }

    /// Builds the packed return signature and attaches the SV instance template used by block2sv.
    fn add_function(&self, name: &str, package: &mut Package) -> Result<ExternalBinding> {
        let mut builder = FnBuilder::new(name);
        let mut ports = HashMap::new();
        let mut connections = Vec::new();
        for (i, port) in self.inputs.iter().enumerate() {
            let arg = format!("arg{i}");
            builder.param(&arg, Type::Bits(port.width))?;
            let mut expr = format!("{{{arg}}}");
            if let Some(ty) = &port.enum_type {
                expr = format!("{ty}'({expr})");
            }
            connections.push(format!(".{}({expr})", port.name));
            ports.insert(port.name.clone(), arg);
        }
        for (i, port) in self.outputs.iter().enumerate() {
            let result = if self.outputs.len() == 1 {
                "return".into()
            } else {
                format!("return.{i}")
            };
            connections.push(format!(".{}({{{result}}})", port.name));
            ports.insert(port.name.clone(), result);
        }
        let return_type = match self.outputs.as_slice() {
            [output] => Type::Bits(output.width),
            outputs => Type::Tuple(
                outputs
                    .iter()
                    .map(|p| Box::new(Type::Bits(p.width)))
                    .collect(),
            ),
        };
        // PIR requires a function body. Only the FFI template specifies the real implementation.
        let zero = builder.literal(zero_ir_value_for_type(&return_type))?;
        builder.set_name(zero, "unmodeled_external_result")?;
        let mut function = builder.build_in_package(zero, package)?;
        let mut template = self.module.clone();
        if !self.parameters.is_empty() {
            let overrides = self
                .parameters
                .iter()
                .map(|(name, literal)| {
                    // Literal braces in a string parameter must survive FFI placeholder substitution.
                    let literal = literal
                        .replace('{', "{{")
                        .replace('}', "}}")
                        // PIR scans the template for this marker even inside an SV string.
                        .replace("xlsynth_pir_ext=", r"xlsynth_pir_ext\075");
                    format!(".{name}({literal})")
                })
                .collect::<Vec<_>>()
                .join(", ");
            template.push_str(&format!(" #({overrides})"));
        }
        template.push_str(&format!(" {{fn}} ({});", connections.join(", ")));
        // The triple-quoted IR token contains a protobuf TextFormat string.
        let escaped = template.replace('\\', "\\\\").replace('"', "\\\"");
        let attr = format!(
            r#"#[ffi_proto("""code_template: "{escaped}"
""")]"#
        );
        function
            .outer_attrs
            .push(attr.parse().map_err(anyhow::Error::msg)?);
        let index = package.members.len();
        package.members.push(PackageMember::Function(function));
        Ok(ExternalBinding {
            function: index,
            ports,
        })
    }
}

/// FFI substitutes raw SV text, so every interpolated identifier must stand on its own.
fn check_identifier(name: &str) -> Result<()> {
    let mut chars = name.chars();
    ensure!(
        chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '$')),
        "unsupported SystemVerilog identifier {name:?} in external binding"
    );
    Ok(())
}
