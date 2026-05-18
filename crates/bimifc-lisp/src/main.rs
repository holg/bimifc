// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! bimifc-lisp CLI — AutoLISP scripting for IFC BIM models
//!
//! Usage:
//!   bimifc-lisp                            # REPL mode
//!   bimifc-lisp script.lsp                 # run a script
//!   bimifc-lisp --ifc model.ifc            # REPL with pre-loaded model
//!   bimifc-lisp --ifc model.ifc script.lsp # pre-load model + run script

use std::io::{self, BufRead, Write};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut ifc_path: Option<String> = None;
    let mut script_path: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--ifc" | "-i" => {
                i += 1;
                if i < args.len() {
                    ifc_path = Some(args[i].clone());
                } else {
                    eprintln!("Error: --ifc requires a file path");
                    std::process::exit(1);
                }
            }
            "--help" | "-h" => {
                println!("bimifc-lisp — AutoLISP scripting for IFC BIM models");
                println!();
                println!("Usage:");
                println!("  bimifc-lisp                            # REPL mode");
                println!("  bimifc-lisp script.lsp                 # run a script");
                println!("  bimifc-lisp --ifc model.ifc            # REPL with pre-loaded model");
                println!("  bimifc-lisp --ifc model.ifc script.lsp # pre-load + run script");
                return;
            }
            arg => {
                script_path = Some(arg.to_string());
            }
        }
        i += 1;
    }

    let mut interp = bimifc_lisp::create_ifc_interpreter();

    // Pre-load IFC model if specified
    if let Some(ref path) = ifc_path {
        let escaped = path.replace('\\', "\\\\").replace('"', "\\\"");
        let results = interp.run(&format!("(ifc-load \"{}\")", escaped));
        // Print any output from loading
        flush_output(&mut interp);
        if results.last() == Some(&acadlisp::Expr::Nil) {
            std::process::exit(1);
        }
    }

    if let Some(ref path) = script_path {
        // Script mode
        let code = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Error: cannot read '{}': {}", path, e);
                std::process::exit(1);
            }
        };
        interp.run(&code);
        flush_output(&mut interp);
    } else {
        // REPL mode
        repl(&mut interp);
    }
}

fn repl(interp: &mut acadlisp::Interpreter) {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    println!("bimifc-lisp REPL — type (quit) to exit");
    println!();

    loop {
        print!("BIMIFC> ");
        stdout.flush().unwrap();

        let mut line = String::new();
        if stdin.lock().read_line(&mut line).unwrap() == 0 {
            break; // EOF
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let results = interp.run(trimmed);
        flush_output(interp);

        for r in &results {
            if *r != acadlisp::Expr::Nil {
                println!("{}", format_expr(r));
            }
        }
    }
}

fn flush_output(interp: &mut acadlisp::Interpreter) {
    if !interp.output.is_empty() {
        print!("{}", interp.output.join(""));
        interp.output.clear();
    }
}

fn format_expr(expr: &acadlisp::Expr) -> String {
    match expr {
        acadlisp::Expr::Nil => "nil".to_string(),
        acadlisp::Expr::T => "T".to_string(),
        acadlisp::Expr::Integer(i) => i.to_string(),
        acadlisp::Expr::Real(r) => format!("{}", r),
        acadlisp::Expr::String(s) => format!("\"{}\"", s),
        acadlisp::Expr::Symbol(s) => s.clone(),
        acadlisp::Expr::Quote(e) => format!("'{}", format_expr(e)),
        acadlisp::Expr::List(items) => {
            let inner: Vec<String> = items.iter().map(format_expr).collect();
            format!("({})", inner.join(" "))
        }
    }
}
