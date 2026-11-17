#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Prove combinational SV equivalence. Optionally compare unflattened wiring and named nets."""

import argparse
import json
import re
import shutil
import subprocess
import tempfile
from collections import defaultdict
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("reference", type=Path)
    parser.add_argument("candidate", type=Path)
    parser.add_argument("--top", required=True)
    parser.add_argument(
        "--structure",
        action="store_true",
        help="compare structural module and instance interfaces and pin connectivity",
    )
    parser.add_argument(
        "--net",
        action="append",
        default=[],
        metavar="MODULE.NET[=INSTANCE.PORT[MSB:LSB]]",
        help="compare a named net and optionally require an instance port or slice binding",
    )
    args = parser.parse_args()
    if not re.fullmatch(r"[A-Za-z_][A-Za-z_0-9$]*", args.top):
        parser.error("--top must be a simple SystemVerilog identifier")
    names = defaultdict(dict)
    for request in args.net:
        wire, separator, endpoint = request.partition("=")
        parts = wire.split(".")
        pin = None
        if separator:
            match = re.fullmatch(r"([^.\[\]]+)\.([^.\[\]]+)(?:\[(\d+):(\d+)\])?", endpoint)
            if not match:
                parser.error("--net expects INSTANCE.PORT or INSTANCE.PORT[MSB:LSB] after '='")
            instance, port, msb, lsb = match.groups()
            pin = (instance, port, (int(msb), int(lsb)) if msb is not None else None)
        if len(parts) != 2 or not all(parts):
            parser.error("--net expects MODULE.NET or MODULE.NET=INSTANCE.PORT")
        names[parts[0]][parts[1]] = pin

    with tempfile.TemporaryDirectory() as directory:
        work = Path(directory)
        script = []
        for label, source in [("gold", args.reference), ("gate", args.candidate)]:
            shutil.copyfile(source, work / (label + ".sv"))
            script.extend(
                [
                    "read_verilog -sv " + label + ".sv",
                    "hierarchy -simcheck -keep_portwidths -top " + args.top,
                ]
            )
            script.extend(
                [
                    "proc",
                    "write_json " + label + ".json",
                    "prep -flatten -top " + args.top,
                    "check -assert",
                    "select -assert-any o:*",
                    "select -assert-none t:$any* t:$all* t:$initstate",
                    "design -stash " + label,
                ]
            )
        script.extend(
            [
                "design -copy-from gold -as gold " + args.top,
                "design -copy-from gate -as gate " + args.top,
                "miter -equiv -flatten gold gate compare",
                "hierarchy -top compare",
                "sat -verify -prove trigger 0 -show-inputs -show-outputs compare",
            ]
        )
        (work / "compare.ys").write_text("\n".join(script))
        result = subprocess.run(
            ["yosys", "-Q", "-T", "-s", "compare.ys"],
            cwd=work,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            universal_newlines=True,
        )
        if result.returncode:
            raise SystemExit("\n".join(result.stdout.splitlines()[-60:]))
        gold, gate = (
            json.loads((work / (tag + ".json")).read_text())["modules"]
            for tag in ["gold", "gate"]
        )
        if signature(gold[args.top]) != signature(gate[args.top]):
            raise SystemExit("Top-level port names, directions or shapes differ")
        if args.structure or names:
            if set(gold) != set(gate) or not names.keys() <= gold.keys():
                raise SystemExit(
                    "Module definitions differ or a requested module is missing"
                )
            for module in gold:
                reference = wiring(gold[module], names.get(module, {}), module)
                candidate = wiring(gate[module], names.get(module, {}), module)
                for kind in reference:
                    if reference[kind] != candidate[kind]:
                        changed = sorted(
                            set(reference[kind]) ^ set(candidate[kind]), key=str
                        )
                        raise SystemExit(
                            "{}: {} differ: {}".format(module, kind, changed[:4])
                        )


def shape(net):
    return (
        len(net["bits"]),
        net.get("offset", 0),
        net.get("upto", 0),
        net.get("signed", 0),
    )


def signature(module):
    return {
        (name, port["direction"], shape(port)) for name, port in module["ports"].items()
    }


def wiring(module, names, module_name):
    """Compare physical endpoints by HDL name and bit index, independent of Yosys's local bit IDs."""
    groups = defaultdict(list)
    constants = []
    cells = []
    wires = []

    def pin(label, bits, offset=0):
        for index, bit in enumerate(bits, offset):
            point = (label, index)
            if isinstance(bit, int):
                groups[bit].append(point)
            else:
                constants.append((point, bit))

    for name, port in module["ports"].items():
        pin(("port", name), port["bits"])
    for name, cell in module["cells"].items():
        cells.append((name, cell["type"]))
        for port, bits in cell["connections"].items():
            pin(("instance", name, port), bits)
    for name, endpoint in names.items():
        if name not in module["netnames"]:
            raise SystemExit("Missing requested net {}.{}".format(module_name, name))
        net = module["netnames"][name]
        selected = net["bits"]
        lsb = 0
        if endpoint:
            instance, port, interval = endpoint
            bits = module["cells"].get(instance, {}).get("connections", {}).get(port)
            msb, lsb = interval if interval is not None else (len(selected) - 1, 0)
            selected = selected[lsb : msb + 1]
            if (
                bits is None
                or len(net["bits"]) != len(bits)
                or not 0 <= lsb <= msb < len(bits)
                or selected != bits[lsb : msb + 1]
                or net.get("offset", 0)
                or net.get("upto", 0)
            ):
                raise SystemExit(
                    "{}.{} does not bind {}.{} in bit order".format(
                        module_name, name, instance, port
                    )
                )
        wires.append((name, shape(net)))
        pin(("net", name), selected, lsb)
    # An unused child output can be left open or bound to a fresh unused signal.
    nets = {tuple(sorted(points)) for points in groups.values() if len(points) > 1}
    return {
        "ports": signature(module),
        "instances": set(cells),
        "named nets": set(wires),
        "pin connections": nets,
        "constant pins": set(constants),
    }


if __name__ == "__main__":
    main()
