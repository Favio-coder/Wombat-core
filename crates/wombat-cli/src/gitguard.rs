//! R19: una clave privada no se escribe dentro de un árbol versionado.
//!
//! No alcanza con mirar si la ruta está bajo el repo actual. Lo que importa es
//! si *algún* ancestro de la ruta es un repositorio git, porque el accidente que
//! R19 previene es el de siempre: `git add -A` barriendo una clave que alguien
//! dejó en una carpeta que resultó estar versionada.

use std::path::{Path, PathBuf};

use crate::error::CliError;

/// Falla si `path` cae dentro de un repositorio git.
///
/// Mira el primer ancestro que exista en disco y sube desde ahí, porque la ruta
/// de la clave normalmente todavía no existe: es la que estamos por crear.
pub fn refuse_inside_git_repo(path: &Path) -> Result<(), CliError> {
    let start = nearest_existing_ancestor(path);
    let absolute = match start.as_deref().map(std::fs::canonicalize) {
        Some(Ok(resolved)) => resolved,
        // Sin un ancestro resoluble no hay nada que comprobar: la escritura
        // fallará después por sí sola con un error de I/O más claro.
        _ => return Ok(()),
    };

    let mut current: Option<&Path> = Some(absolute.as_path());
    while let Some(dir) = current {
        // `.git` es un directorio en un clon normal y un *archivo* en un
        // worktree o un submódulo. Las dos formas cuentan.
        if dir.join(".git").exists() {
            return Err(CliError::KeyInsideGitRepo {
                path: path.display().to_string(),
                repo: dir.display().to_string(),
            });
        }
        current = dir.parent();
    }
    Ok(())
}

/// El ancestro más cercano de `path` que exista en disco, `path` incluido.
fn nearest_existing_ancestor(path: &Path) -> Option<PathBuf> {
    let mut current = Some(path);
    while let Some(candidate) = current {
        if candidate.exists() {
            return Some(candidate.to_path_buf());
        }
        current = candidate.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_a_path_inside_this_repo() {
        let inside = Path::new(env!("CARGO_MANIFEST_DIR")).join("target-key-should-be-refused.key");
        let err = refuse_inside_git_repo(&inside).unwrap_err();
        assert!(
            matches!(err, CliError::KeyInsideGitRepo { .. }),
            "se esperaba KeyInsideGitRepo, llegó {err}"
        );
    }

    #[test]
    fn refuses_a_path_several_levels_below_the_repo_root() {
        let deep = Path::new(env!("CARGO_MANIFEST_DIR")).join("a/b/c/demo.key");
        assert!(refuse_inside_git_repo(&deep).is_err());
    }

    #[test]
    fn accepts_a_path_in_a_directory_that_is_not_versioned() {
        let outside = tempfile::tempdir().unwrap();
        let key = outside.path().join("demo.key");
        refuse_inside_git_repo(&key).expect("un temporal no versionado debe aceptarse");
    }

    #[test]
    fn refuses_when_the_git_marker_is_a_file_as_in_a_worktree() {
        let root = tempfile::tempdir().unwrap();
        // Un worktree tiene `.git` como archivo con un gitdir: adentro.
        std::fs::write(root.path().join(".git"), "gitdir: /otro/lado\n").unwrap();
        let key = root.path().join("sub/demo.key");
        let err = refuse_inside_git_repo(&key).unwrap_err();
        assert!(matches!(err, CliError::KeyInsideGitRepo { .. }), "{err}");
    }
}
