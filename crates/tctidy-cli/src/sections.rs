//! The `tctidy --stdin-sections` path: format several raw ST sections in one run.
//!
//! The XAE Shell extension formats the sections of the POUs open in the editor
//! this way, so it starts one process for all of them. The request is one JSON
//! document on stdin and the response one JSON document on stdout, with a result
//! per section, in the request's order:
//!
//! ```text
//! {"sections":[{"source":"x:=1;","declaration":false,"filepath":"C:\\Plc\\FB_X.TcPOU"}]}
//! {"results":[{"output":"x := 1;"}]}
//! ```
//!
//! `filepath` is the file the section comes from (it need not exist): the
//! `.tctidy.toml` that applies to it is used, as with `--stdin-filepath`. A
//! section that cannot be formatted gets `{"error":"..."}` and does not stop the
//! others. All of stdin is read before anything is written, so a caller can
//! write the whole request, then read the response. A leading UTF-8 byte order
//! mark is skipped.

use std::io::{Read, Write};
use std::path::Path;
use std::process::ExitCode;

use serde::{Deserialize, Serialize};
use tctidy::{format_st, StSection};

use crate::config::ConfigResolver;
use crate::exit;

#[derive(Deserialize)]
struct Request {
    sections: Vec<Section>,
}

#[derive(Deserialize)]
struct Section {
    source: String,
    /// A `<Declaration>` section; otherwise an `<Implementation>` body.
    #[serde(default)]
    declaration: bool,
    #[serde(default)]
    filepath: Option<String>,
}

#[derive(Serialize)]
struct Response {
    results: Vec<Outcome>,
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum Outcome {
    Output(String),
    Error(String),
}

/// Read the request from stdin and write the response to stdout. A request that
/// is not valid is a usage error (exit 2) and writes nothing; otherwise the
/// response is complete, and the exit code is the most severe status of its
/// sections (an invalid configuration 2, a section that failed to format 3).
pub fn run(configs: &mut ConfigResolver) -> ExitCode {
    let mut bytes = Vec::new();
    if let Err(e) = std::io::stdin().read_to_end(&mut bytes) {
        eprintln!("tctidy: failed to read stdin: {e}");
        return exit::format_failed();
    }
    let request: Request = match serde_json::from_slice(crate::stdin::skip_bom(&bytes)) {
        Ok(request) => request,
        Err(e) => {
            eprintln!("tctidy: --stdin-sections: invalid request: {e}");
            return exit::usage_failed();
        }
    };

    let (mut any_error, mut any_config_error) = (false, false);
    let mut results = Vec::with_capacity(request.sections.len());
    for section in &request.sections {
        let opts = match configs.for_stdin(section.filepath.as_deref().map(Path::new)) {
            Ok(opts) => opts,
            Err(e) => {
                any_config_error = true;
                results.push(Outcome::Error(e));
                continue;
            }
        };
        let kind = if section.declaration {
            StSection::Declaration
        } else {
            StSection::Implementation
        };
        results.push(match format_st(&section.source, kind, &opts) {
            Ok(output) => Outcome::Output(output),
            Err(e) => {
                any_error = true;
                Outcome::Error(e.to_string())
            }
        });
    }

    let mut stdout = std::io::stdout().lock();
    let written =
        serde_json::to_writer(&mut stdout, &Response { results }).map_err(std::io::Error::from);
    if let Err(e) = written.and_then(|()| stdout.flush()) {
        eprintln!("tctidy: failed to write stdout: {e}");
        return exit::format_failed();
    }

    if any_config_error {
        exit::config_failed()
    } else if any_error {
        exit::format_failed()
    } else {
        exit::OK
    }
}
