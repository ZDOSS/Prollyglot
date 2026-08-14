use std::{env, fs, path::PathBuf, process::ExitCode};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let check = match env::args().skip(1).collect::<Vec<_>>().as_slice() {
        [] => false,
        [argument] if argument == "--check" => true,
        arguments => {
            return Err(format!(
                "usage: export-runtime-bindings [--check] (received {arguments:?})"
            ));
        }
    };
    let destination = destination();
    let generated = prollyglot_application_runtime::typescript_bindings();
    if check {
        let current = fs::read_to_string(&destination).map_err(|error| {
            format!(
                "runtime bindings are missing at {}: {error}",
                destination.display()
            )
        })?;
        if current != generated {
            return Err(
                "runtime bindings are stale; run `cargo run -p prollyglot-application-runtime --bin export-runtime-bindings`"
                    .to_owned(),
            );
        }
        println!("runtime bindings are current");
        return Ok(());
    }

    let parent = destination
        .parent()
        .ok_or_else(|| "runtime binding destination has no parent directory".to_owned())?;
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "could not create runtime binding directory {}: {error}",
            parent.display()
        )
    })?;
    fs::write(&destination, generated).map_err(|error| {
        format!(
            "could not write runtime bindings to {}: {error}",
            destination.display()
        )
    })?;
    println!("wrote {}", destination.display());
    Ok(())
}

fn destination() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/desktop/src/generated/runtime.ts")
}
