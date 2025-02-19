// SPDX-License-Identifier: Apache-2.0

use num_bigint::BigUint;

use crate::{ModDefCore, IO};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PortKey {
    ModDefPort {
        mod_def_name: String,
        port_name: String,
    },
    ModInstPort {
        mod_def_name: String,
        inst_name: String,
        port_name: String,
    },
}

impl PortKey {
    pub(crate) fn debug_string(&self) -> String {
        match &self {
            PortKey::ModDefPort {
                mod_def_name,
                port_name,
            } => format!("{}.{}", mod_def_name, port_name),
            PortKey::ModInstPort {
                mod_def_name,
                inst_name,
                port_name,
            } => format!("{}.{}.{}", mod_def_name, inst_name, port_name),
        }
    }

    pub(crate) fn variant_name(&self) -> &'static str {
        match self {
            PortKey::ModDefPort { .. } => "ModDef",
            PortKey::ModInstPort { .. } => "ModInst",
        }
    }

    pub(crate) fn retrieve_port_io(&self, mod_def_core: &ModDefCore) -> IO {
        match self {
            PortKey::ModDefPort { port_name, .. } => mod_def_core.ports[port_name].clone(),
            PortKey::ModInstPort {
                inst_name,
                port_name,
                ..
            } => mod_def_core.instances[inst_name].borrow().ports[port_name].clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DrivenPortBits {
    driven: BigUint,
    width: usize,
}

impl DrivenPortBits {
    pub(crate) fn new(width: usize) -> Self {
        DrivenPortBits {
            driven: BigUint::from(0u32),
            width,
        }
    }

    pub(crate) fn driven(&mut self, msb: usize, lsb: usize) -> Result<(), DrivenError> {
        let mut mask = (BigUint::from(1u32) << (msb - lsb + 1)) - BigUint::from(1u32);

        // make sure this is not already driven
        if (self.driven.clone() >> lsb) & mask.clone() != BigUint::from(0u32) {
            return Err(DrivenError::AlreadyDriven);
        };

        // mark the bits as driven
        mask <<= lsb;
        self.driven |= mask;

        Ok(())
    }

    pub(crate) fn all_driven(&self) -> bool {
        self.driven == (BigUint::from(1u32) << self.width) - BigUint::from(1u32)
    }

    pub(crate) fn example_problematic_bits(&self) -> Option<String> {
        example_problematic_bits(&self.driven, self.width)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DrivingPortBits {
    driving: BigUint,
    unused: BigUint,
    width: usize,
}

pub enum DrivenError {
    AlreadyDriven,
}

pub enum DrivingError {
    AlreadyMarkedUnused,
}

pub enum UnusedError {
    AlreadyMarkedUnused,
    AlreadyUsed,
}

impl DrivingPortBits {
    pub(crate) fn new(width: usize) -> Self {
        DrivingPortBits {
            driving: BigUint::from(0u32),
            unused: BigUint::from(0u32),
            width,
        }
    }

    pub(crate) fn driving(&mut self, msb: usize, lsb: usize) -> Result<(), DrivingError> {
        let mut mask = (BigUint::from(1u32) << (msb - lsb + 1)) - BigUint::from(1u32);

        // make sure nothing in this range is marked as unused
        if (self.unused.clone() >> lsb) & mask.clone() != BigUint::from(0u32) {
            return Err(DrivingError::AlreadyMarkedUnused);
        };

        // mark the bits as driving
        mask <<= lsb;
        self.driving |= mask;

        Ok(())
    }

    pub(crate) fn unused(&mut self, msb: usize, lsb: usize) -> Result<(), UnusedError> {
        let mut mask = (BigUint::from(1u32) << (msb - lsb + 1)) - BigUint::from(1u32);

        // make sure nothing in this range is marked as unused
        if (self.unused.clone() >> lsb) & mask.clone() != BigUint::from(0u32) {
            return Err(UnusedError::AlreadyMarkedUnused);
        };

        // make sure nothing in this range is marked as driving
        if (self.driving.clone() >> lsb) & mask.clone() != BigUint::from(0u32) {
            return Err(UnusedError::AlreadyUsed);
        };

        // mark the bits as unused
        mask <<= lsb;
        self.unused |= mask;

        Ok(())
    }

    pub(crate) fn all_driving_or_unused(&self) -> bool {
        (self.driving.clone() | self.unused.clone())
            == (BigUint::from(1u32) << self.width) - BigUint::from(1u32)
    }

    pub(crate) fn example_problematic_bits(&self) -> Option<String> {
        example_problematic_bits(&(self.driving.clone() | self.unused.clone()), self.width)
    }
}

fn example_problematic_bits(value: &BigUint, width: usize) -> Option<String> {
    let mut lsb = None;
    let mut msb = None;
    let mut found_problem = false;
    for i in 0..width {
        if (value.clone() >> i) & BigUint::from(1usize) == BigUint::from(0usize) {
            if found_problem {
                msb = Some(i);
            } else {
                lsb = Some(i);
                found_problem = true;
            }
        } else if found_problem {
            break;
        }
    }
    if found_problem {
        if msb.is_none() {
            msb = Some(width - 1);
        }
        if (msb.unwrap() - lsb.unwrap() + 1) == width {
            Some("".to_string())
        } else if lsb == msb {
            Some(format!("[{}]", lsb.unwrap()))
        } else {
            Some(format!("[{}:{}]", msb.unwrap(), lsb.unwrap()))
        }
    } else {
        None
    }
}
