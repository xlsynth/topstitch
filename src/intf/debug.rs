// SPDX-License-Identifier: Apache-2.0

use crate::Intf;

impl std::fmt::Debug for Intf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mod_def_core = self.get_mod_def_core();
        let core = mod_def_core.borrow();
        match self {
            Intf::ModDef { name, .. } => {
                writeln!(f, "Interface Mapping:")?;
                for (func_name, (port_name, msb, lsb)) in core.interfaces.get(name).unwrap() {
                    writeln!(
                        f,
                        "{func_name}: (port_name: {port_name}, msb: {msb}, lsb: {lsb})",
                    )?;
                }
            }
            Intf::ModInst {
                inst_name,
                intf_name,
                ..
            } => {
                let inst_core = core.instances.get(inst_name).unwrap();
                let inst_binding = inst_core.borrow();
                writeln!(f, "Interface Mapping:")?;
                for (func_name, (port_name, msb, lsb)) in
                    inst_binding.interfaces.get(intf_name).unwrap()
                {
                    writeln!(
                        f,
                        "{func_name}: (port_name: {port_name}, msb: {msb}, lsb: {lsb})",
                    )?;
                }
            }
        };

        Ok(())
    }
}

impl Intf {
    pub(crate) fn debug_string(&self) -> String {
        match self {
            Intf::ModDef { name, .. } => {
                format!("{}.{}", self.get_mod_def_core().borrow().name, name)
            }
            Intf::ModInst {
                inst_name,
                intf_name,
                ..
            } => format!(
                "{}.{}.{}",
                self.get_mod_def_core().borrow().name,
                inst_name,
                intf_name
            ),
        }
    }
}
