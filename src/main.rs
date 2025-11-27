use anyhow::Context;
use clap::{Parser, Subcommand};
use flate2::read::ZlibDecoder;
use std::ffi::CStr;
use std::fs;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};

/// Git directories.
const GIT_DIR: &str = ".git";
const GIT_OBJECT_DIR: &str = ".git/objects";
const GIT_REF_DIR: &str = ".git/refs";
const GIT_HEAD: &str = ".git/HEAD";

/// Application arguments.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

/// Git sub commands (init, add, commit, push, etc.)
#[derive(Subcommand, Debug)]
enum Command {
    /// Initialize a new git repository.
    Init,

    /// Cat file contents in object.
    CatFile {
        #[clap(short = 'p')]
        pretty_print: bool,

        object_hash: String,
    },
}

enum Kind {
    Blob,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Command::Init => {
            fs::create_dir(GIT_DIR).unwrap();
            fs::create_dir(format!("{GIT_OBJECT_DIR}")).unwrap();
            fs::create_dir(format!("{GIT_REF_DIR}")).unwrap();
            fs::write(format!("{GIT_HEAD}"), "ref: refs/heads/main\n").unwrap();
            println!("Initialized git directory")
        }
        Command::CatFile {
            pretty_print,
            object_hash,
        } => {
            anyhow::ensure!(
                pretty_print,
                "mode '-p' should be give and we don't support other modes."
            );
            anyhow::ensure!(
                object_hash.len() == 40,
                "object hash must be 40 characters long"
            );
            //TODO: support shortest unique object hash
            let file = File::open(format!(
                "{GIT_OBJECT_DIR}/{}/{}",
                &object_hash[..2],
                &object_hash[2..]
            ))
            .context(format!("Failed to open {GIT_OBJECT_DIR}"))?;

            let zlib = ZlibDecoder::new(file);
            let mut zlib = BufReader::new(zlib);
            let mut buf = Vec::new();
            zlib.read_until(0, &mut buf)
                .context(format!("Failed to read header from {GIT_OBJECT_DIR}"))?;
            let header = CStr::from_bytes_until_nul(&buf)
                .expect("there is only one nul and that is at the end - this should not fail");
            let header = header.to_str().context("header is valid utf-8")?;

            let Some((kind, size)) = header.split_once(' ') else {
                anyhow::bail!(
                    "corrupted {GIT_OBJECT_DIR}! header doesn't start with a known known kind: '{header}'"
                )
            };

            let kind = match kind {
                "blob" => Kind::Blob,
                _ => anyhow::bail!("kind {kind} is not implemented yet"),
            };

            let Some(size) = header.strip_prefix("blob ") else {
                anyhow::bail!(
                    "corrupted {GIT_OBJECT_DIR}! header doesn't start with 'blob ': '{header}'"
                )
            };
            let size = size
                .parse::<usize>()
                .context("failed to parse size: {size}")?;

            buf.clear();
            buf.resize(size, 0); // resize the buffer vector to the size of the file with 0 as each element (non-performant but meh for now)

            zlib.read_exact(&mut buf[..])
                .context("failed to read true contents of {GIT_OBJECT_DIR} file")?;
            let n = zlib
                .read(&mut [0])
                .context("validate EOF in {GIT_OBJECT_DIR} file")?;
            anyhow::ensure!(
                n == 0,
                "expected EOF in {GIT_OBJECT_DIR} file, had {n} trailing bytes"
            );
            let stdout = std::io::stdout();
            let mut stdout = stdout.lock();

            match kind {
                Kind::Blob => stdout
                    .write_all(&buf)
                    .context("failed to write contents to stdout")?,
            }
        }
    }

    Ok(())
}
