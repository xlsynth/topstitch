// SPDX-License-Identifier: Apache-2.0

use indexmap::IndexMap;
use regex::Regex;
use std::rc::Rc;

use crate::{Intf, ModDef};

impl ModDef {
    /// Defines an interface with the given name. `mapping` is a map from
    /// function names to tuples of `(port_name, msb, lsb)`. For example, if
    /// `mapping` is `{"data": ("a_data", 3, 0), "valid": ("a_valid", 1, 1)}`,
    /// this defines an interface with two functions, `data` and `valid`, where
    /// the `data` function is provided by the port slice `a_data[3:0]` and the
    /// `valid` function is provided by the port slice `[1:1]`.
    pub fn def_intf(
        &self,
        name: impl AsRef<str>,
        mapping: IndexMap<String, (String, usize, usize)>,
    ) -> Intf {
        let mut core = self.core.borrow_mut();
        if core.interfaces.contains_key(name.as_ref()) {
            panic!(
                "Interface {} already exists in module {}",
                name.as_ref(),
                core.name
            );
        }
        core.interfaces.insert(name.as_ref().to_string(), mapping);
        Intf::ModDef {
            name: name.as_ref().to_string(),
            mod_def_core: Rc::downgrade(&self.core),
        }
    }

    /// Defines an interface with the given name, where the function names are
    /// derived from the port names by stripping a common prefix. For example,
    /// if the module has ports `a_data`, `a_valid`, `b_data`, and `b_valid`,
    /// calling `def_intf_from_prefix("a_intf", "a_")` will define an interface
    /// with functions `data` and `valid`, where `data` is provided by the full
    /// port `a_data` and `valid` is provided by the full port `a_valid`.
    pub fn def_intf_from_prefix(&self, name: impl AsRef<str>, prefix: impl AsRef<str>) -> Intf {
        self.def_intf_from_prefixes(name, &[prefix.as_ref()], true)
    }

    /// Defines an interface with the given name, where the function names are
    /// derived from the port names by stripping the prefix `<name>_`. For
    /// example, if the module has ports `a_data`, `a_valid`, `b_data`, and
    /// `b_valid`, calling `def_intf_from_prefix("a")` will define an
    /// interface with functions `data` and `valid`, where `data` is provided by
    /// the full port `a_data` and `valid` is provided by the full port
    /// `a_valid`.
    pub fn def_intf_from_name_underscore(&self, name: impl AsRef<str>) -> Intf {
        let prefix = format!("{}_", name.as_ref());
        self.def_intf_from_prefix(name, prefix)
    }

    /// Defines an interface with the given name, where the signals to be
    /// included are identified by those that start with one of the provided
    /// prefixies. Function names are either the signal names themselves (if
    /// `strip_prefix` is `false`) or by stripping the prefix (if `strip_prefix`
    /// is true). For example, if the module has ports `a_data`, `a_valid`,
    /// `b_data`, and `b_valid`, calling `def_intf_from_prefixes("intf", &["a_",
    /// "b_"], false)` will define an interface with functions `a_data`,
    /// `a_valid`, `b_data`, and `b_valid`, where each function is provided by
    /// the corresponding port.
    pub fn def_intf_from_prefixes(
        &self,
        name: impl AsRef<str>,
        prefixes: &[&str],
        strip_prefix: bool,
    ) -> Intf {
        let mut mapping = IndexMap::new();
        {
            let core = self.core.borrow();
            for port_name in core.ports.keys() {
                for prefix in prefixes {
                    if port_name.starts_with(prefix) {
                        let func_name = if strip_prefix {
                            port_name.strip_prefix(prefix).unwrap().to_string()
                        } else {
                            port_name.clone()
                        };
                        let port = self.get_port(port_name);
                        mapping.insert(func_name, (port_name.clone(), port.io().width() - 1, 0));
                        break;
                    }
                }
            }
        }

        assert!(
            !mapping.is_empty(),
            "Empty interface definition for {}.{}",
            self.get_name(),
            name.as_ref()
        );

        self.def_intf(name, mapping)
    }

    pub fn def_intf_from_regex(
        &self,
        name: impl AsRef<str>,
        search: impl AsRef<str>,
        replace: impl AsRef<str>,
    ) -> Intf {
        self.def_intf_from_regexes(name, &[(search.as_ref(), replace.as_ref())])
    }

    pub fn def_intf_from_regexes(&self, name: impl AsRef<str>, regexes: &[(&str, &str)]) -> Intf {
        let mut mapping = IndexMap::new();
        let regexes = regexes
            .iter()
            .map(|(search, replace)| {
                (
                    Regex::new(search).expect("Failed to compile regex"),
                    replace,
                )
            })
            .collect::<Vec<_>>();
        {
            let core = self.core.borrow();
            for port_name in core.ports.keys() {
                for (regex, replace) in &regexes {
                    if regex.is_match(port_name) {
                        let func_name = regex.replace(port_name, **replace).to_string();
                        let port = self.get_port(port_name);
                        mapping.insert(func_name, (port_name.clone(), port.io().width() - 1, 0));
                        break;
                    }
                }
            }
        }

        assert!(
            !mapping.is_empty(),
            "Empty interface definition for {}.{}",
            self.get_name(),
            name.as_ref()
        );

        self.def_intf(name, mapping)
    }

    /// Returns the interface with the given name; panics if an interface with
    /// that name does not exist.
    pub fn get_intf(&self, name: impl AsRef<str>) -> Intf {
        let core = self.core.borrow();
        if core.interfaces.contains_key(name.as_ref()) {
            Intf::ModDef {
                name: name.as_ref().to_string(),
                mod_def_core: Rc::downgrade(&self.core),
            }
        } else {
            panic!(
                "Interface '{}' does not exist in module '{}'",
                name.as_ref(),
                core.name
            );
        }
    }

    /// Returns `true` if this module definition has an interface with the given
    /// name.
    pub fn has_intf(&self, name: impl AsRef<str>) -> bool {
        self.core.borrow().interfaces.contains_key(name.as_ref())
    }

    /// Returns a vector of all interfaces on this module definition with the
    /// given prefix. If `prefix` is `None`, returns all interfaces.
    pub fn get_intfs(&self, prefix: Option<&str>) -> Vec<Intf> {
        let inner = self.core.borrow();
        let mut result = Vec::new();
        for name in inner.interfaces.keys() {
            if prefix.is_none_or(|pfx| name.starts_with(pfx)) {
                result.push(Intf::ModDef {
                    name: name.clone(),
                    mod_def_core: Rc::downgrade(&self.core),
                });
            }
        }
        result
    }
}
