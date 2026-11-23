// SPDX-License-Identifier: Apache-2.0

use indexmap::map::Entry;
use std::sync::Arc;

use crate::port::PortDecl;
use crate::{IO, ModDef, ModDefCore, PackedLayout, Port, PortSlice};

impl ModDef {
    /// Adds a port to this module. Pass [`IO`](crate::IO) for plain bits or a
    /// declaration from a [`PackedLayout`](crate::PackedLayout) to keep its
    /// fields and array indices selectable. Both emit a flat port.
    ///
    /// ```
    /// use topstitch::{ConvertibleToPortSlice, IO, ModDef, PackageLookup, PackedLayout, ParserConfig, Source, VerilogLibrary};
    /// let sources = [Source::text(
    ///     "package p; typedef struct packed { logic valid; logic [2:0] data; } item_t; endpackage",
    /// )];
    /// let lib = VerilogLibrary::from_config(&ParserConfig { sources: &sources, ..Default::default() })?;
    /// let pkg = lib.package("p")?;
    /// let ty = pkg.ty("item_t")?;
    /// let item = PackedLayout::from_sv_type(&ty)?;
    /// let top = ModDef::new("Top");
    /// let enable = top.add_port("enable", IO::Input(1));
    /// let inputs = top.add_port("items", PackedLayout::array_of(&item, 4).input());
    /// assert_eq!(inputs.element(2).field("valid").to_port_slice(), inputs.bit(11));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn add_port(&self, name: impl AsRef<str>, decl: impl Into<PortDecl>) -> Port {
        if self.frozen() {
            panic!(
                "Module {} is frozen. wrap() first if modifications are needed.",
                self.core.read().name
            );
        }

        let decl = decl.into();
        let mut core = self.core.write();
        core.insert_port(name.as_ref(), decl);
        Port::ModDef {
            name: name.as_ref().to_string(),
            mod_def_core: Arc::downgrade(&self.core),
        }
    }

    /// Adds a port with a multidimensional packed declaration. Give the array dimensions
    /// outermost first and the direction and bit width of its innermost word. All bounds
    /// descend to zero. A one-bit word contributes no extra dimension.
    /// This declaration keeps its dimensions in generated SystemVerilog. Ordinary
    /// [`Self::add_port`] declarations and wrappers of imported ports emit flat vectors.
    /// Use [`Port::element`] for declared array indices and [`Port::slice`] for flat bit offsets.
    /// Panics for empty or zero-sized dimensions, zero-width words, or unrepresentable indices or widths.
    ///
    /// ```
    /// use topstitch::{ConvertibleToPortSlice, IO, ModDef};
    /// let top = ModDef::new("Readout");
    /// let lanes = top.add_port_array("lanes", IO::Input(8), [2, 3]); // [1:0][2:0][7:0]
    /// assert_eq!(lanes.element(1).to_port_slice(), lanes.slice(47, 24));
    /// assert_eq!(lanes.element_at([1, 0, 2]).to_port_slice(), lanes.bit(26));
    /// ```
    pub fn add_port_array(
        &self,
        name: impl AsRef<str>,
        element: IO,
        dimensions: impl AsRef<[usize]>,
    ) -> Port {
        let dimensions = dimensions.as_ref();
        assert!(
            !dimensions.is_empty(),
            "array ports require at least one dimension"
        );
        assert!(
            element.width() > 0,
            "array port word width must be positive"
        );
        let mut layout = PackedLayout::bits(1);
        if element.width() > 1 {
            layout = PackedLayout::array_of(&layout, element.width());
        }
        for &count in dimensions.iter().rev() {
            layout = PackedLayout::array_of(&layout, count);
        }
        let width = layout.width();
        i64::try_from(width).expect("array port width exceeds i64");
        self.add_port(
            name,
            PortDecl {
                shape: Some(layout.shape),
                emit_packed_dimensions: true,
                ..element.with_width(width).into()
            },
        )
    }

    /// Returns `true` if this module definition has a port with the given name.
    pub fn has_port(&self, name: impl AsRef<str>) -> bool {
        self.core.read().ports.contains_key(name.as_ref())
    }

    /// Returns the port on this module definition with the given name; panics
    /// if a port with that name does not exist.
    pub fn get_port(&self, name: impl AsRef<str>) -> Port {
        let inner = self.core.read();
        if inner.ports.contains_key(name.as_ref()) {
            Port::ModDef {
                name: name.as_ref().to_string(),
                mod_def_core: Arc::downgrade(&self.core),
            }
        } else {
            panic!("Port {}.{} does not exist", inner.name, name.as_ref())
        }
    }

    /// Returns a slice of the port on this module definition with the given
    /// name, from `msb` down to `lsb`, inclusive; panics if a port with that
    /// name does not exist.
    pub fn get_port_slice(&self, name: impl AsRef<str>, msb: usize, lsb: usize) -> PortSlice {
        self.get_port(name).slice(msb, lsb)
    }

    /// Returns a vector of all ports on this module definition with the given
    /// prefix. If `prefix` is `None`, returns all ports.
    pub fn get_ports(&self, prefix: Option<&str>) -> Vec<Port> {
        let inner = self.core.read();
        let mut result = Vec::new();
        for name in inner.ports.keys() {
            if prefix.is_none_or(|pfx| name.starts_with(pfx)) {
                result.push(Port::ModDef {
                    name: name.clone(),
                    mod_def_core: Arc::downgrade(&self.core),
                });
            }
        }
        result
    }
}

impl ModDefCore {
    /// Insert a port after checking its declaration name. The caller holds the write lock.
    pub(crate) fn insert_port(&mut self, name: &str, decl: PortDecl) {
        self.check_instance_name_collision(name);
        match self.ports.entry(name.to_string()) {
            Entry::Occupied(_) => panic!("Port {}.{name} already exists.", self.name),
            Entry::Vacant(entry) => {
                entry.insert(decl);
            }
        }
    }
}
