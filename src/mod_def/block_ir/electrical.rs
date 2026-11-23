// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result, bail, ensure};
use indexmap::{IndexMap, IndexSet};
use xlsynth_pir::ir::Type;
use xlsynth_pir::{BInoutNet, BInstantiation, BValue, BlockBuilder, IrBits, IrValue};

use super::super::net_names::NetNames;
use crate::connection::resolve::ResolvedNet;
use crate::{ConnectedItem, IO, Port, PortSlice};

#[derive(Clone, Copy)]
struct NetBit {
    net: BInoutNet,
    bit: usize,
    width: usize,
}

/// A contiguous selection of one resolved net, in that net's bit coordinates.
struct NetRange {
    net: BInoutNet,
    msb: usize,
    lsb: usize,
    net_width: usize,
}

impl NetRange {
    /// Returns whether the selection covers the full net.
    fn is_whole(&self) -> bool {
        self.lsb == 0 && self.msb + 1 == self.net_width
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BitDriver {
    Port { index: usize, bit: usize },
    Constant(bool),
}

struct NamedElectricalNet {
    bits: Vec<Option<usize>>,
}

/// A union-find over inout port bits before any PIR net is allocated.
struct ElectricalGraph {
    bases: HashMap<Port, usize>,
    parents: Vec<usize>,
    drivers: Vec<Option<BitDriver>>,
    driver_ports: IndexSet<Port>,
    named: IndexMap<String, NamedElectricalNet>,
    named_aliases: Vec<(String, usize, usize, usize)>,
}

/// PIR allocation state shared while lowering one module's electrical components.
struct ElectricalEmitter<'a> {
    builder: &'a mut BlockBuilder,
    sources: &'a mut IndexMap<Port, BValue>,
    drivers: &'a [Option<BitDriver>],
    driver_ports: &'a IndexSet<Port>,
    occupants: Vec<Option<NetBit>>,
    reads: HashMap<BInoutNet, BValue>,
    enabled: Option<BValue>,
}

/// Resolves and emits electrical connectivity, returning values for explicitly named nets.
pub(super) fn emit(
    builder: &mut BlockBuilder,
    names: &mut NetNames,
    pins: &[(Port, Option<BInstantiation>)],
    requests: &[(PortSlice, String)],
    sources: &mut IndexMap<Port, BValue>,
) -> Result<HashMap<String, BValue>> {
    let mut graph = ElectricalGraph::new(pins);
    graph.extract(pins)?;
    graph.complete_named_bits(requests)?;
    for bit in 0..graph.parents.len() {
        graph.parents[bit] = graph.root(bit);
    }
    let root_by_bit = std::mem::take(&mut graph.parents);
    for (name, bit, a, b) in std::mem::take(&mut graph.named_aliases) {
        ensure!(
            root_by_bit[a] == root_by_bit[b],
            "requested electrical net {name}[{bit}] refers to disconnected bits"
        );
    }
    let requests = std::mem::take(&mut graph.named);
    for (name, request) in &requests {
        ensure!(
            request.bits.iter().all(Option::is_some),
            "requested electrical net {name} has bits outside its inout connections; PIR cannot preserve the requested net name"
        );
    }
    let mut class_drivers = vec![None; root_by_bit.len()];
    let mut public = vec![false; root_by_bit.len()];
    for (bit, &root) in root_by_bit.iter().enumerate() {
        if let Some(driver) = graph.drivers[bit] {
            if let Some(previous) = class_drivers[root] {
                ensure!(
                    previous == driver,
                    "conflicting drivers on an electrical net"
                );
            }
            class_drivers[root] = Some(driver);
        }
    }
    for (port, instance) in pins {
        if instance.is_none() {
            let base = graph.bases[port];
            for &root in &root_by_bit[base..base + port.io().width()] {
                public[root] = true;
            }
        }
    }
    let mut emitter = ElectricalEmitter {
        builder,
        sources,
        drivers: &class_drivers,
        driver_ports: &graph.driver_ports,
        occupants: vec![None; root_by_bit.len()],
        reads: HashMap::new(),
        enabled: None,
    };
    let mut named = HashMap::new();
    for (name, request) in requests {
        let roots = request
            .bits
            .into_iter()
            .map(|bit| root_by_bit[bit.unwrap()])
            .collect::<Vec<_>>();
        ensure!(
            roots.iter().all(|root| !public[*root]),
            "requested net name {name} aliases a public inout; PIR codegen cannot preserve both names"
        );
        ensure!(
            distinct_with_uniform_drive_state(&roots, &class_drivers),
            "requested electrical net {name} is partially driven or has electrically tied slices; PIR cannot preserve the requested net name"
        );
        ensure!(
            roots.iter().all(|root| emitter.occupants[*root].is_none()),
            "conflicting requested electrical net names for {name}"
        );
        let net = emitter.allocate(&name, &roots)?;
        named.insert(name, emitter.read(net)?);
    }
    let mut generated = 0;
    for (port, _) in pins {
        let base = graph.bases[port];
        let mut bit = 0;
        while bit < port.io().width() {
            let root = root_by_bit[base + bit];
            if emitter.occupants[root].is_some() {
                bit += 1;
                continue;
            }
            let mut roots = vec![root];
            let mut seen = HashSet::from([root]);
            let driven = class_drivers[root].is_some();
            while bit + roots.len() < port.io().width() {
                let next = root_by_bit[base + bit + roots.len()];
                if emitter.occupants[next].is_some()
                    || !seen.insert(next)
                    || class_drivers[next].is_some() != driven
                {
                    break;
                }
                roots.push(next);
            }
            let base_name = format!("ts_inout_{generated}");
            generated += 1;
            let name = names.reserve_generated(&base_name).unwrap_or(base_name);
            emitter.allocate(&name, &roots)?;
            bit += roots.len();
        }
    }
    for (port, instance) in pins {
        let base = graph.bases[port];
        let bits = root_by_bit[base..base + port.io().width()]
            .iter()
            .map(|root| emitter.occupants[*root].expect("allocated electrical bit"))
            .collect::<Vec<_>>();
        emitter.bind_and_sample(port, *instance, &bits)?;
    }
    Ok(named)
}

impl ElectricalEmitter<'_> {
    /// Allocates one whole PIR net and immediately emits its optional directed source.
    fn allocate(&mut self, name: &str, roots: &[usize]) -> Result<BInoutNet> {
        let width = roots.len();
        let net = self.builder.declare_inout_net(name, Type::Bits(width))?;
        for (bit, root) in roots.iter().enumerate() {
            self.occupants[*root] = Some(NetBit { net, bit, width });
        }
        if self.drivers[roots[0]].is_some() {
            let data = self.drive_value(roots)?;
            let enabled = if let Some(enabled) = self.enabled {
                enabled
            } else {
                let enabled = self.builder.literal(IrValue::make_ubits(1, 1)?)?;
                self.enabled = Some(enabled);
                enabled
            };
            self.builder.ext_inout_drive(net, data, enabled)?;
        }
        Ok(net)
    }

    /// Builds a whole-net value from contiguous slices of directional sources or constants.
    fn drive_value(&mut self, roots: &[usize]) -> Result<BValue> {
        let mut pieces = Vec::new();
        let mut lo = 0;
        while lo < roots.len() {
            let mut hi = lo + 1;
            while hi < roots.len()
                && compatible_drivers(self.drivers[roots[hi - 1]], self.drivers[roots[hi]])
            {
                hi += 1;
            }
            let value = match self.drivers[roots[lo]].expect("whole electrical net is driven") {
                BitDriver::Port { index, bit } => {
                    let port = &self.driver_ports[index];
                    let source = *self
                        .sources
                        .get(port)
                        .with_context(|| format!("missing driver {}", port.debug_string()))?;
                    if bit == 0 && hi - lo == port.io().width() {
                        source
                    } else {
                        self.builder.bit_slice(source, bit, hi - lo)?
                    }
                }
                BitDriver::Constant(_) => {
                    let mut bytes = vec![0u8; (hi - lo).div_ceil(8)];
                    for (i, root) in roots[lo..hi].iter().enumerate() {
                        if matches!(self.drivers[*root], Some(BitDriver::Constant(true))) {
                            bytes[i / 8] |= 1 << (i % 8);
                        }
                    }
                    self.builder
                        .literal(IrValue::Bits(IrBits::from_le_bytes(hi - lo, &bytes)?))?
                }
            };
            pieces.push(value);
            lo = hi;
        }
        pieces.reverse();
        Ok(if pieces.len() == 1 {
            pieces[0]
        } else {
            self.builder.concat(&pieces)?
        })
    }

    /// Returns a cached directional sample of a resolved electrical net.
    fn read(&mut self, net: BInoutNet) -> Result<BValue> {
        if let Some(read) = self.reads.get(&net) {
            Ok(*read)
        } else {
            let read = self.builder.ext_inout_read(net)?;
            self.reads.insert(net, read);
            Ok(read)
        }
    }

    /// Binds one pin passively and records its directional sample using the same bit ranges.
    fn bind_and_sample(
        &mut self,
        port: &Port,
        instance: Option<BInstantiation>,
        bits: &[NetBit],
    ) -> Result<()> {
        let ranges = inout_ranges(bits);
        let binding = make_inout_binding(&ranges);
        if let Some(instance) = instance {
            self.builder
                .bind_inout_net(instance, port.name(), binding)?;
        } else {
            self.builder
                .ext_inout_port(port.name(), Type::Bits(bits.len()), binding)?;
        }
        let mut values = Vec::with_capacity(ranges.len());
        for range in ranges {
            let read = self.read(range.net)?;
            values.push(if range.is_whole() {
                read
            } else {
                self.builder
                    .bit_slice(read, range.lsb, range.msb - range.lsb + 1)?
            });
        }
        let value = if values.len() == 1 {
            values[0]
        } else {
            self.builder.concat(&values)?
        };
        self.sources.insert(port.clone(), value);
        Ok(())
    }
}

impl ElectricalGraph {
    /// Reserves a stable, contiguous bit range for each bidirectional port.
    fn new(pins: &[(Port, Option<BInstantiation>)]) -> Self {
        let mut bases = HashMap::new();
        let mut count = 0;
        for (port, _) in pins {
            bases.insert(port.clone(), count);
            count += port.io().width();
        }
        Self {
            bases,
            parents: (0..count).collect(),
            drivers: vec![None; count],
            driver_ports: indexmap::IndexSet::new(),
            named: IndexMap::new(),
            named_aliases: Vec::new(),
        }
    }

    /// Returns an electrical component's root while compressing its path.
    fn root(&mut self, mut bit: usize) -> usize {
        while self.parents[bit] != bit {
            self.parents[bit] = self.parents[self.parents[bit]];
            bit = self.parents[bit];
        }
        bit
    }

    /// Unites corresponding electrical bits without introducing a direction.
    fn unite(&mut self, a: usize, b: usize) {
        let a = self.root(a);
        let b = self.root(b);
        if a != b {
            self.parents[b] = a;
        }
    }

    /// Resolves each local electrical component once using the shared emission validator.
    fn extract(&mut self, pins: &[(Port, Option<BInstantiation>)]) -> Result<()> {
        let mut covered = vec![false; self.parents.len()];
        for (port, _) in pins {
            let base = self.bases[port];
            if covered[base..base + port.io().width()].iter().all(|v| *v) {
                continue;
            }
            let connections = port
                .get_port_connections()
                .with_context(|| format!("{} is unconnected", port.debug_string()))?;
            for chunk in connections
                .read()
                .resolved_chunks(&port.io(), &port.debug_string())
                .map_err(anyhow::Error::msg)?
            {
                let this = &chunk[0].this;
                let at = base + this.lsb;
                let width = this.width();
                if covered[at..at + width].iter().all(|v| *v) {
                    continue;
                }
                let net = chunk.resolve_net().map_err(anyhow::Error::msg)?;
                covered[at..at + width].fill(true);
                // The resolver already follows every local alias and aligns each peer to this
                // chunk. Unite all inout peers before skipping any reached pin on later passes.
                for connection in &chunk {
                    let ConnectedItem::PortSlice(peer) = &connection.other else {
                        continue;
                    };
                    if !matches!(peer.port.io(), IO::InOut(_)) {
                        continue;
                    }
                    let peer_base = *self.bases.get(&peer.port).with_context(|| {
                        format!("unknown inout endpoint {}", peer.debug_string())
                    })?;
                    for offset in 0..width {
                        self.unite(at + offset, peer_base + peer.lsb + offset);
                        covered[peer_base + peer.lsb + offset] = true;
                    }
                }
                match net {
                    ResolvedNet::Ports { driver, wire, .. } => {
                        if let Some(wire) = wire {
                            self.record_name(&wire.name, wire.width, wire.lsb, at, width)?;
                        }
                        if let Some(driver) = driver {
                            if output_requires_electrical_binding(&driver.port)? {
                                bail!(
                                    "{} and {} must share an electrical net; PIR cannot bind an instance output pin directly to an inout net",
                                    driver.debug_string(),
                                    this.debug_string(),
                                );
                            }
                            let (index, _) = self.driver_ports.insert_full(driver.port.clone());
                            for offset in 0..width {
                                self.record_driver(
                                    at + offset,
                                    BitDriver::Port {
                                        index,
                                        bit: driver.lsb + offset,
                                    },
                                )?;
                            }
                        }
                    }
                    ResolvedNet::Terminal(ConnectedItem::Tieoff(tieoff)) => {
                        let (_, bytes) = tieoff.value.to_bytes_le();
                        for offset in 0..width {
                            let value = bytes
                                .get(offset / 8)
                                .is_some_and(|byte| byte & (1 << (offset % 8)) != 0);
                            self.record_driver(at + offset, BitDriver::Constant(value))?;
                        }
                    }
                    ResolvedNet::Terminal(ConnectedItem::Unused(_)) => {}
                    ResolvedNet::Terminal(_) => {
                        unreachable!("a resolved terminal is a tieoff or unused marker")
                    }
                }
            }
        }
        Ok(())
    }

    /// Extends a named electrical net to cover requested lanes used only by directional ports.
    fn complete_named_bits(&mut self, requests: &[(PortSlice, String)]) -> Result<()> {
        for (slice, name) in requests {
            if !self.named.contains_key(name) {
                continue;
            }
            if slice.port.is_driver() {
                let instance = slice.port.get_mod_inst().expect("named instance output");
                let module = instance.get_mod_def();
                let native = !module.is_external_leaf()
                    || module.imported_block_ir_package().is_some_and(|package| {
                        package
                            .get_block(&module.get_name())
                            .is_some_and(|block| block.clock_port_name().is_none())
                    });
                ensure!(
                    native && slice.lsb == 0 && slice.width() == slice.port.io().width(),
                    "requested electrical net {name} cannot bind output {} directly; PIR codegen requires a whole native block output",
                    slice.debug_string()
                );
            }
            let connections = slice
                .port
                .get_port_connections()
                .with_context(|| format!("{} is unconnected", slice.port.debug_string()))?;
            for chunk in connections
                .read()
                .resolved_chunks(&slice.port.io(), &slice.port.debug_string())
                .map_err(anyhow::Error::msg)?
            {
                let ResolvedNet::Ports {
                    driver: Some(driver),
                    wire: Some(wire),
                    ..
                } = chunk.resolve_net().map_err(anyhow::Error::msg)?
                else {
                    continue;
                };
                if wire.name != *name {
                    continue;
                }
                let (index, _) = self.driver_ports.insert_full(driver.port.clone());
                for offset in 0..chunk[0].this.width() {
                    if self.named[name].bits[wire.lsb + offset].is_some() {
                        continue;
                    }
                    let bit = self.parents.len();
                    self.parents.push(bit);
                    self.drivers.push(Some(BitDriver::Port {
                        index,
                        bit: driver.lsb + offset,
                    }));
                    self.record_name(name, wire.width, wire.lsb + offset, bit, 1)?;
                }
            }
        }
        Ok(())
    }

    /// Records the full requested SV wire width and its mapped electrical bit offsets.
    fn record_name(
        &mut self,
        name: &str,
        width: usize,
        lsb: usize,
        at: usize,
        len: usize,
    ) -> Result<()> {
        let request = self
            .named
            .entry(name.to_owned())
            .or_insert_with(|| NamedElectricalNet {
                bits: vec![None; width],
            });
        ensure!(
            request.bits.len() == width,
            "conflicting widths for requested electrical net {name}"
        );
        for offset in 0..len {
            let id = at + offset;
            let slot = &mut request.bits[lsb + offset];
            if let Some(previous) = *slot {
                if previous != id {
                    self.named_aliases
                        .push((name.to_owned(), lsb + offset, previous, id));
                }
            } else {
                *slot = Some(id);
            }
        }
        Ok(())
    }

    /// Records a directed source without silently replacing an existing distinct driver.
    fn record_driver(&mut self, bit: usize, driver: BitDriver) -> Result<()> {
        if let Some(previous) = self.drivers[bit] {
            ensure!(
                previous == driver,
                "conflicting drivers on an electrical net"
            );
        } else {
            self.drivers[bit] = Some(driver);
        }
        Ok(())
    }
}

/// Adjacent net bits can share one local driver only when their directed sources line up.
fn compatible_drivers(a: Option<BitDriver>, b: Option<BitDriver>) -> bool {
    match (a, b) {
        (None, None) | (Some(BitDriver::Constant(_)), Some(BitDriver::Constant(_))) => true,
        (
            Some(BitDriver::Port { index: a, bit }),
            Some(BitDriver::Port {
                index: b,
                bit: next,
            }),
        ) => a == b && bit + 1 == next,
        _ => false,
    }
}

/// A named net cannot duplicate an electrical bit or mix driven and undriven connected lanes.
fn distinct_with_uniform_drive_state(roots: &[usize], drivers: &[Option<BitDriver>]) -> bool {
    let mut seen = HashSet::new();
    roots.iter().all(|r| seen.insert(*r))
        && roots
            .iter()
            .all(|r| drivers[*r].is_some() == drivers[roots[0]].is_some())
}

/// Coalesces consecutive electrical bits into MSB-first net ranges.
fn inout_ranges(bits: &[NetBit]) -> Vec<NetRange> {
    let mut ranges: Vec<NetRange> = Vec::new();
    for bit in bits.iter().rev() {
        if let Some(range) = ranges.last_mut()
            && range.net == bit.net
            && range.lsb == bit.bit + 1
        {
            range.lsb = bit.bit;
        } else {
            ranges.push(NetRange {
                net: bit.net,
                msb: bit.bit,
                lsb: bit.bit,
                net_width: bit.width,
            });
        }
    }
    ranges
}

/// Converts aligned net ranges to XLSynth's whole/select/concat passive binding.
fn make_inout_binding(ranges: &[NetRange]) -> xlsynth_pir::ir::InoutBinding<BInoutNet> {
    use xlsynth_pir::ir::{InoutBinding, InoutPiece};
    let pieces = ranges
        .iter()
        .map(|range| {
            if range.is_whole() {
                InoutPiece::Whole(range.net)
            } else {
                InoutPiece::Select {
                    net: range.net,
                    hi: range.msb,
                    lo: range.lsb,
                }
            }
        })
        .collect::<Vec<_>>();
    if pieces.len() == 1 {
        InoutBinding::Piece(pieces.into_iter().next().unwrap())
    } else {
        InoutBinding::Concat(pieces)
    }
}

/// Checks whether a child can observe the resolved value of its output wire.
/// A directed drive cannot preserve readback through that electrical connection.
fn output_requires_electrical_binding(port: &Port) -> Result<bool> {
    let Some(instance) = port.get_mod_inst() else {
        return Ok(false);
    };
    let module = instance.get_mod_def();
    // An opaque external output is a directional source unless its owner supplies
    // additional electrical readback semantics.
    if module.is_external_leaf() {
        return Ok(false);
    }
    let output = module.get_port(port.name());
    let connections = output
        .get_port_connections()
        .with_context(|| format!("{} is unconnected", output.debug_string()))?;
    let chunks = connections
        .read()
        .resolved_chunks(&output.io(), &output.debug_string())
        .map_err(anyhow::Error::msg)?;
    for chunk in chunks {
        // Only an output chosen as the shared internal net can pass a parent drive
        // back into the child. An assignment to a separate output is directional.
        if !matches!(chunk.resolve_net().map_err(anyhow::Error::msg)?.sv_expression(),
            Some(ConnectedItem::PortSlice(ref slice)) if slice.port == output)
        {
            continue;
        }
        // An output wire may be read inside the child, including through a second
        // output or an inout pin. A parent drive can change what those readers see.
        if (&chunk)
            .into_iter()
            .filter_map(|connection| match &connection.other {
                ConnectedItem::PortSlice(slice) => Some(slice),
                _ => None,
            })
            .any(|slice| match (&slice.port, slice.port.io()) {
                (Port::ModInst { .. }, IO::Input(_) | IO::InOut(_)) => true,
                (Port::ModDef { .. }, IO::Output(_) | IO::InOut(_)) => slice.port != output,
                _ => false,
            })
        {
            return Ok(true);
        }
        if let ResolvedNet::Ports {
            driver: Some(driver),
            ..
        } = chunk.resolve_net().map_err(anyhow::Error::msg)?
            && output_requires_electrical_binding(&driver.port)?
        {
            return Ok(true);
        }
    }
    Ok(false)
}
