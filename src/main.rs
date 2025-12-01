use anyhow::Context;
use clap::{Parser, Subcommand};
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use sha1::{Digest, Sha1};
use std::ffi::CStr;
use std::fs;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};

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

    HashObject {
        #[clap(short = 'w')]
        write: bool,

        file_path: PathBuf,
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

            let Some((kind, _)) = header.split_once(' ') else {
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
                .parse::<u64>()
                .context("failed to parse size: {size}")?;

            let mut zlib = LimitReader {
                reader: zlib,
                limit: size as usize,
            };

            match kind {
                Kind::Blob => {
                    let n = std::io::copy(&mut zlib, &mut buf)
                        .context("write .git/objects file to stdout")?;
                    anyhow::ensure!(
                        n == size,
                        "{GIT_OBJECT_DIR} was not expected size (expected: {size} actual: {n}"
                    );
                }
            }
        }
        Command::HashObject { write, file_path } => {
            fn write_blob<W: Write>(file: &Path, writer: W) -> anyhow::Result<String> {
                let stat = fs::metadata(&file).with_context(|| {
                    format!(
                        "Failed to read metadata from {} - stat: {}",
                        GIT_OBJECT_DIR,
                        file.display()
                    )
                })?;

                let writer = ZlibEncoder::new(writer, Compression::default());
                let mut writer = HashWriter {
                    writer,
                    hasher: Sha1::new(),
                };
                write!(writer, "blob")?;
                write!(writer, "{}\0", stat.len())?;
                let mut file = File::open(file)
                    .with_context(|| format!("failed to open file {}", file.display()))?;
                std::io::copy(&mut file, &mut writer)
                    .context("failed to copy file to encoder - stream file into blob")?;

                let _ = writer.writer.finish()?;
                let hash = writer.hasher.finalize();

                Ok(hex::encode(hash))
            }

            let hash = if write {
                let tmp = "temporary";
                let hash = write_blob(
                    &file_path,
                    File::create(tmp).context("failed to construct temporary file for blob")?,
                )?;
                fs::rename(
                    tmp,
                    format!("{GIT_OBJECT_DIR}/{}/{}", &hash[..2], &hash[2..]),
                )
                .context("failed to move temporary file to object directory")?;
                hash
            } else {
                write_blob(&file_path, std::io::sink())?
            };
            println!("{}", hash);
        }
    }

    Ok(())
}

struct LimitReader<R> {
    reader: R,
    limit: usize,
}

impl<R> Read for LimitReader<R>
where
    R: Read,
{
    fn read(&mut self, mut buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.len() > self.limit {
            buf = &mut buf[..self.limit + 1];
        }
        let n = self.reader.read(buf)?;
        if n > self.limit {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "limit exceeded",
            ));
        }
        self.limit -= n;
        Ok(n)
    }
}

struct HashWriter<W> {
    writer: W,
    hasher: Sha1,
}

impl<W> Write for HashWriter<W>
where
    W: Write,
{
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        use sha1::digest::Digest;
        let n = self.writer.write(buf)?;
        Digest::update(&mut self.hasher, &buf[..n]);
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}
