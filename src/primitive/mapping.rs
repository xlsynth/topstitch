// SPDX-License-Identifier: Apache-2.0

use crate::primitive::DELAY_STAGES_PORT;

use parking_lot::{Mutex, RwLock};
use std::collections::{HashMap, HashSet, hash_map::Entry};
use std::hash::Hash;
use std::sync::Arc;
use std::thread::{self, ThreadId};

use num_bigint::BigUint;

use super::tree::{Tree, fold_tree};
use crate::{ConvertibleToPortSlice, IO, ModDef, PortSlice, Primitive, ResetKind};

/// Policy for logic created directly in this definition. Child definitions choose independently.
#[derive(Clone, Default)]
pub enum PrimitivePolicy {
    /// Keep bitwise operations, mux data, and registers word-wide.
    #[default]
    Allow,
    /// Build a structural implementation using this library. Missing rules are errors.
    RequireMapping(GateMap),
    /// Accept wiring (including concatenation and zero-cycle delays) and ordinary module instances.
    Forbid,
}

/// Pin names of a one-bit signal mux. High select selects `when_true`.
pub struct Mux2Pins<'a> {
    pub select: &'a str,
    pub when_true: &'a str,
    pub when_false: &'a str,
    pub out: &'a str,
}

/// D and Q are one bit for a scalar flop and full width for a register bank or delay.
#[derive(Clone, Copy)]
pub struct FlopPins<'a> {
    pub d: &'a str,
    pub clk: &'a str,
    pub q: &'a str,
    pub reset: Option<&'a str>,
}

/// Clock and reset behavior of a cell. Scalar bindings use a fixed reset bit;
/// a width-aware factory can request the full reset value for each operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FlopFlavor<R = bool> {
    falling: bool,
    reset: Option<(ResetKind, R)>,
}

impl FlopFlavor {
    /// A positive-edge flip-flop without reset.
    pub fn posedge() -> Self {
        Self {
            falling: false,
            reset: None,
        }
    }
    /// A negative-edge flip-flop without reset.
    pub fn negedge() -> Self {
        Self {
            falling: true,
            reset: None,
        }
    }
    /// Adds a reset which drives every mapped bit to the specified value.
    pub fn reset(mut self, kind: ResetKind, value: bool) -> Self {
        self.reset = Some((kind, value));
        self
    }
    /// Matches this reset kind at any value and passes the full unsigned word to the factory.
    pub fn reset_by_value(self, kind: ResetKind) -> FlopFlavor<()> {
        FlopFlavor {
            falling: self.falling,
            reset: Some((kind, ())),
        }
    }
}

/// Selects the factory signature for [`GateMap::flop_by_width`] from its reset specification.
pub trait FlopByWidthFactory<F> {
    #[doc(hidden)]
    fn register(self, map: &mut GateMap, pins: FlopPins<'_>, make: F) -> Result<(), String>;
}

impl<F> FlopByWidthFactory<F> for FlopFlavor
where
    F: Fn(usize) -> Result<ModDef, String> + Send + Sync + 'static,
{
    fn register(self, map: &mut GateMap, pins: FlopPins<'_>, make: F) -> Result<(), String> {
        let rule = SequentialRule::new(
            pins,
            self.reset.is_some(),
            Arc::new(make) as Arc<WidthFactory>,
        )?;
        let Entry::Vacant(entry) = Arc::make_mut(&mut map.0).wide_flops.entry(self) else {
            return Err(format!("duplicate width-aware flop mapping for {self:?}"));
        };
        entry.insert(rule);
        Ok(())
    }
}

impl<F> FlopByWidthFactory<F> for FlopFlavor<()>
where
    F: Fn(usize, &BigUint) -> Result<ModDef, String> + Send + Sync + 'static,
{
    fn register(self, map: &mut GateMap, pins: FlopPins<'_>, make: F) -> Result<(), String> {
        let rule = SequentialRule::new(pins, true, Arc::new(make) as Arc<ResetFactory>)?;
        let Entry::Vacant(entry) = Arc::make_mut(&mut map.0).resettable_banks.entry(self) else {
            return Err(format!("duplicate width-aware flop mapping for {self:?}"));
        };
        entry.insert(rule);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Gate {
    And,
    Or,
    Xor,
    Inv,
    Mux,
    Flop(FlopFlavor),
}

#[derive(Clone)]
struct Binding {
    cell: ModDef,
    inputs: Vec<String>,
    output: String,
}

impl Binding {
    /// Validates the complete interface up front, including duplicate, extra and missing pins.
    fn new(
        cell: ModDef,
        inputs: &[(&str, usize)],
        output: &str,
        width: usize,
        allow_extra_outputs: bool,
    ) -> Result<Self, String> {
        let core = cell.core.read();
        if core.primitive.is_some() {
            return Err("mapping target must be an ordinary module".into());
        }
        let mut used = HashSet::new();
        for (name, io) in inputs
            .iter()
            .map(|(p, w)| (*p, IO::Input(*w)))
            .chain([(output, IO::Output(width))])
        {
            if !used.insert(name) || core.ports.get(name).map(|decl| &decl.io) != Some(&io) {
                return Err(format!(
                    "{}.{}: expected a unique {io:?} pin",
                    core.name, name
                ));
            }
        }
        for (name, decl) in &core.ports {
            if !used.contains(name.as_str())
                && !(allow_extra_outputs && matches!(decl.io, IO::Output(_)))
            {
                return Err(format!(
                    "{}.{} is unbound (use a wrapper for additional inputs)",
                    core.name, name
                ));
            }
        }
        drop(core);
        Ok(Self {
            cell,
            inputs: inputs.iter().map(|(name, _)| (*name).into()).collect(),
            output: output.into(),
        })
    }

    /// Wires a physical cell and returns its output for composition.
    fn instantiate(&self, parent: &ModDef, name: &str, inputs: &[PortSlice]) -> PortSlice {
        let inst = parent.instantiate(&self.cell, Some(name), None);
        for (pin, src) in self.inputs.iter().zip(inputs) {
            inst.get_port(pin).connect(src);
        }
        for pin in self.cell.get_ports(None) {
            if matches!(pin.io(), IO::Output(_)) && pin.name() != self.output {
                inst.get_port(pin.name()).unused();
            }
        }
        inst.get_port(&self.output).to_port_slice()
    }
}

type WidthFactory = dyn Fn(usize) -> Result<ModDef, String> + Send + Sync;
type ResetFactory = dyn Fn(usize, &BigUint) -> Result<ModDef, String> + Send + Sync;
type DelayFactory = dyn Fn(usize, usize) -> Result<ModDef, String> + Send + Sync;

type ActiveFactoryCalls<K> = Mutex<HashSet<(ThreadId, K)>>;

/// Keeps a rule's exact arguments active on the calling thread until its factory returns or unwinds.
struct FactoryCall<'a, K: Eq + Hash> {
    calls: &'a ActiveFactoryCalls<K>,
    key: (ThreadId, K),
}

impl<'a, K: Eq + Hash + Clone> FactoryCall<'a, K> {
    /// Rejects recursion while allowing independent threads to build the same binding.
    fn enter(calls: &'a ActiveFactoryCalls<K>, key: &K) -> Result<Self, String> {
        let key = (thread::current().id(), key.clone());
        if !calls.lock().insert(key.clone()) {
            return Err("recursive mapping factory for the same arguments".into());
        }
        Ok(Self { calls, key })
    }
}

impl<K: Eq + Hash> Drop for FactoryCall<'_, K> {
    fn drop(&mut self) {
        self.calls.lock().remove(&self.key);
    }
}

#[derive(Clone)]
struct SequentialRule<F, K> {
    cache: Arc<RwLock<HashMap<K, Binding>>>,
    active_calls: Arc<ActiveFactoryCalls<K>>,
    make: F,
    d: String,
    clk: String,
    q: String,
    reset: Option<String>,
    stages: Option<String>,
}

impl<F, K: Eq + Hash + Clone> SequentialRule<F, K> {
    /// Copies the cell interface from the library registration.
    fn new(pins: FlopPins<'_>, has_reset: bool, make: F) -> Result<Self, String> {
        if pins.reset.is_some() != has_reset {
            return Err("reset pin does not match sequential cell flavor".into());
        }
        Ok(Self {
            cache: Default::default(),
            active_calls: Default::default(),
            make,
            d: pins.d.into(),
            clk: pins.clk.into(),
            q: pins.q.into(),
            reset: pins.reset.map(str::to_owned),
            stages: None,
        })
    }

    /// Shares a validated physical definition across all uses of this rule and argument list.
    /// Call the user factory outside the lock; concurrent builders use the same winning binding.
    fn bind(
        &self,
        parent: &ModDef,
        key: K,
        width: usize,
        delay_depth: Option<usize>,
        make: impl FnOnce() -> Result<ModDef, String>,
    ) -> Result<Binding, String> {
        if let Some(binding) = self.cache.read().get(&key) {
            return Ok(binding.clone());
        }
        let mut inputs = vec![(self.d.as_str(), width), (self.clk.as_str(), 1)];
        inputs.extend(self.reset.as_deref().map(|p| (p, 1)));
        let cell = {
            let _call = FactoryCall::enter(&self.active_calls, &key)?;
            make()?
        };
        parent.check_mapping_hierarchy(&cell)?;
        let binding = Binding::new(cell, &inputs, &self.q, width, delay_depth.is_some())?;
        if let (Some(stages), Some(depth)) = (&self.stages, delay_depth) {
            let stage_width = depth
                .checked_add(1)
                .and_then(|words| words.checked_mul(width))
                .ok_or("delay tap bus width overflow")?;
            let core = binding.cell.core.read();
            if stages == &binding.output
                || core.ports.get(stages).map(|decl| &decl.io) != Some(&IO::Output(stage_width))
            {
                return Err(format!(
                    "{}.{stages} must be a distinct output of width {stage_width}",
                    core.name
                ));
            }
        }
        Ok(self.cache.write().entry(key).or_insert(binding).clone())
    }
}

#[derive(Clone, Default)]
struct Rules {
    gates: HashMap<Gate, Binding>,
    wide_flops: HashMap<FlopFlavor, SequentialRule<Arc<WidthFactory>, usize>>,
    resettable_banks: HashMap<FlopFlavor<()>, SequentialRule<Arc<ResetFactory>, (usize, BigUint)>>,
    delays: HashMap<FlopFlavor, SequentialRule<Arc<DelayFactory>, (usize, usize)>>,
}

/// A library of scalar gates, register banks, and whole-delay implementations.
/// Finish defining the ports of generated cells before binding them. All occurrences
/// share the supplied definition, including cells returned by a factory.
#[derive(Clone, Default)]
pub struct GateMap(Arc<Rules>);

impl GateMap {
    /// Creates a library with no implicit mappings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Maps bitwise AND and AND reductions.
    pub fn and2(&mut self, cell: ModDef, inputs: [&str; 2], out: &str) -> Result<(), String> {
        self.gate(Gate::And, cell, &inputs, out)
    }
    /// Maps bitwise OR and OR reductions.
    pub fn or2(&mut self, cell: ModDef, inputs: [&str; 2], out: &str) -> Result<(), String> {
        self.gate(Gate::Or, cell, &inputs, out)
    }
    /// Maps bitwise XOR and parity reductions.
    pub fn xor2(&mut self, cell: ModDef, inputs: [&str; 2], out: &str) -> Result<(), String> {
        self.gate(Gate::Xor, cell, &inputs, out)
    }
    /// Maps bitwise inversion.
    pub fn inv(&mut self, cell: ModDef, input: &str, out: &str) -> Result<(), String> {
        self.gate(Gate::Inv, cell, &[input], out)
    }
    /// Maps a regular data mux. The select pin is shared across all lanes.
    pub fn mux2(&mut self, cell: ModDef, pins: Mux2Pins<'_>) -> Result<(), String> {
        self.gate(
            Gate::Mux,
            cell,
            &[pins.select, pins.when_false, pins.when_true],
            pins.out,
        )
    }
    /// Registers a scalar sequential implementation. Each bit must match its reset value.
    pub fn flop(
        &mut self,
        flavor: FlopFlavor,
        cell: ModDef,
        pins: FlopPins<'_>,
    ) -> Result<(), String> {
        if pins.reset.is_some() != flavor.reset.is_some() {
            return Err("reset pin does not match flop flavor".into());
        }
        let mut inputs = vec![pins.d, pins.clk];
        inputs.extend(pins.reset);
        self.gate(Gate::Flop(flavor), cell, &inputs, pins.q)
    }

    /// Registers one full-width register bank for a flop or each stage of a delay.
    /// With a plain or fixed-bit flavor the factory takes `width`. With
    /// [`FlopFlavor::reset_by_value`] it takes `(width, &BigUint)`, including mixed reset words.
    /// For uniform resets a matching fixed-value bank takes precedence over the general bank.
    /// A whole-delay mapping takes precedence for delays. Scalar flop bindings are the fallback.
    pub fn flop_by_width<F>(
        &mut self,
        flavor: impl FlopByWidthFactory<F>,
        pins: FlopPins<'_>,
        make: F,
    ) -> Result<(), String> {
        flavor.register(self, pins, make)
    }

    /// Registers a whole-word, whole-depth delay factory, called with (width, cycles).
    /// All stages must implement the declared flavor. Extra status outputs are left unused.
    /// A uniform reset value may use this rule. Other resets use the matching register-bank or scalar flop rules.
    pub fn delay(
        &mut self,
        flavor: FlopFlavor,
        pins: FlopPins<'_>,
        make: impl Fn(usize, usize) -> Result<ModDef, String> + Send + Sync + 'static,
    ) -> Result<(), String> {
        let rule = SequentialRule::new(
            pins,
            flavor.reset.is_some(),
            Arc::new(make) as Arc<DelayFactory>,
        )?;
        let Entry::Vacant(entry) = Arc::make_mut(&mut self.0).delays.entry(flavor) else {
            return Err(format!("duplicate delay mapping for {flavor:?}"));
        };
        entry.insert(rule);
        Ok(())
    }

    /// Registers a delay whose stage output contains the input at word zero and every registered stage.
    pub fn delay_with_stages(
        &mut self,
        flavor: FlopFlavor,
        pins: FlopPins<'_>,
        stages: &str,
        make: impl Fn(usize, usize) -> Result<ModDef, String> + Send + Sync + 'static,
    ) -> Result<(), String> {
        self.delay(flavor, pins, make)?;
        Arc::make_mut(&mut self.0)
            .delays
            .get_mut(&flavor)
            .unwrap()
            .stages = Some(stages.into());
        Ok(())
    }

    /// Adds a checked scalar pin binding.
    fn gate(&mut self, gate: Gate, cell: ModDef, inputs: &[&str], out: &str) -> Result<(), String> {
        let inputs: Vec<_> = inputs.iter().map(|p| (*p, 1)).collect();
        let binding = Binding::new(cell, &inputs, out, 1, false)?;
        let Entry::Vacant(entry) = Arc::make_mut(&mut self.0).gates.entry(gate) else {
            return Err(format!("duplicate mapping for {gate:?}"));
        };
        entry.insert(binding);
        Ok(())
    }

    /// Looks up the required cell for this library.
    fn get(&self, g: Gate) -> Result<&Binding, String> {
        self.0
            .gates
            .get(&g)
            .ok_or_else(|| format!("missing mapping for {g:?}"))
    }

    /// Constructs the shared definition after all library bindings have been resolved.
    fn build_definition(
        &self,
        parent: &ModDef,
        op: &Primitive,
        width: usize,
    ) -> Result<ModDef, String> {
        let def = ModDef::new(format!(
            "{}__{}",
            parent.get_name(),
            op.definition_name(width)
        ));
        let final_only_delay = match op {
            Primitive::Delay { falling, reset, .. } => {
                let flavor = FlopFlavor {
                    falling: *falling,
                    reset: reset.as_ref().map(|(k, v)| (*k, v.bit(0))),
                };
                let uniform = reset
                    .as_ref()
                    .is_none_or(|(_, v)| v.bits() == 0 || v.count_ones() == width as u64);
                uniform
                    && self
                        .0
                        .delays
                        .get(&flavor)
                        .is_some_and(|rule| rule.stages.is_none())
            }
            _ => false,
        };
        for (pin, io) in op.ports(width) {
            if pin != DELAY_STAGES_PORT || !final_only_delay {
                def.add_port(pin, io);
            }
        }
        op.annotate_clock_ports(&def);
        self.implement(parent, &def, op, width)?;
        parent.check_mapping_hierarchy(&def)?;
        {
            let mut core = def.core.write();
            core.primitive = Some(op.clone());
            core.primitives.mapped_body = true;
        }
        Ok(def)
    }

    /// Builds a detached structural body; failure cannot add a partially mapped parent instance.
    fn implement(
        &self,
        parent: &ModDef,
        def: &ModDef,
        op: &Primitive,
        width: usize,
    ) -> Result<(), String> {
        use Primitive::*;
        let output = def.get_port(op.output());
        match op {
            And | Or | Xor | Not | ReduceAnd | ReduceOr | ReduceXor => {
                let gate = match op {
                    And | ReduceAnd => Gate::And,
                    Or | ReduceOr => Gate::Or,
                    Xor | ReduceXor => Gate::Xor,
                    _ => Gate::Inv,
                };
                let binding = self.get(gate)?;
                let a = def.get_port("a");
                if matches!(op, ReduceAnd | ReduceOr | ReduceXor) {
                    let bits: Vec<_> = (0..width).map(|i| a.bit(i)).collect();
                    // Unary reduction of Z is X, including a one-bit input.
                    if width == 1 {
                        let identity = u8::from(matches!(op, ReduceAnd));
                        let inst = def.instantiate(&binding.cell, Some("reduce"), None);
                        inst.get_port(&binding.inputs[0]).connect(&bits[0]);
                        inst.get_port(&binding.inputs[1]).tieoff(identity);
                        output.connect(&inst.get_port(&binding.output));
                    } else {
                        let mut serial = 0;
                        let root = fold_tree(&bits, Tree::Reduction, None, &mut |_, a, b| {
                            let name = format!("node_{serial}");
                            serial += 1;
                            binding.instantiate(def, &name, &[a, b])
                        });
                        output.connect(&root);
                    }
                } else {
                    let b = (!matches!(op, Not)).then(|| def.get_port("b"));
                    for i in 0..width {
                        let mut args = vec![a.bit(i)];
                        args.extend(b.as_ref().map(|p| p.bit(i)));
                        output.bit(i).connect(&binding.instantiate(
                            def,
                            &format!("bit_{i}"),
                            &args,
                        ));
                    }
                }
            }
            Select {
                index_width,
                arms,
                fallback,
            } => {
                let sel = def.get_port("sel");
                let mux = if *arms == 0 {
                    None
                } else {
                    Some(self.get(Gate::Mux)?)
                };
                let data: Vec<_> = (0..*arms).map(|i| def.get_port(format!("d{i}"))).collect();
                let fill = fallback.then(|| def.get_port("fallback"));
                if arms.is_power_of_two()
                    && arms.trailing_zeros() as usize == *index_width
                    && let Some(pin) = &fill
                {
                    pin.unused();
                }
                if *arms == 0 {
                    sel.unused();
                }
                for bit in 0..width {
                    let leaves: Vec<_> = data.iter().map(|p| p.bit(bit)).collect();
                    let mut serial = 0;
                    let root = fold_tree(
                        &leaves,
                        Tree::Select {
                            index_width: *index_width,
                        },
                        fill.as_ref().map(|p| p.bit(bit)).as_ref(),
                        &mut |k, lo, hi| {
                            let name = format!("bit_{bit}_mux_{serial}");
                            serial += 1;
                            mux.unwrap().instantiate(def, &name, &[sel.bit(k), lo, hi])
                        },
                    );
                    output.bit(bit).connect(&root);
                }
            }
            Flop { falling, reset } | Delay { falling, reset, .. } => {
                let depth = if let Delay { cycles, .. } = op {
                    *cycles
                } else {
                    1
                };
                let d = def.get_port(if matches!(op, Delay { .. }) {
                    "in"
                } else {
                    "d"
                });
                let clk = def.get_port("clk").to_port_slice();
                let rst = reset.as_ref().map(|_| def.get_port("rst").to_port_slice());
                let flavor = |i: usize| FlopFlavor {
                    falling: *falling,
                    reset: reset.as_ref().map(|(k, v)| (*k, v.bit(i as u64))),
                };
                // Fixed-bit bank and delay rules apply only to unreset, all-clear, or all-set words.
                let uniform = match reset {
                    None => Some(flavor(0)),
                    Some((_, value)) if value.bits() == 0 || value.count_ones() == width as u64 => {
                        Some(flavor(0))
                    }
                    _ => None,
                };
                if matches!(op, Delay { .. })
                    && let Some(rule) = uniform.and_then(|f| self.0.delays.get(&f))
                {
                    let binding = rule.bind(parent, (width, depth), width, Some(depth), || {
                        (rule.make)(width, depth)
                    })?;
                    let mut args = vec![d.to_port_slice(), clk];
                    args.extend(rst);
                    let inst = def.instantiate(&binding.cell, Some("delay"), None);
                    for (pin, src) in binding.inputs.iter().zip(&args) {
                        inst.get_port(pin).connect(src);
                    }
                    for pin in binding.cell.get_ports(None) {
                        if !matches!(pin.io(), IO::Output(_)) {
                            continue;
                        }
                        if pin.name() == binding.output {
                            output.connect(&inst.get_port(pin.name()));
                        } else if rule.stages.as_deref() == Some(pin.name()) {
                            def.get_port(DELAY_STAGES_PORT)
                                .connect(&inst.get_port(pin.name()));
                        } else {
                            inst.get_port(pin.name()).unused();
                        }
                    }
                    return Ok(());
                }
                let bank = if let Some(rule) = uniform.and_then(|f| self.0.wide_flops.get(&f)) {
                    Some(rule.bind(parent, width, width, None, || (rule.make)(width))?)
                } else if let Some((kind, value)) = reset
                    && let Some(rule) = self.0.resettable_banks.get(&FlopFlavor {
                        falling: *falling,
                        reset: Some((*kind, ())),
                    })
                {
                    Some(rule.bind(parent, (width, value.clone()), width, None, || {
                        (rule.make)(width, value)
                    })?)
                } else {
                    None
                };
                if let Some(binding) = bank {
                    let mut inputs = vec![d.to_port_slice(), clk];
                    inputs.extend(rst);
                    let stages =
                        matches!(op, Delay { .. }).then(|| def.get_port(DELAY_STAGES_PORT));
                    if let Some(stages) = &stages {
                        stages.slice(width - 1, 0).connect(&inputs[0]);
                    }
                    for stage in 0..depth {
                        inputs[0] = binding.instantiate(def, &format!("stage_{stage}"), &inputs);
                        if let Some(stages) = &stages {
                            stages
                                .slice((stage + 2) * width - 1, (stage + 1) * width)
                                .connect(&inputs[0]);
                        }
                    }
                    output.connect(&inputs[0]);
                } else {
                    for bit in 0..width {
                        let binding = self.get(Gate::Flop(flavor(bit)))?;
                        let mut q = d.bit(bit);
                        let stages =
                            matches!(op, Delay { .. }).then(|| def.get_port(DELAY_STAGES_PORT));
                        if let Some(stages) = &stages {
                            stages.bit(bit).connect(&q);
                        }
                        for stage in 0..depth {
                            let mut args = vec![q, clk.clone()];
                            args.extend(rst.clone());
                            q = binding.instantiate(
                                def,
                                &format!("stage_{stage}_bit_{bit}"),
                                &args,
                            );
                            if let Some(stages) = &stages {
                                stages.bit((stage + 1) * width + bit).connect(&q);
                            }
                        }
                        output.bit(bit).connect(&q);
                    }
                }
            }
            Concat { .. } => unreachable!("wiring is independent of gate libraries"),
        }
        Ok(())
    }
}

impl ModDef {
    /// Sets this definition's policy before its first logic instance. Wiring never locks the policy.
    pub fn set_primitive_policy(&self, policy: PrimitivePolicy) -> Result<(), String> {
        if let PrimitivePolicy::RequireMapping(map) = &policy {
            for binding in map.0.gates.values() {
                self.check_mapping_hierarchy(&binding.cell)?;
            }
        }
        let mut core = self.core.write();
        if core.primitives.instantiated_logic {
            return Err(format!("{} already contains primitive logic", core.name));
        }
        if core.primitive.is_some() {
            return Err("cannot change a built-in definition".into());
        }
        core.primitives.policy = policy;
        core.primitives.policy_epoch += 1;
        core.primitives.mapped_definitions.clear();
        Ok(())
    }
    /// Returns the policy for this definition's own body.
    pub fn primitive_policy(&self) -> PrimitivePolicy {
        self.core.read().primitives.policy.clone()
    }

    /// Rejects a mapping that would make this definition its own descendant.
    fn check_mapping_hierarchy(&self, implementation: &ModDef) -> Result<(), String> {
        let mut pending = vec![implementation.core.clone()];
        let mut visited = HashSet::new();
        while let Some(core) = pending.pop() {
            if Arc::ptr_eq(&core, &self.core) {
                return Err(format!(
                    "recursive primitive mapping in {}",
                    self.get_name()
                ));
            }
            if visited.insert(Arc::as_ptr(&core)) {
                pending.extend(core.read().instances.values().cloned());
            }
        }
        Ok(())
    }

    /// Selects the parent's implementation even when passed a definition from a different parent.
    pub(crate) fn implementation(&self, op: &Primitive, width: usize) -> (ModDef, usize) {
        loop {
            let (policy, epoch) = {
                let core = self.core.read();
                if let Some(def) = core.primitives.mapped_definitions.get(&(op.clone(), width)) {
                    return (def.clone(), core.primitives.policy_epoch);
                }
                (core.primitives.policy.clone(), core.primitives.policy_epoch)
            };
            if op.is_wiring() || matches!(policy, PrimitivePolicy::Allow) {
                return (self.primitive_def(op.clone(), width), epoch);
            }
            let PrimitivePolicy::RequireMapping(map) = policy else {
                panic!("{} forbids primitive {op:?}", self.get_name());
            };
            let def = map
                .build_definition(self, op, width)
                .unwrap_or_else(|err| panic!("{}: {op:?} width {width}: {err}", self.get_name()));
            let mut core = self.core.write();
            if core.primitives.policy_epoch != epoch {
                continue;
            }
            let cached = core
                .primitives
                .mapped_definitions
                .entry((op.clone(), width))
                .or_insert(def);
            return (cached.clone(), epoch);
        }
    }
}
