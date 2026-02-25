// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, VecDeque};
use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

use topstitch::*;

fn parse_jobs_in_pool(sources: Vec<(&str, &str)>, n_workers: usize) -> HashMap<String, ModDef> {
    assert!(n_workers > 0, "n_workers must be at least 1");

    let owned_sources = sources
        .into_iter()
        .map(|(module_name, source)| (module_name.to_string(), source.to_string()))
        .collect::<Vec<_>>();

    let total = owned_sources.len();
    let queue = Arc::new(Mutex::new(VecDeque::from(owned_sources)));
    let (tx, rx) = mpsc::channel::<Result<(String, ModDef), String>>();

    let mut handles = Vec::new();
    for _ in 0..n_workers {
        let queue = Arc::clone(&queue);
        let tx = tx.clone();

        handles.push(thread::spawn(move || {
            loop {
                let job = {
                    let mut q = queue.lock().unwrap();
                    q.pop_front()
                };
                let Some((module_name, source)) = job else {
                    break;
                };

                let result = std::panic::catch_unwind(|| {
                    let source_file = slang_rs::str2tmpfile(&source)
                        .unwrap_or_else(|err| panic!("failed to create temp source file: {err}"));
                    let source_path = source_file
                        .path()
                        .to_str()
                        .unwrap_or_else(|| panic!("temp source path is not valid UTF-8"))
                        .to_string();
                    let source_paths = [source_path.as_str()];
                    let cfg = ParserConfig {
                        sources: &source_paths,
                        ignore_unknown_modules: true,
                        skip_unsupported: false,
                        extra_arguments: &["--threads", "1"],
                        ..Default::default()
                    };
                    ModDef::from_verilog_with_config(&module_name, &cfg)
                })
                .map(|md| (module_name.clone(), md))
                .map_err(|_| format!("parse panic in worker for module '{module_name}'"));

                if tx.send(result).is_err() {
                    break;
                }
            }
        }));
    }
    drop(tx);

    let mut parsed = HashMap::new();
    for _ in 0..total {
        match rx.recv().unwrap() {
            Ok((module_name, md)) => {
                let existing = parsed.insert(module_name.clone(), md);
                assert!(
                    existing.is_none(),
                    "duplicate parsed module name '{module_name}'"
                );
            }
            Err(msg) => panic!("{msg}"),
        }
    }

    for handle in handles {
        handle.join().expect("worker thread panicked unexpectedly");
    }

    parsed
}

fn emit_jobs_in_pool(mod_defs: Vec<(String, ModDef)>, n_workers: usize) -> HashMap<String, String> {
    assert!(n_workers > 0, "n_workers must be at least 1");

    let total = mod_defs.len();
    let queue = Arc::new(Mutex::new(VecDeque::from(mod_defs)));
    let (tx, rx) = mpsc::channel::<Result<(String, String), String>>();

    let mut handles = Vec::new();
    for _ in 0..n_workers {
        let queue = Arc::clone(&queue);
        let tx = tx.clone();

        handles.push(thread::spawn(move || {
            loop {
                let job = {
                    let mut q = queue.lock().unwrap();
                    q.pop_front()
                };
                let Some((module_name, mod_def)) = job else {
                    break;
                };

                let result = std::panic::catch_unwind(AssertUnwindSafe(|| mod_def.emit(true)))
                    .map(|emitted| (module_name.clone(), emitted))
                    .map_err(|_| format!("emit panic in worker for module '{module_name}'"));

                if tx.send(result).is_err() {
                    break;
                }
            }
        }));
    }
    drop(tx);

    let mut emitted = HashMap::new();
    for _ in 0..total {
        match rx.recv().unwrap() {
            Ok((module_name, verilog)) => {
                let existing = emitted.insert(module_name.clone(), verilog);
                assert!(
                    existing.is_none(),
                    "duplicate emitted module name '{module_name}'"
                );
            }
            Err(msg) => panic!("{msg}"),
        }
    }

    for handle in handles {
        handle.join().expect("worker thread panicked unexpectedly");
    }

    emitted
}

#[test]
fn test_parallel_parse() {
    const LETTERS: &[&str] = &["A", "B", "C", "D"];
    let mut verilog = Vec::new();

    for letter in LETTERS {
        verilog.push(format!(
            "module {letter}(input wire [7:0] in, output wire [7:0] out); endmodule"
        ));
    }

    let parsed = parse_jobs_in_pool(
        LETTERS
            .iter()
            .zip(verilog.iter())
            .map(|(letter, verilog)| (*letter, verilog.as_str()))
            .collect(),
        4,
    );

    let top = ModDef::new("Top");

    for letter in LETTERS {
        top.instantiate(&parsed[*letter], None, None)
            .unused_and_tieoff(0);
    }

    let expected = format!(
        "\
module Top;
{}endmodule
",
        LETTERS
            .iter()
            .map(|letter| {
                format!("  {letter} {letter}_i (\n    .in(8'h00),\n    .out()\n  );\n")
            })
            .collect::<Vec<_>>()
            .join("")
    );

    assert_eq!(top.emit(true), expected);
}

#[test]
fn test_parallel_emit() {
    const LETTERS: &[&str] = &["A", "B", "C", "D"];
    let mut mod_defs = Vec::new();

    for letter in LETTERS {
        let mod_def = ModDef::new(letter);
        mod_def.add_port("in", IO::Input(8)).unused();
        mod_def.add_port("out", IO::Output(8)).tieoff(0);
        mod_defs.push((letter.to_string(), mod_def));
    }

    let emitted = emit_jobs_in_pool(mod_defs, 4);

    let emitted_in_order = LETTERS
        .iter()
        .map(|letter| emitted[*letter].as_str())
        .collect::<Vec<_>>()
        .join("\n");

    let expected_in_order = LETTERS
        .iter()
        .map(|letter| {
            format!(
                "\
module {letter}(
  input wire [7:0] in,
  output wire [7:0] out
);
  assign out = 8'h00;
endmodule
"
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert_eq!(emitted_in_order, expected_in_order);
}
