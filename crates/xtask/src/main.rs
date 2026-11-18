use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const APP_ID: &str = "io.github.searchlinux.Search";

#[derive(Debug, Parser)]
#[command(
    name = "xtask",
    about = "Utilidades de compilación e instalación de Search"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Compila y valida el esquema GSettings en data/
    CompileSchemas {
        /// Directorio de esquemas a compilar
        #[arg(long, default_value = "data")]
        dir: PathBuf,
    },
    /// Instala el binario, recursos de escritorio, esquema GSettings y runtime CEF
    Install {
        /// Directorio raíz de destino para empaquetado (p. ej. DESTDIR)
        #[arg(long, default_value = "")]
        destdir: String,
        /// Prefijo de instalación (p. ej. /usr o /app en Flatpak)
        #[arg(long, default_value = "/usr")]
        prefix: PathBuf,
    },
}

fn workspace_root() -> Result<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .context("No se pudo resolver la raíz del workspace desde CARGO_MANIFEST_DIR")
}

fn compile_schemas(schema_dir: &Path) -> Result<()> {
    if !schema_dir.is_dir() {
        bail!(
            "El directorio de esquemas '{}' no existe",
            schema_dir.display()
        );
    }
    let status = Command::new("glib-compile-schemas")
        .arg("--strict")
        .arg(schema_dir)
        .status()
        .context("No se pudo ejecutar glib-compile-schemas")?;
    if !status.success() {
        bail!(
            "glib-compile-schemas falló en '{}' con código {:?}",
            schema_dir.display(),
            status.code()
        );
    }
    Ok(())
}

fn staged_root(destdir: &str, prefix: &Path) -> PathBuf {
    let trimmed_prefix = prefix.strip_prefix("/").unwrap_or(prefix);
    if destdir.is_empty() {
        Path::new("/").join(trimmed_prefix)
    } else {
        PathBuf::from(destdir).join(trimmed_prefix)
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)
        .with_context(|| format!("No se pudo crear el directorio '{}'", dst.display()))?;
    for entry in fs::read_dir(src)
        .with_context(|| format!("No se pudo leer el directorio '{}'", src.display()))?
    {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        let meta = fs::metadata(&src_path)
            .with_context(|| format!("No se pudo inspeccionar '{}'", src_path.display()))?;
        if meta.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path).with_context(|| {
                format!(
                    "No se pudo copiar '{}' a '{}'",
                    src_path.display(),
                    dst_path.display()
                )
            })?;
        }
    }
    Ok(())
}

fn install(root: &Path, destdir: &str, prefix: &Path) -> Result<()> {
    let dest_prefix = staged_root(destdir, prefix);
    let bin_dir = dest_prefix.join("bin");
    let lib_dir = dest_prefix.join("lib").join("search");
    let apps_dir = dest_prefix.join("share").join("applications");
    let schemas_dir = dest_prefix.join("share").join("glib-2.0").join("schemas");

    for dir in [&bin_dir, &lib_dir, &apps_dir, &schemas_dir] {
        fs::create_dir_all(dir).with_context(|| format!("No se pudo crear '{}'", dir.display()))?;
    }

    let desktop_name = format!("{APP_ID}.desktop");
    let desktop_src = root.join("data").join(&desktop_name);
    fs::copy(&desktop_src, apps_dir.join(&desktop_name))
        .with_context(|| format!("No se pudo instalar '{}'", desktop_src.display()))?;

    let schema_name = format!("{APP_ID}.gschema.xml");
    let schema_src = root.join("data").join(&schema_name);
    fs::copy(&schema_src, schemas_dir.join(&schema_name))
        .with_context(|| format!("No se pudo instalar '{}'", schema_src.display()))?;
    compile_schemas(&schemas_dir)?;

    let release_bin = root.join("target").join("release").join("search");
    if release_bin.is_file() {
        let installed_bin = lib_dir.join("search");
        fs::copy(&release_bin, &installed_bin)
            .with_context(|| format!("No se pudo instalar '{}'", release_bin.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&installed_bin, fs::Permissions::from_mode(0o755))?;
        }
        let wrapper_bin = bin_dir.join("search");
        if wrapper_bin.exists() {
            fs::remove_file(&wrapper_bin)?;
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink("../lib/search/search", &wrapper_bin)
            .with_context(|| format!("No se pudo crear enlace '{}'", wrapper_bin.display()))?;
    }

    if let Ok(cef_dir) = std::env::var("CEF_PATH") {
        let cef_path = PathBuf::from(cef_dir);
        if cef_path.is_dir() {
            for asset in [
                "libcef.so",
                "libEGL.so",
                "libGLESv2.so",
                "libvk_swiftshader.so",
                "libvulkan.so.1",
                "vk_swiftshader_icd.json",
                "icudtl.dat",
                "resources.pak",
                "chrome_100_percent.pak",
                "chrome_200_percent.pak",
                "v8_context_snapshot.bin",
            ] {
                let src = cef_path.join(asset);
                if src.is_file() {
                    fs::copy(&src, lib_dir.join(asset)).with_context(|| {
                        format!("No se pudo copiar activo CEF '{}'", src.display())
                    })?;
                }
            }
            let locales_src = cef_path.join("locales");
            if locales_src.is_dir() {
                copy_dir_recursive(&locales_src, &lib_dir.join("locales"))?;
            }
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = workspace_root()?;
    match cli.command {
        Commands::CompileSchemas { dir } => {
            let schema_dir = if dir.is_absolute() {
                dir
            } else {
                root.join(dir)
            };
            compile_schemas(&schema_dir)
        }
        Commands::Install { destdir, prefix } => install(&root, &destdir, &prefix),
    }
}
