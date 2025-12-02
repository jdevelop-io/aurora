//! Beamfile discovery logic.

use std::path::{Path, PathBuf};

use miette::{Result, miette};

/// Default Beamfile name.
const BEAMFILE_NAME: &str = "Beamfile";

/// Finds the Beamfile by searching from the current directory upwards.
pub fn find_beamfile() -> Result<PathBuf> {
    find_beamfile_from(
        &std::env::current_dir().map_err(|e| miette!("Cannot get current directory: {}", e))?,
    )
}

/// Finds the Beamfile starting from the given directory.
pub fn find_beamfile_from(start: &Path) -> Result<PathBuf> {
    let mut current = start.to_path_buf();

    loop {
        let beamfile = current.join(BEAMFILE_NAME);

        if beamfile.exists() && beamfile.is_file() {
            return Ok(beamfile);
        }

        // Try parent directory
        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => {
                return Err(miette!(
                    "Beamfile not found in {} or any parent directory",
                    start.display()
                ));
            }
        }
    }
}

/// Returns the working directory for a Beamfile (its parent directory).
pub fn working_dir(beamfile_path: &Path) -> PathBuf {
    beamfile_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Returns the cache directory for a project.
pub fn cache_dir(beamfile_path: &Path) -> PathBuf {
    working_dir(beamfile_path).join(".aurora").join("cache")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_find_beamfile_in_current() {
        let dir = tempdir().unwrap();
        let beamfile = dir.path().join("Beamfile");
        fs::write(&beamfile, "# test").unwrap();

        let result = find_beamfile_from(dir.path()).unwrap();
        assert_eq!(result, beamfile);
    }

    #[test]
    fn test_find_beamfile_in_parent() {
        let dir = tempdir().unwrap();
        let beamfile = dir.path().join("Beamfile");
        fs::write(&beamfile, "# test").unwrap();

        let subdir = dir.path().join("subdir");
        fs::create_dir(&subdir).unwrap();

        let result = find_beamfile_from(&subdir).unwrap();
        assert_eq!(result, beamfile);
    }

    #[test]
    fn test_find_beamfile_not_found() {
        // Use the root directory - no Beamfile should be there
        let result = find_beamfile_from(Path::new("/"));
        // Searching from / should fail as there's no Beamfile at the root
        assert!(result.is_err());
    }

    #[test]
    fn test_working_dir() {
        let beamfile = Path::new("/some/project/Beamfile");
        assert_eq!(working_dir(beamfile), Path::new("/some/project"));
    }

    #[test]
    fn test_cache_dir() {
        let beamfile = Path::new("/some/project/Beamfile");
        assert_eq!(
            cache_dir(beamfile),
            Path::new("/some/project/.aurora/cache")
        );
    }
}
