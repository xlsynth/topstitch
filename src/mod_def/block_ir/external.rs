// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

use anyhow::{Context, Result, ensure};
use indexmap::IndexMap;
use parking_lot::RwLock;
use xlsynth_pir::ir::{OpaqueExternPort, OpaqueExternPortKind, Package, Type};
use xlsynth_pir::{BInstantiation, BlockBuilder};

use super::super::ModuleOrigin;
use super::super::core::BlockIrModel;
use crate::port::PortType;
use crate::{IO, ModDef, ModDefCore};

pub(super) type DefinitionId = *const RwLock<ModDefCore>;

/// The complete interface, optional behavioral model, and SV-specific external configuration.
#[derive(Hash, PartialEq, Eq)]
pub(super) struct ExternalBinding {
    module: String,
    ports: Vec<OpaqueExternPort>,
    parameters: BTreeMap<String, String>,
    input_casts: BTreeMap<String, String>,
    model: Option<String>,
}

impl ExternalBinding {
    /// Declares an external instance with its retained model, overrides, and input casts.
    pub(super) fn instantiate(
        &self,
        builder: &mut BlockBuilder,
        package: &Package,
        instance_name: &str,
    ) -> Result<BInstantiation> {
        let instance = if let Some(model) = &self.model {
            let model = package.get_block(model).with_context(|| {
                format!(
                    "external module {} has no linked model {model}",
                    self.module
                )
            })?;
            builder.instantiate_extern_block_with_parameters(
                instance_name,
                model,
                &self.module,
                self.parameters.clone(),
            )?
        } else {
            builder.instantiate_opaque_extern_block_with_parameters(
                instance_name,
                &self.module,
                self.ports.clone(),
                self.parameters.clone(),
            )?
        };
        for (name, sv_type) in &self.input_casts {
            builder.set_instantiation_input_cast(instance, name, sv_type)?;
        }
        Ok(instance)
    }

    /// Captures the elaborated interface and every override, including behavior-only parameters.
    fn from_module(core: &ModDefCore) -> Result<Self> {
        check_identifier(&core.name)?;
        let parameters = core
            .sv_parameter_bindings()
            .map_err(anyhow::Error::msg)?
            .into_iter()
            .collect::<BTreeMap<_, _>>();
        for name in parameters.keys() {
            check_identifier(name)?;
        }
        let mut ports = Vec::with_capacity(core.ports.len());
        let mut input_casts = BTreeMap::new();
        for (name, decl) in &core.ports {
            let io = &decl.io;
            check_identifier(name)?;
            ensure!(
                io.width() > 0,
                "zero-width external port {}.{name}",
                core.name
            );
            if let PortType::EnumInputCast(sv_type) = &decl.ty {
                for identifier in sv_type.split("::") {
                    check_identifier(identifier)?;
                }
                if matches!(io, IO::Input(_)) {
                    input_casts.insert(name.clone(), sv_type.clone());
                }
            }
            let ty = Type::Bits(io.width());
            let kind = match io {
                IO::Input(_) => OpaqueExternPortKind::Input(ty),
                IO::Output(_) => OpaqueExternPortKind::Output(ty),
                IO::InOut(_) => OpaqueExternPortKind::Inout(ty),
            };
            ports.push(OpaqueExternPort {
                name: name.clone(),
                kind,
            });
        }
        Ok(Self {
            module: core.name.clone(),
            ports,
            parameters,
            input_casts,
            model: behavioral_model(core).map(|m| m.block.clone()),
        })
    }
}

/// Returns an external model whose clock can be wired with native PIR semantics.
/// Clocked models use an opaque interface so their clock remains an ordinary TopStitch connection.
pub(super) fn behavioral_model(core: &ModDefCore) -> Option<&BlockIrModel> {
    let ModuleOrigin::External(external) = &core.origin else {
        return None;
    };
    let model = external.model.as_ref()?;
    let block = model.package.get_block(&model.block)?;
    block.clock_port_name().is_none().then_some(model)
}

/// Indexes external module interfaces without inventing behavioral package members.
pub(super) fn bind_externals(
    blocks: &IndexMap<String, ModDef>,
    leaves: &IndexMap<DefinitionId, ModDef>,
) -> Result<HashMap<DefinitionId, Rc<ExternalBinding>>> {
    let mut bindings = HashMap::new();
    let mut variants = HashSet::<Rc<ExternalBinding>>::new();
    for (&id, leaf) in leaves {
        let core = leaf.core.read();
        ensure!(
            !blocks.contains_key(&core.name),
            "{} has both an external implementation and a generated block",
            core.name
        );
        let candidate = Rc::new(
            ExternalBinding::from_module(&core)
                .with_context(|| format!("binding external module {}", core.name))?,
        );
        let binding = if let Some(binding) = variants.get(&candidate) {
            Rc::clone(binding)
        } else {
            variants.insert(Rc::clone(&candidate));
            candidate
        };
        bindings.insert(id, binding);
    }
    Ok(bindings)
}

/// Rejects identifiers that cannot appear as simple SV module or port names.
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
